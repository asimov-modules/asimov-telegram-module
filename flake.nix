{
  description = "Rust development environment";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        tdlib = pkgs.tdlib.overrideAttrs (oldAttrs: {
          version = "1.8.29";
          src = pkgs.fetchFromGitHub {
            owner = "tdlib";
            repo = "td";
            rev = "af69dd4397b6dc1bf23ba0fd0bf429fcba6454f6";
            hash = "sha256-2RhKSxy0AvuA74LHI86pqUxv9oJZ+ZxxDe4TPI5UYxE=";
          };
        });
      in {
        formatter = pkgs.nixfmt;
        devShells.default = pkgs.mkShell rec {
          nativeBuildInputs = [ pkgs.pkg-config tdlib ];
          buildInputs = [ pkgs.rustup ];

          RUSTC_VERSION = "1.87.0";

          shellHook = ''
            export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin
          '';

          # Add precompiled library to rustc search path
          RUSTFLAGS = (builtins.map (a: "-L ${a}/lib") [
            # add libraries here (e.g. pkgs.libvmi)
            # pkgs.openssl.dev
          ]) ++ [ "-l tdjson" ];

          LD_LIBRARY_PATH =
            pkgs.lib.makeLibraryPath (buildInputs ++ nativeBuildInputs);
        };
      });
}
