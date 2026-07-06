# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Style rules

- No em dashes anywhere: not in code, comments, docs, UI strings, or commit messages. Use hyphens, commas, or colons.
- Plain commit messages with no trailers of any kind.

## Build and run

Rust is not installed globally on dev machines, everything comes from the flake:

```sh
nix develop
cd src-tauri && cargo build      # binary at src-tauri/target/debug/toolshot
./target/debug/toolshot          # tray-only app, look in the menu bar
```

`npm run tauri dev` also works (Tauri CLI via npm, no frontend bundler involved). `nix build .#toolshot` builds the release package including the macOS app bundle at `result/Applications/Toolshot.app`.

There are no tests yet. There is no lint setup beyond `cargo` warnings, keep the build warning-free.

Important: the static frontend in `src/` is embedded into the binary at compile time (`frontendDist` points at `../src`, no dev server). Editing any HTML file requires a `cargo build` before the running app picks it up.

## Releasing

Push a tag to build and publish all platforms:

```sh
git tag v0.2.0 && git push origin v0.2.0
```

`.github/workflows/release.yml` builds a universal macOS dmg, Linux AppImage/deb/rpm (ubuntu-24.04, older runners have a PipeWire too old for xcap), and Windows installers via tauri-action. Tags containing `beta` publish as prereleases. The macOS job also uploads a version-free `Toolshot_universal.dmg` that the Homebrew cask at DeeKahy/homebrew-tap points to via `releases/latest/download`, so brew needs no bump per release. The download site in `docs/` (GitHub Pages, deployed by `pages.yml`) reads the releases API client-side.

Version lives in three places that must stay in sync: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `flake.nix`.

## Architecture

Tray-only Tauri 2 app: no main window, no dock icon (`ActivationPolicy::Accessory` on macOS, `windows: []` in tauri.conf.json). Every window is created on demand from Rust and identified by label. Adding a new window label requires listing it in `src-tauri/capabilities/default.json` or its IPC silently fails.

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

All cross-window state lives in `tauri::State` mutexes registered in `lib.rs`: `CaptureState` (PNG of the last capture), `ScreenState` (frozen frame), `OverlayMode`, `PickerState` (picked color), `SettingsState`. Settings persist as JSON in the app config dir; global shortcuts are registered through `tauri-plugin-global-shortcut` before being persisted, so an invalid or taken combo never saves.

The app must keep running with zero windows: `lib.rs` intercepts `RunEvent::ExitRequested` and prevents exit unless it came from the tray Quit (`app.exit(0)`).

### Platform gotchas

- Without macOS Screen Recording permission, `xcap::Window::all()` returns an error, not an empty list. The overlay checks `check_screen_permission` first and shows instructions. When running the raw dev binary, the TCC grant attaches to the parent terminal, not the app.
- Accessory apps do not focus their windows automatically. Every window that needs keyboard input calls `set_focus()` after creation, otherwise Esc and shortcuts silently do nothing.
- The overlay only covers the primary monitor, and macOS Space switching leaves a stale overlay behind (two dismissal approaches failed, see the open issue).
- Linux compiles but has never been run. Wayland specifics are untested.

## Roadmap

Planned work lives in GitHub issues (gh issue list). TODO.md keeps the history of what has landed, append to it when a feature ships and close the matching issue.
