# Native executable support

## Goal

Allow AppBento to manage standalone native Linux executables (ELF binaries) alongside AppImages, using the existing launch permission review, Bubblewrap sandbox, private app data, launch logs, and desktop menu integration.

The first release targets GUI applications, including applications extracted from an archive next to their support files. It focuses on local import and sandboxed launching; executable updates are a separate phase.

## Scope

### First release

- Explicitly import a native Linux ELF executable for a supported host architecture, including files without an extension.
- Register the executable at its existing location. Do not move or copy it into the managed directory.
- Target GUI applications. Console output is recorded in launch logs; AppBento does not open a terminal.
- Reuse the existing permission review, arguments, environment variables, direct launch option, and Bubblewrap controls.
- Keep "Run without restrictions" as the initial launch mode for both AppImages and executables. The first launch of every new entry always requires a permission review.
- The default Bubblewrap permissions for an executable include read-only access to its containing folder and subfolders, so support files extracted alongside it are available inside the sandbox.
- Support private persistent data, custom icons, desktop menu entries, launch logs, and configuration export/restore.
- Detect executable content changes and require permission review again, using the same approach as AppImages.
- Keep existing AppImage import, scanning, managed storage, updates, and launch behavior working.

### Deferred

- Shell, Python, and other interpreted scripts, including launch scripts as an application's entry point.
- Reviewing or pinning support files. Only the executable itself is covered by the reviewed digest.
- Archive extraction, installers, dependency installation, and terminal application integration.
- Automatic discovery of arbitrary executables in scanned folders.
- Generic executable URL downloads, managed relocation, and automatic updates.
- Relocating an existing entry after its file is moved or renamed.

A dynamically linked binary can use compatible libraries from the existing system runtime mounts and from its granted folder. Its program loader (`PT_INTERP`) must be installed on the host. AppBento will not install missing dependencies.

### Known limits

- An entry's identity is its file path, and its private data location derives from that identity (`sandbox.rs:393`). Moving or renaming the file makes the entry unusable. Adding the file from its new location creates a new entry with a new review, new permissions, and new private data. Error messages and removal copy must say this instead of suggesting a folder scan.
- Support files in the granted folder are not part of the reviewed digest. They can change without triggering a new review. The review page and README must state this.
- Command-line tools can be imported and launched, but their output is visible only in launch logs.

## Current architecture

The application already separates Tauri commands, library lifecycle operations, persistence, sandbox planning, and process monitoring. A focused extraction is sufficient; a general plugin framework is unnecessary.

| Area | Current behavior | Required change |
| --- | --- | --- |
| `src-tauri/src/models.rs` | `InstalledApp` stores a file path, optional update configuration, launch settings, and reviewed digest | Add an explicit app kind with a backward-compatible default |
| `src-tauri/src/appimage.rs` | Opens a regular non-symlink file and validates AppImage headers and architecture | Move safe file opening out; keep AppImage format validation |
| `src-tauri/src/lifecycle.rs` | Preparation, discovery, scanning, removal, and review hashing assume AppImages | Dispatch validation and identity by kind; enforce removal ownership by kind; skip kind mismatches during scans |
| `src-tauri/src/integration.rs` | Local import and managed integration both assume AppImages | Add in-place executable registration; reject managed moves for executables before integration starts |
| `src-tauri/src/sandbox.rs` | Shared policy, Type 2 validation, AppImage launch arguments, and file digests live together | Separate policy construction from format-specific launch targets; move digests out; add executable default permissions |
| `src-tauri/src/sandbox_launch.rs` | Snapshots, digest checks, and descriptor mounts assume `/app/application.AppImage` | Use the launch target destination carried by the plan |
| `src-tauri/src/menu_integration.rs` | Enabling a menu entry validates an AppImage | Validate using the saved kind |
| `src-tauri/src/configuration.rs` | Restore expects AppImages, copies ownership flags from the backup, and always queues a permission change | Restore both kinds, enforce executable invariants, never change executable file permissions; backup format version 2 |
| `src-tauri/src/apps.rs`, `app_identity.rs`, `embedded_icon.rs` | Import runs embedded AppImage metadata and icon discovery | Skip embedded discovery for executables; reuse filename identity |
| `src-tauri/src/lib.rs`, `updates.rs`, `update_download.rs`, `replacement.rs` | Update entry points and artifact replacement assume AppImages | Reject executable updates before any download or replacement |
| `src-tauri/build.rs`, `capabilities/default.json` | Lists the allowed commands | Register the new default-permissions command |
| `src/lib/api.ts`, `types.ts`, `library-actions.ts`, `sandbox.ts`, dialogs, `launch-fix-prompt.ts`, `preview.ts` | Pickers, the location prompt, review defaults, and copy assume AppImages | Add executable import, kind-aware review defaults, and format-neutral copy where behavior is shared |
| `README.md` | Describes an AppImage manager and a Type 2-only sandbox | Document executables, the default folder grant, and the limits above |

`lifecycle.rs` (422 lines) and `sandbox.rs` (433 lines) already exceed the repository's ~400-line guideline. New dispatch code goes into the new modules; moving digest code out of `sandbox.rs` is part of step 2.

## Proposed implementation

Implement the steps in this order. Executable entries become reachable only in step 5, after launch handling, backups, and update guards support them, so no intermediate commit can create an entry that falls through to AppImage-only code.

### 1. Add app kind and compatibility handling

- Add a Rust `AppKind` enum serialized as `appimage` or `binary`, and a `kind` field on `InstalledApp`. The value `binary` avoids confusion with the existing `InstalledApp.executable` permission flag and `executablePromptId`; since the value is persisted in `library.json` and backups, it is hard to rename later. User-facing copy can still say "executable".
- Use `#[serde(default)]` with AppImage as the default so existing library records load without a manual migration.
- Add the equivalent TypeScript union and field. Update fixtures and preview data to represent both kinds.
- Preserve app IDs, paths, launch configuration, reviewed digests, and private data identities for existing records. Adding a kind must not reset existing reviews.
- Reject unknown kinds rather than treating them as binaries. `library.json` is a single map read by `persistence::read` (`persistence.rs:41`), so one unknown kind fails loading the whole library. Accept this fail-closed behavior, which is reachable only after a downgrade from a future version, but make the error name the invalid field and ensure no write path overwrites `library.json` while it fails to load.
- Define the binary entry invariants in one small check: `integrated` is false, `source_path` is unset, and `update_config` is unset. Use it on every path that creates or restores an entry.

Validation after this step: old library JSON loads as AppImage; both kinds round-trip; unknown kinds fail clearly without the file being rewritten.

### 2. Extract shared file handling and add binary validation

Introduce two modules, wired through `#[path]` from `lifecycle.rs` the way `appimage.rs` is today and re-exported where `sandbox.rs` and `configuration.rs` need them:

- `app_file.rs`: shared absolute UTF-8 path checks, control-character rejection, non-symlink checks, safe open flags, format detection, kind-aware opening, and content hashing through an opened handle. It replaces `appimage_digest` and `digest_file` in `sandbox.rs`.
- `executable.rs`: native ELF validation and program-loader checks.

Use an enum and ordinary functions rather than a trait registry. Keep AppImage magic, architecture, and Type 1/Type 2 handling in `appimage.rs`. Sandbox support remains Type 2 only.

Format detection:

- `AI` magic at offset 8 means AppImage regardless of the type byte. Reject unsupported AppImage types as AppImages; never fall back to binary handling for them.
- Otherwise validate a binary without running it:
  - `\x7fELF` magic, 64-bit class, little-endian data, and ELF version 1. Big-endian is rejected even though big-endian aarch64 shares machine number 183.
  - OS ABI 0 (System V) or 3 (GNU/Linux). Static glibc binaries use 3, so accepting only 0 would reject valid files.
  - Type `ET_EXEC` or `ET_DYN`. Reject relocatable objects and core files. Shared libraries cannot be told apart from position-independent executables by their headers; accept them and let them fail at launch.
  - Machine number matching the host (62 for x86_64, 183 for aarch64).
  - A bounded program-header parse: entry size 56, a capped entry count, and a table inside the file. If `PT_INTERP` is present, it must be a NUL-terminated absolute path no longer than 4096 bytes, and the loader must exist on the host. Report a missing loader specifically, for example "requires the program loader /lib/ld-musl-x86_64.so.1, which is not installed".
  - Files starting with `#!` are rejected with a message that scripts are not supported yet. Other formats get a specific "not a supported Linux executable" error.
- Keep the executable permission bit separate from format validation so the existing make-executable confirmation still works.
- Make missing-file and access errors kind-aware. The current AppImage message suggests scanning the folder (`appimage.rs:68`), which never finds binaries. For binaries, say that a moved or renamed file must be added again from its new location and the old entry removed.
- Replace shared uses of `appimage_digest` with kind-aware opening followed by common hashing. Keep explicitly AppImage-only update paths validated as AppImages.
- Derive a binary's name and version with the existing `app_identity::resolve` using empty embedded metadata. It already strips architecture suffixes and reads versions from file names, so `executable.rs` needs no identity code. Skip `read_identity` and icon discovery through `embedded_icon`, which open the file as an AppImage and run `unsquashfs`. Never run an imported program to discover metadata.

First perform the shared extraction with existing AppImage behavior preserved, then add binary handling. Verify each step independently.

### 3. Separate launch targets from Bubblewrap policy

Keep one implementation of namespaces, system runtime/config mounts, devices, network permissions, folder grants, and private home handling. Format-specific preparation supplies the application mount destination and command prefix:

| Kind | Sandbox mount destination | Command prefix |
| --- | --- | --- |
| AppImage | `/app/application.AppImage` | `/app/application.AppImage --appimage-extract-and-run` |
| Binary inside an active folder grant | Its original path (`file_path`) | Its original path |
| Binary outside every folder grant | `/app/<file_name>` | `/app/<file_name>` |

A binary is inside a grant when its parent folder equals or lies within an active folder grant. Mounting the pinned snapshot over its original path is required for the default folder grant to work. With Bubblewrap 0.13.1, a binary run from `/app` could not load a library stored next to it through `$ORIGIN` (`error while loading shared libraries`), while the same snapshot mounted over its original path inside the granted folder loaded it. Executable-relative resources behave the same way. Both destinations also keep the real file name in `argv[0]` and `/proc/self/exe`, which multi-call binaries and toolkits that derive the program name from `argv[0]` rely on. Because grants already pass `validate_grant`, an original-path destination never lies in reserved runtime, `/app`, `/home/app`, or AppBento policy locations.

- Keep the application mount after the folder grant mounts (as `sandbox.rs:356` does today) so the reviewed snapshot overlays the file inside the granted folder.
- Add `sandbox::default_permissions(app)`. It returns the existing defaults; for binaries it adds a read-only grant on the canonical parent folder when `validate_grant` accepts it and adds nothing otherwise. Examples of no grant: a binary directly in the home folder (the home contains the AppBento policy directory) or in `/usr/local/bin` (reserved). Users can switch the grant to read/write for portable apps that write next to their binary. A binary saved directly in a broad folder such as `~/Downloads` grants that whole folder; the review page shows the grant, and users can remove it.
- Retain the Type 2 check for sandboxed AppImages. Do not apply that check or the extraction argument to binaries.
- If a binary's program loader is not inside the mounted system runtime directories or its covering grant, fail during planning with a clear error. Without this, Bubblewrap reports `bwrap: execvp /app/application: No such file or directory`, which reads as a missing mount.
- Append configured arguments and URI arguments as individual arguments, preserving existing ordering. Do not introduce a shell command string.
- Preserve the sandbox working directory `/home/app`, environment handling, and permission choices. A custom working-directory setting stays deferred.
- Keep direct launch behavior unchanged: it runs the file at its path with normal account access.
- Carry the application mount destination explicitly in the launch plan. `pin_launch_paths_in_place` currently searches for `/app/application.AppImage` (`sandbox_launch.rs:216`); it must use the carried destination and must never select a folder grant mount.
- Generalize snapshot creation to validate the saved kind, copy the file, reopen it safely, compare its digest with the reviewed content, and bind the pinned descriptor read-only.
- Preserve temporary-file cleanup, permission grant validation, private home pinning, and descriptor inheritance.
- Keep the detached launch monitor and logging flow shared. Make failure messages format-neutral where they apply to both kinds, including startup and exit failures (`sandbox_launch.rs:105`, `116`) and changed-since-review errors (`sandbox_launch.rs:53`, `164`).
- If Bubblewrap is unavailable or startup fails, show the error through the existing flow. Do not automatically retry without restrictions.

Validation after this step: both formats use the same policy construction; only validation, mount destination, and command prefix differ. Changing a binary after review blocks its launch until it is reviewed again.

### 4. Support configuration backups and enforce update boundaries

- Include kind in exported app records and validate restored files against the saved kind.
- Introduce backup format version 2 and continue accepting version 1 as AppImage-only data. Export version 2 so older AppBento versions reject new backups clearly. This is required because `InstalledApp` does not deny unknown fields, so older versions would otherwise ignore `kind` and treat binaries as AppImages.
- Use a kind-appropriate file picker during restore; `configuration-dialog.tsx` currently uses the AppImage picker. Preserve existing path remapping, duplicate/conflict handling, and transaction recovery.
- Enforce the binary invariants on restore. Reject binary entries with `integrated: true`, a `source_path`, or an update configuration rather than silently discarding the values. Restore currently copies `integrated` straight from the backup (`configuration.rs:280`).
- Never queue a permission change for binaries in `prepare_app` (`configuration.rs:350`); read the `executable` flag from the selected file instead. Changing the mode of a file owned by another user fails with `EPERM`, even when the mode is unchanged. The restore journal is replayed by every `with_repository` call (`persistence.rs:89`), so a single root-owned binary would block every library operation until the journal is removed by hand. Restore would also strip the execute bit from a user's file. AppImage restore behavior is unchanged.
- Check restored content against the saved digest using shared hashing. Retain existing review invalidation rules and never grant a fresh review just because a backup was restored.
- Guard updates by kind in `updates_configure`, `updates_check`, and `updates_install` before any download or replacement. `check_app` already returns `Unsupported` when no source is configured, and bulk checks already count it, so the only new behavior is the kind guard and a message stating that executable updates are not supported yet. `replacement.rs` stays AppImage-only.
- Keep AppImage download, replacement, recovery, and self-update behavior unchanged.

### 5. Add local executable import

- Generalize the lifecycle import entry point to dispatch on the detected format. The `library_import` command and its arguments stay unchanged.
- Keep the AppImage location choice, managed integration, and self-AppImage detection unchanged.
- For a binary, register its canonical path in place with the binary invariants, and reuse duplicate detection.
- Reject `move_to_managed` for binaries in the backend. `import_appimage` calls `integrate` before opening the file (`integration.rs:38`), so format detection must happen first.
- Never mark a binary as owned, even when it lives under the managed directory.
- Reject `delete_file` for binaries by kind in `remove`. Today only `integrated` is checked (`lifecycle.rs:303`); the AppImage open at `lifecycle.rs:311` is what currently blocks deletion of non-AppImages, and it stops doing so once opening becomes kind-aware.
- Keep folder scanning limited to AppImages. If a scanned `.AppImage` path matches an existing entry of a different kind, skip it with a scan warning. `discovered_app` copies the existing record, which would otherwise keep a stale kind.
- Re-importing the same file refreshes the existing entry without discarding its settings. If the detected format differs from the existing entry, reject the import and ask the user to remove the old entry first; never convert an entry in place.
- New binary entries start with no launch configuration and no reviewed digest, so the existing gates force a review before the first launch from the library (`library-actions.ts:133`) and from a desktop entry, where `--launch` opens the UI for unreviewed apps (`lib.rs:101`).
- Update launch preparation, permission saving, and menu-entry validation to use the saved kind.

Validation after this step: importing a binary without an extension creates one persistent library entry; scanning and restarting keep it; duplicate import does not create another entry; the first launch opens the review.

### 6. Update the frontend flow

- Add an explicit executable import action beside AppImage import, with a picker that allows extensionless files. The backend remains responsible for format validation.
- Skip the managed-location prompt for binaries by filtering on kind in `offerMove` (`library-actions.ts:22`), which every import path passes through.
- Keep `initialLaunchConfig` (`permissions-page.tsx:107`) starting in "Run without restrictions" for both kinds. For binaries without a saved configuration, take the draft permissions from a new `library_default_permissions` command backed by `sandbox::default_permissions`, so the folder grant is already listed when the user switches to Bubblewrap. The default must come from the backend: grants must pass `validate_grant`, and `configure` validates grants even in direct mode (`lifecycle.rs:162`), so an invalid frontend-derived folder would block saving the review. Register the command in `build.rs`, `capabilities/default.json`, and the `lib.rs` permission test.
- For binaries, show a short review note explaining that AppBento uses the file at its current location, that GUI apps are the focus with console output in launch logs, and that files in the granted folder are not covered by the review.
- Replace shared AppImage wording with "this app" in `executable-dialog.tsx`, `permissions-page.tsx`, `app-settings-dialog.tsx`, `integration-page.tsx`, `remove-dialog.tsx`, and the operation labels in `library-actions.ts`. Keep AppImage wording in AppImage-only features: URL downloads, folder scanning, updates, and Open With.
- In the remove dialog, offer no file deletion for binaries, and replace "A later scan can add it again" with guidance to add the file again from its location.
- Include the kind in `launch-fix-prompt.ts`. For binaries, describe the folder grant and mention missing support files or program loaders as likely causes.
- Make update settings unavailable for binaries with a short explanation, and exclude them from bulk update work.
- Keep the existing make-executable confirmation and custom icon controls.
- Allow desktop menu integration through the existing AppBento launcher, so menu launches still use the stored review and sandbox policy.
- Update preview fixtures and API mocks, including the new command, so the shared UI can display both kinds.

### 7. Update documentation

- Update `README.md`: the product description, the security model (the binary snapshot, the default read-only folder grant, and unreviewed support files), requirements and limits (native binaries for the host architecture with an installed program loader, Type 2 AppImages), and the feature list.

## Verification

Add behavior tests alongside the repository's existing Rust and Vitest suites:

- Compatibility: legacy library records and version 1 backups; version 2 round-trip with both kinds; unknown kinds fail loading without `library.json` being rewritten; mismatched restore files.
- Restore invariants: binaries with `integrated`, `source_path`, or an update configuration are rejected; restoring a binary queues no permission change.
- Validation: extensionless binary, wrong architecture, 32-bit, big-endian, unsupported OS ABI rejected and GNU/Linux OS ABI accepted, relocatable object, truncated or oversized program headers, missing program loader, `AI` magic with an unknown type treated as an AppImage, script rejection, symlink rejection, missing execute bit.
- Import and lifecycle: duplicate import, rejected managed move, binary under the managed directory not owned, deletion rejected for binaries even when `integrated` is true, format change rejected on re-import, scan skips kind mismatches, binaries persist through scan and reload, filename-derived name and version, binary-specific missing-file message.
- Default permissions: a binary's parent folder is granted read-only; no grant when the parent is the home folder or a reserved location; AppImage defaults unchanged; the initial mode is "Run without restrictions" for both kinds; the first launch requires review from the library and from a desktop entry.
- Launch planning: a binary inside a grant mounts at its original path after the grant mount; a binary outside grants mounts at `/app/<file_name>`; no extraction argument for binaries; AppImages unchanged; a loader outside the mounted runtime fails with a clear error; descriptor pinning uses the carried destination and never a grant mount; arguments, environment, and grants preserve existing behavior.
- Review and snapshots: content changes invalidate approval; snapshot digest mismatch blocks execution; snapshots are cleaned up on success and failure.
- Desktop integration and configuration restore for each kind.
- Updates: binary checks report unsupported with the binary message; configure and install reject binaries before download or replacement; existing AppImage update tests keep passing.

Use small fixture files for parsing and planning tests. Use real runnable fixture binaries only where execution is required. Stub network access. Follow existing Bubblewrap integration-test conventions and report environment-related skips explicitly.

Run the project's existing checks after implementation:

```sh
pnpm typecheck
pnpm lint
pnpm test
cargo test --workspace
```

Do not run a production build without approval. No new dependencies are expected; request approval if implementation reveals a need for one.

After automated checks pass, perform one final browser verification of the import, review, and settings flow with the existing mock backend. Native Tauri file dialogs and actual Bubblewrap execution require a native check:

1. Import a GUI application extracted from an archive with libraries or resources next to its binary.
2. Confirm that the first launch opens the review, starting in "Run without restrictions".
3. Switch to Bubblewrap, confirm the pre-filled read-only folder grant, grant display access, and launch.
4. Confirm that support files load, logs are recorded, and private data persists.
5. Close AppBento while the app runs, then launch it again from its desktop entry.

Cover unavailable Bubblewrap, a changed binary requiring review, and a moved binary's error message as failure cases. Ask the user to review visual layout manually if needed.

## Completion criteria

- Existing AppImage libraries, permission reviews, imports, sandbox launches, updates, and backups remain usable.
- A supported native executable can be imported in place, reviewed, sandboxed with access to its own folder, configured, and launched from AppBento or the desktop menu.
- Every new entry requires a permission review before its first launch, starting from "Run without restrictions".
- Executable changes trigger the existing permission re-review flow.
- Scanning does not hide imported executables, and no backend path or restored backup can delete an executable's file.
- Configuration export/restore supports both kinds, reads old backups, and never changes an executable's file permissions.
- Unsupported formats, missing program loaders, and executable updates fail clearly without falling through to AppImage-specific code.
- The README describes executable support and its limits.
- Relevant automated checks pass, with native verification results and any environment limitations reported.

## Later phase: updates and application folders

For standalone binary updates, reuse the existing GitHub/GitLab/direct URL source model and download infrastructure. Generalize artifact validation, destination naming, and transactional replacement while preserving executable permissions, app identity, rollback, and review invalidation. Version detection must be configured or derived from release metadata rather than assuming an arbitrary binary exposes a safe version command.

Treat full application folders as a separate design step. The first release exposes an executable's folder but reviews only the executable. Folder support needs an explicit application root and entry point (including launch scripts), a content-review strategy covering the bundle, working-directory rules, relocation of moved entries, and atomic folder replacement and rollback. Do not extend the single-file digest guarantee to an entire application folder without that design.
