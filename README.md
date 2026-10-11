# AppBento

Local AppImage manager for Linux — inspired by [Gear Lever](https://github.com/mijorus/gearlever) (feature ideas only, original code).

- **Bento home**: most-recently-used apps in a bento grid
- **Library**: scan `~/Applications`, `~/Downloads`, … for scattered AppImages, integrate (copy into the managed folder, chmod +x, `.desktop` entry; keep the original download)
- **Updates v1**: GitHub Releases asset matching via `asset_regex`
- **Themes**: Tokyo Night, Gruvbox, Catppuccin, Nord, Dracula, Terminal green, Paper — TUI vibe throughout

## Stack

Tauri v2 (Rust) + React 19 + Tailwind v4 (shadcn-style `cva` components in `src/components/ui/`).

## Dev

```sh
sfw pnpm install
sfw pnpm dev            # web preview (mock data)
sfw pnpm tauri dev      # full app with Rust backend
cargo check -p appbento # backend check
```
