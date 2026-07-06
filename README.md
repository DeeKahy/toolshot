# Toolshot

A screenshot and screen utility tool that lives in your menu bar. Click a window to capture it, drag to capture an area with a pixel-perfect magnifier loupe, annotate with rectangles and arrows, pick colors from anywhere on screen. Built with Tauri and xcap.

Download: https://deekahy.github.io/toolshot/

## Features

- Window capture: hover highlights any window, click grabs it
- Area capture in the same overlay: just drag instead of clicking
- Magnifier loupe with pixel grid, coordinates and live hex color readout
- Crosshair guide lines for lining up selections
- Editor with rectangle and arrow annotations, color choice and undo
- Cmd+C copies the annotated shot to the clipboard and gets out of your way
- Color picker with a format popup: hex, rgb, hsl, hsb, SwiftUI, click to copy
- Configurable global hotkeys for capture and color picker
- Launch at login toggle

See [TODO.md](TODO.md) for the roadmap: blur/blackout, text tool, OCR, scrolling capture, presenting mode with laser pointer and fading ink, and more.

## Install

### macOS (Homebrew)

```sh
brew install --cask deekahy/tap/toolshot
```

Add `--no-quarantine` to skip the Gatekeeper prompt for the unsigned app.

### macOS (download)

Grab the dmg from the [download page](https://deekahy.github.io/toolshot/) or the [releases](https://github.com/DeeKahy/toolshot/releases), drag Toolshot to Applications.

The app is not signed with an Apple developer certificate yet, so the first launch needs one manual step: macOS will refuse to open it, then you go to System Settings, Privacy and Security, scroll down and click "Open Anyway". Alternatively clear the quarantine flag yourself:

```sh
xattr -dr com.apple.quarantine /Applications/Toolshot.app
```

On first capture, grant Screen Recording permission (System Settings, Privacy and Security, Screen and System Audio Recording) and relaunch.

### Nix (flake, macOS via nix-darwin or NixOS)

The repo is a flake with a package output. Try it without installing:

```sh
nix run github:DeeKahy/toolshot
```

Install into your profile:

```sh
nix profile install github:DeeKahy/toolshot
```

Or add it to a nix-darwin / NixOS configuration:

```nix
{
  inputs.toolshot.url = "github:DeeKahy/toolshot";
  # then in your system packages:
  # environment.systemPackages = [ inputs.toolshot.packages.${pkgs.system}.default ];
}
```

On macOS the package also ships an app bundle at `$out/Applications/Toolshot.app`. Spotlight does not index symlinked apps, so copy it into place from an activation script if you want it searchable:

```nix
system.activationScripts.postActivation.text = ''
  rm -rf /Applications/Toolshot.app
  cp -R ${inputs.toolshot.packages.aarch64-darwin.default}/Applications/Toolshot.app /Applications/Toolshot.app
'';
```

Linux support is compiled in but has not had a real testing pass yet. A nixpkgs submission is planned once that lands.

## Build from source

Everything comes from the flake:

```sh
nix develop
cargo build
./target/debug/toolshot
```

Without nix: install a Rust toolchain and run `cargo build`. The workspace builds two binaries: `toolshot`, the tiny tray daemon that stays resident, and `toolshot-ui`, the Tauri app it spawns for each capture, edit or settings session.

## Updates

Toolshot checks for updates only when you ask it to: Settings, "Check for updates". If a newer version exists it opens the download page. Nix installs update through nix instead.

## Layout

- `daemon/` the resident tray and hotkey process, kept as small as possible
- `src/` static frontend pages, no build step (overlay, editor, color popup, settings)
- `src-tauri/src/` Rust backend for UI sessions: capture, clipboard, shortcuts, settings
- `docs/` the GitHub Pages download site
- `.github/workflows/` release CI building macOS, Linux and Windows artifacts

## License

GPL-3.0. You can do what you want with it as long as derivatives stay open. If you find it useful, donations are welcome.
