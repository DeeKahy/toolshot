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
