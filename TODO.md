# Toolshot roadmap

Working list of everything planned. Items get checked off as they land.

## Done

- [x] Tray icon with menu (macOS menu bar, Windows system tray)
- [x] Window capture: pick a window by clicking it, with live highlight and app/title label
- [x] Editor window opens after capture, Cmd+C (Ctrl+C) copies to clipboard and closes, Esc closes

## Capture

- [ ] Area select with drag rectangle
  - [ ] Magnifier loupe around the cursor showing zoomed pixels for pixel-perfect edges
  - [ ] Crosshair guide lines extending up, down, left and right from the cursor
  - [ ] Show live width x height readout while dragging
- [ ] Fullscreen capture (current monitor, and all-monitors option)
- [ ] Multi-monitor support for the picker overlay (currently primary monitor only)
- [ ] Global hotkeys for each capture mode, configurable in settings
- [ ] Scrolling capture: auto-scroll a window and stitch frames into one tall image
- [ ] Delayed capture: 3/5/10 second timer for menus and hover states
- [ ] Repeat last region: hotkey that re-captures the exact same rect as last time
- [ ] GIF / MP4 screen recording of a region

## Editor

- [ ] Rectangle tool
- [ ] Arrow tool
- [ ] Freeform pen tool
- [ ] Text tool
- [ ] Blur and blackout tool for hiding sensitive info
- [ ] Color picker that samples pixels from the screenshot
- [ ] Undo / redo
- [ ] Crop
- [ ] Save to file (Cmd+S) with configurable folder and filename template
- [ ] Step number badges: auto-incrementing 1, 2, 3 stamps for guides
- [ ] Pretty mode: gradient background padding, rounded corners, drop shadow
- [ ] Pin to screen: float the shot as a small always-on-top reference window
- [ ] OCR text grab: select a region, recognized text goes to the clipboard (Apple Vision on macOS, Windows.Media.Ocr on Windows, Tesseract on Linux)

## Color picker (standalone, from the tray)

- [ ] Pick any pixel on screen with a magnifier loupe
- [ ] Copy as hex / rgb / hsl
- [ ] Palette history: remember every picked color, export a palette

## Presenting mode

- [ ] Laser pointer: glowing trail that follows the mouse and fades out
- [ ] Fading ink: draw strokes that fade away after a few seconds
- [ ] Persistent ink toggle for drawings that stay until cleared
- [ ] Spotlight mode: dim everything except a circle around the cursor
- [ ] Live zoom lens: magnify around the cursor, scroll to change zoom
- [ ] Key + click visualizer: show pressed keys and click ripples for tutorials
- [ ] Click-through overlay so the desktop stays usable while ink is on screen
- [ ] Hotkey to toggle presenting mode on and off quickly

## Utilities

- [ ] Capture history: browsable library of past shots, search by app and date
- [ ] Screen ruler: measure pixel distances and rectangle dimensions on screen
- [ ] Upload / share: push a capture to imgur / S3 / custom endpoint, link on clipboard
- [ ] Settings window: hotkeys, save location, filename template, default copy behavior
- [ ] Launch at login

## Housekeeping

- [ ] Custom tray and app icon (currently the default Tauri icon)
- [ ] Windows and Linux testing pass (code is cross-platform but only exercised on macOS so far)
- [ ] Wayland support notes and fallbacks for Linux
