{
  description = "VibeLang - Make music with code. Make code with vibes.";

  # Nix flake for reproducible builds and development
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
          ];
        };

        nativeBuildInputs = with pkgs; [
          rustToolchain
          pkg-config
        ];

        buildInputs =
          with pkgs;
          [
            # Add any system dependencies here
            # For example, if the project needs OpenSSL:
            # openssl
            alsa-lib
            jack2
            supercollider
            xorg.libX11
            xorg.libXi
            xorg.libXtst
          ]
          ++ lib.optionals stdenv.isDarwin [
            darwin.apple_sdk.frameworks.Security
            darwin.apple_sdk.frameworks.CoreServices
            darwin.apple_sdk.frameworks.SystemConfiguration
          ];

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        vibelang = rustPlatform.buildRustPackage {
          pname = "vibelang";
          version = "0.1.0";
          src = builtins.path {
            name = "vibelang";
            path = ./.;
          };

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          inherit nativeBuildInputs buildInputs;

          # Fix aubio-sys build by enabling POSIX functions
          NIX_CFLAGS_COMPILE = "-D_GNU_SOURCE";

          meta = with pkgs.lib; {
            description = "VibeLang - A musical programming language";
            homepage = "https://github.com/dpc/vibelang";
            license = licenses.mit;
            maintainers = [ ];
          };
        };

      in
      {
        packages = {
          default = vibelang;
          vibelang = vibelang;
        };

        devShells.default = pkgs.mkShell {
          inherit buildInputs;

          nativeBuildInputs =
            nativeBuildInputs
            ++ (with pkgs; [
              # Development tools
              rustfmt
              clippy
              cargo-watch
              cargo-edit
            ]);

          # Fix aubio-sys build by enabling POSIX functions
          NIX_CFLAGS_COMPILE = "-D_GNU_SOURCE";
        };

        # Provide an overlay for use in other flakes
        overlays.default = final: prev: {
          vibelang = vibelang;
        };
      }
    );
}
