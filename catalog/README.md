# AppBento catalog

Each file in `apps/*.toml` describes one popular AppImage. The backend
embeds them at compile time and validates them through `src-tauri/src/catalog.rs`.
The frontend renders the bundled metadata with logos/covers; the launch working
directory cannot replace the catalog.

## Schema (`schema.json` is the normative reference)

```toml
# Matching fields belong at the root, before any table headers.
match_filenames = ["Obsidian-*.AppImage"]
match_binaries = ["obsidian"]
version_regex = "^Obsidian-(?P<version>[0-9]+(?:\\.[0-9]+){1,3})(?:-[A-Za-z0-9_]+)?\\.AppImage$"

[app]
id = "obsidian"                 # slug, == filename
name = "Obsidian"
summary = "Short one-liner"
description = "Longer markdown-ish text"
website = "https://obsidian.md"
docs_url = "https://help.obsidian.md"   # optional
repo = "https://github.com/obsidianmd/obsidian-releases" # optional
license = "Proprietary"                  # optional
categories = ["Productivity", "Notes"]   # free-form tags
verified = true                          # metadata reviewed by maintainer

[branding]
logo = "https://obsidian.md/favicon.svg" # https URL preferred
cover = "https://obsidian.md/images/hero.png" # wide banner for bento tiles
screenshots = []
accent = "#7c3aed"                       # tile accent color

[update]
strategy = "github-release"  # github-release | manual

[update.github]
repo = "obsidianmd/obsidian-releases"
architecture = "x86_64"       # x86_64 | aarch64
tag_regex = "^[vV]?(?P<version>[0-9]+(?:\\.[0-9]+){1,3})$"
asset_regex = "Obsidian-.*\\.AppImage$"
prerelease = false
channel = "stable"           # stable non-prerelease releases only

[desktop]
categories = "Office;"
terminal = false

```

### Update checks and manual downloads

- `github-release`: check GitHub releases for one compatible AppImage matching
  `asset_regex`, `architecture`, and `tag_regex`. Ambiguous assets are errors.
  Checks compare numeric release versions and report failures individually.
- `manual`: no release check; use the application's website.
- Downloads remain manual. AppBento does not automatically download, install,
  replace, or verify the authenticity of release artifacts.
- A filename version is inferred only from the entry's explicit patterns and
  named `version` capture. Unknown installed versions are reported as unknown;
  a latest release link does not establish that an update is available.
- `verified` means catalog metadata was reviewed, not that a local executable
  or release artifact has a verified signature.

Entries reject unknown fields, duplicate or mismatched IDs, invalid expressions,
unsupported configuration, and links or artwork outside HTTPS. Artwork origins
must also be permitted by the application's CSP when adding a new catalog entry.

### Artwork policy

- `logo`: square-ish, HTTPS. `cover`: 16/9 banner for bento grid.
- No binary blobs in git for v1 — URLs only. A `catalog://` local-asset
  scheme is unsupported by the current strict catalog.
