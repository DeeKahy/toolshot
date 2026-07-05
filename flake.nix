{
  description = "Toolshot dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        default = toolshot;
        toolshot = pkgs.rustPlatform.buildRustPackage {
          pname = "toolshot";
          version = "0.1.0";
          src = ./.;
          cargoRoot = "src-tauri";
          buildAndTestSubdir = "src-tauri";
          cargoLock.lockFile = ./src-tauri/Cargo.lock;
          nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.pkg-config pkgs.wrapGAppsHook3 ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [
            pkgs.gtk3
            pkgs.webkitgtk_4_1
            pkgs.libsoup_3
            pkgs.librsvg
            pkgs.libayatana-appindicator
            pkgs.dbus
            pkgs.openssl
            pkgs.xorg.libxcb
            pkgs.pipewire
          ];
          doCheck = false;

          # A real .app bundle so Spotlight and Launchpad can find it.
          postInstall = pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
            app="$out/Applications/Toolshot.app/Contents"
            mkdir -p "$app/MacOS" "$app/Resources"
            cp "$out/bin/toolshot" "$app/MacOS/toolshot"
            cp ${./src-tauri/icons/icon.icns} "$app/Resources/icon.icns"
            cat > "$app/Info.plist" <<'EOF'
            <?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
            <plist version="1.0">
            <dict>
              <key>CFBundlePackageType</key><string>APPL</string>
              <key>CFBundleName</key><string>Toolshot</string>
              <key>CFBundleDisplayName</key><string>Toolshot</string>
              <key>CFBundleIdentifier</key><string>dev.deekahy.toolshot</string>
              <key>CFBundleExecutable</key><string>toolshot</string>
              <key>CFBundleVersion</key><string>0.1.0</string>
              <key>CFBundleShortVersionString</key><string>0.1.0</string>
              <key>CFBundleIconFile</key><string>icon.icns</string>
              <key>LSMinimumSystemVersion</key><string>11.0</string>
              <key>LSUIElement</key><true/>
              <key>NSHighResolutionCapable</key><true/>
            </dict>
            </plist>
            EOF
          '';
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            nodejs
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            # Tauri and xcap system deps, untested so far
            pkg-config
            gtk3
            webkitgtk_4_1
            libsoup_3
            librsvg
            libayatana-appindicator
            dbus
            openssl
            xorg.libxcb
            pipewire
          ];
        };
      });
    };
}
