# Toolshot

A cross-platform screenshot and screen utility tool that lives in the tray. Built with Tauri and xcap.

Current state: window capture works. Click the tray icon, pick "Capture Window", hover to highlight any window, click to capture it. The shot opens in an editor window where Cmd+C copies it to the clipboard and closes the window. See [TODO.md](TODO.md) for the roadmap (area select, annotations, color picker, presenting mode and more).

## Requirements

- Rust toolchain (cargo). On a nix system without a global Rust install:

  ```sh
  nix shell nixpkgs#cargo nixpkgs#rustc
  ```

- Node.js (only used for the Tauri CLI)

## Run

```sh
npm install
npm run tauri dev
```

Or build the Rust side directly:

```sh
cd src-tauri
cargo build
./target/debug/toolshot
```

The app has no main window. Look for the icon in the menu bar (macOS) or system tray.

## macOS permissions

The first capture will prompt for Screen Recording permission (System Settings, Privacy and Security, Screen and System Audio Recording). Grant it and relaunch the app. Without it, window titles come back empty and captures fail.

## Layout

- `src/` static frontend pages, no build step (`overlay.html` window picker, `editor.html` screenshot editor)
- `src-tauri/src/lib.rs` app setup and run loop
- `src-tauri/src/tray.rs` tray icon and menu
- `src-tauri/src/capture.rs` window enumeration, capture, clipboard, editor/overlay window management
