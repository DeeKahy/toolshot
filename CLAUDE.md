# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Style rules

- No em dashes anywhere: not in code, comments, docs, UI strings, or commit messages. Use hyphens, commas, or colons.
- Plain commit messages with no trailers of any kind.
- No AI attribution anywhere: no "Generated with Claude Code" footers or Co-Authored-By lines in commits, PR bodies, or issues.

## Build and run

Rust is not installed globally on dev machines, everything comes from the flake. The repo is a cargo workspace with two crates:

```sh
nix develop
cargo build                      # both binaries land in target/debug/
./target/debug/toolshot          # tray daemon, look in the menu bar
./target/debug/toolshot-ui capture   # or run one UI session directly
```

`toolshot` (from `daemon/`) is the tiny resident tray/hotkey process. `toolshot-ui` (from `src-tauri/`) is the Tauri app; it takes exactly one mode argument (`capture`, `pick`, `fullscreen`, `settings`), runs that session and exits. The daemon spawns it and expects it as a sibling file, which is true in `target/debug` and inside the app bundle. Running a mode directly is the fastest way to test one surface.

`nix build .#toolshot` builds the release package including the macOS app bundle at `result/Applications/Toolshot.app` (bundle executable is the daemon, `toolshot-ui` sits next to it).

There are no tests yet. There is no lint setup beyond `cargo` warnings, keep the build warning-free.

Important: the static frontend in `src/` is embedded into the toolshot-ui binary at compile time (`frontendDist` points at `../src`, no dev server). Editing any frontend file (HTML, `src/css/`, `src/js/`) requires a `cargo build` before the running app picks it up. Each page is markup-only HTML plus a matching `css/<page>.css` and `js/<page>.js`, with shared styles in `css/base.css`.

## Releasing

Push a tag to build and publish all platforms:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

`.github/workflows/release.yml` builds a universal macOS dmg (both binaries lipo'd, bundle assembled in the workflow with the daemon as the bundle executable), and Linux AppImage/deb/rpm plus Windows NSIS/MSI via `npx tauri build`. On those platforms the daemon ships as a Tauri sidecar: the CI copies it to `src-tauri/binaries/toolshot-<triple>` and passes `--config src-tauri/tauri.sidecar.conf.json`, which adds `bundle.externalBin`. That overlay exists because `tauri-build` refuses to compile when an externalBin file is missing, and local builds should not need a pre-staged daemon. The installed entry point everywhere is the Tauri binary with no arguments, which execs/spawns the daemon before any webview starts (`launch_daemon` in `lib.rs`); the daemon holds a lock file so double launches do not create a second tray icon.

Tags containing `beta` publish as prereleases. The macOS job also uploads a version-free `Toolshot_universal.dmg` that the Homebrew cask at DeeKahy/homebrew-tap points to via `releases/latest/download`, so brew needs no bump per release. The download site in `docs/` (GitHub Pages, deployed by `pages.yml`) reads the releases API client-side.

Version lives in five places that must stay in sync: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `daemon/Cargo.toml`, `package.json`, and `flake.nix` (twice, package version and the Info.plist).

## Architecture

Two processes, one job each:

- **Daemon** (`daemon/`, binary `toolshot`): the only resident process. A winit event loop with zero windows holding the tray icon (`tray-icon`) and global shortcuts (`global-hotkey`), around 10MB physical footprint. It spawns `toolshot-ui <mode>` per session, kills the previous session child when a new capture starts (which is also how "press the shortcut again to dismiss the overlay" works), and re-registers shortcuts when the settings file's mtime changes (2s poll). Keep this crate free of UI, capture and image dependencies; every dependency here is RAM spent 24/7.
- **UI sessions** (`src-tauri/`, binary `toolshot-ui`): the Tauri 2 app, one session per process. It takes a mode argument, opens the matching window from `setup`, and exits when its last window closes. All the webview/capture memory dies with the process.

The session-exit rule lives in `lib.rs`: `RunEvent::ExitRequested` is allowed through unless the `Busy` flag is set. Commands that close one window before opening the next (overlay to editor, overlay to color popup, fullscreen capture with zero windows) set `Busy` first and clear it once the next window exists; failure paths must either clear it or call `app.exit(1)`, otherwise the process lingers invisibly. Keep that invariant when adding flows.

Tray-only look: no dock icon (`ActivationPolicy::Accessory` on macOS in both processes, `windows: []` in tauri.conf.json). Every window is created on demand from Rust and identified by label. Adding a new window label requires listing it in `src-tauri/capabilities/default.json` or its IPC silently fails.

Windows and who creates them:

- `overlay` (`src/overlay.html`): fullscreen transparent capture/picker surface, built by `capture::open_overlay`
- `editor` (`src/editor.html`): post-capture annotation editor, built by `capture::open_editor`
- `colorpick` (`src/picker.html`): color format popup, built by `picker::pick_color`
- `settings` (`src/settings.html`): shortcuts, autostart, update check, built by `settings::open_settings_window`

Transparency on macOS needs both the `macos-private-api` cargo feature and `"macOSPrivateApi": true` in tauri.conf.json.

### The frozen screen model

When the overlay opens, `capture.rs` grabs the entire monitor once into `ScreenState` (raw RGBA plus a scale factor) before the window becomes visible, so the overlay itself is never in the frame. Area selection crops from this frozen buffer in Rust, the magnifier loupe samples it in JS, and the color picker reads pixels from it. Window capture is the exception: it captures live via `xcap::Window` on click. Consequence: anything that changes on screen after the overlay opens does not exist as far as drag capture is concerned.

The overlay is dual-mode. `OverlayMode` state ("capture" or "pick") is set before the window opens and fetched by the page on load. The page starts inert: `mode` is null and all mouse handlers bail until the backend answers, which prevents capture-mode visuals (dim layer, guide lines) from leaking into picker mode. Keep that invariant when adding modes.

### IPC conventions

Big pixel buffers never go through JSON. The frozen screen travels Rust to JS as raw bytes via `tauri::ipc::Response` (`get_screen_rgba`), and the annotated editor canvas travels JS to Rust as a raw `Uint8Array` payload with an 8-byte width/height header (`copy_annotated`). Small images (the capture shown in the editor, PNG-encoded with fast compression) use base64. If you add a pixel-heavy path, follow the raw-bytes pattern, the base64+JSON detour cost around a second per 5K frame before it was removed.

Coordinates: CSS pixels in the overlay equal macOS logical points (overlay sits at the monitor origin). Captured images are physical pixels. `FrozenScreen.scale` converts, and `capture_area` expects logical coordinates.

### State and settings

All cross-window state lives in `tauri::State` mutexes registered in `lib.rs`: `CaptureState` (PNG of the last capture), `ScreenState` (frozen frame), `OverlayMode`, `PickerState` (picked color), `SettingsState`, plus the `Busy` exit guard. Settings persist as JSON in the app config dir (`dev.deekahy.toolshot`). The settings UI validates a new shortcut by registering and immediately unregistering it (so an invalid or taken combo never saves), then writes the file; the live registration belongs to the daemon, which reloads on mtime change. The autostart toggle uses the `auto-launch` crate pointed at the daemon binary, not this UI binary.

### Platform gotchas

- Without macOS Screen Recording permission, `xcap::Window::all()` returns an error, not an empty list. The overlay checks `check_screen_permission` first and shows instructions. When running the raw dev binary, the TCC grant attaches to the parent terminal, not the app.
- Accessory apps do not focus their windows automatically. Every window that needs keyboard input calls `set_focus()` after creation, otherwise Esc and shortcuts silently do nothing.
- The overlay only covers the primary monitor, and macOS Space switching leaves a stale overlay behind (two dismissal approaches failed, see the open issue).
- Linux compiles but has never been run. Wayland specifics are untested, and the daemon tray needs libayatana-appindicator at runtime.
- A leftover `src-tauri/target` from before the workspace may exist locally with root-owned files; it is gitignored, remove it with `sudo rm -rf src-tauri/target`. The workspace builds into `target/` at the repo root.

## Roadmap

Planned work lives in GitHub issues (gh issue list). TODO.md keeps the history of what has landed, append to it when a feature ships and close the matching issue.
