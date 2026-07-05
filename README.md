# Toolshot

A cross-platform screenshot and screen utility tool that lives in the tray. Built with Tauri and xcap.

Current state: window capture works. Click the tray icon, pick "Capture Window", hover to highlight any window, click to capture it. The shot opens in an editor window where Cmd+C copies it to the clipboard and closes the window. See [TODO.md](TODO.md) for the roadmap (area select, annotations, color picker, presenting mode and more).

## Requirements

Everything comes from the flake:

```sh
nix develop
```

That provides cargo, rustc, rustfmt, clippy, rust-analyzer and node (plus the Tauri system libraries on Linux). Without nix, install a Rust toolchain and Node.js yourself.

## Run

Inside the dev shell:

```sh
cd src-tauri
cargo build
./target/debug/toolshot
```

Or through the Tauri CLI:

```sh
npm install
npm run tauri dev
```

The app has no main window. Look for the icon in the menu bar (macOS) or system tray.

## macOS permissions

The first capture will prompt for Screen Recording permission (System Settings, Privacy and Security, Screen and System Audio Recording). Grant it and relaunch the app. Without it, window titles come back empty and captures fail.

## Layout

- `src/` static frontend pages, no build step (`overlay.html` window picker, `editor.html` screenshot editor)
- `src-tauri/src/lib.rs` app setup and run loop
- `src-tauri/src/tray.rs` tray icon and menu
- `src-tauri/src/capture.rs` window enumeration, capture, clipboard, editor/overlay window management
