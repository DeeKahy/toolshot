# Toolshot roadmap

Planned work is tracked as GitHub issues: https://github.com/DeeKahy/toolshot/issues

This file keeps the history of what has landed.

## Done

- [x] Tray icon with menu (macOS menu bar, Windows system tray)
- [x] Window capture: pick a window by clicking it, with live highlight and app/title label
- [x] Editor window opens after capture, Cmd+C (Ctrl+C) copies to clipboard and closes, Esc closes
- [x] Area select merged into the same overlay: click captures the window, drag captures the area
  - [x] Magnifier loupe with pixel grid, coordinates and color under the cursor
  - [x] Crosshair guide lines extending up, down, left and right from the cursor
  - [x] Live width x height readout while dragging
  - [x] Area crops from a frame frozen when the overlay opens, so pixels cannot shift mid drag
- [x] Global hotkeys for capture and color picker, recorded in the settings window
- [x] Editor: rectangle tool, arrow tool, color swatch, undo, copies include annotations
- [x] Standalone color picker with loupe and a format popup (hex / rgb / hsl / hsb / SwiftUI)
- [x] Settings window with shortcut recording, launch at login, manual update check
- [x] Nix flake with dev shell and package output (macOS app bundle included)
- [x] GitHub release CI for macOS, Linux and Windows, download site on GitHub Pages
- [x] Homebrew tap (DeeKahy/homebrew-tap) with a self-updating cask
- [x] Fullscreen capture: tray menu item and a recordable global shortcut, opens straight in the editor
- [x] Editor: blur tool (mosaic pixelation, also covers annotations drawn under it)
- [x] Editor: crop tool with dimmed selection preview, non-destructive and undoable
- [x] Editor: pretty mode toggle, gradient padding with rounded corners and drop shadow composited on copy
- [x] Editor: six gradient presets for pretty mode; gradient, toggle and annotation color persist across captures
- [x] Editor: auto gradient (default) sampled from the screenshot's own dominant colors
- [x] Editor: annotations always render above blurs and above the pretty border, arrows can overhang the padding
- [x] Editor: pretty framing is drawn on the working canvas itself, the preview and the copy are the same pixels
- [x] Memory: frozen screen frame and capture PNG are dropped as soon as the overlay / editor is done with them
- [x] Frontend split into markup-only HTML plus per-page files under src/css/ and src/js/, shared css/base.css
- [x] Memory: resident process split into a tiny tray/hotkey daemon (about 12MB) that spawns a per-session Tauri UI process, all capture and webview memory is returned on close
