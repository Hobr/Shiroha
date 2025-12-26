{
  description = "Rust Shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      flake-utils,
      nixpkgs,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [
          rust-overlay.overlays.default
          (final: prev: {
            rustToolchain =
              let
                rust = prev.rust-bin;
              in
              if builtins.pathExists ./rust-toolchain.toml then
                rust.fromRustupToolchainFile ./rust-toolchain.toml
              else if builtins.pathExists ./rust-toolchain then
                rust.fromRustupToolchainFile ./rust-toolchain
              else
                rust.stable."1.92.0".default.override {
                  extensions = [
                    "rust-src"
                    "rustfmt"
                    "rust-analyzer"
                    "clippy"
                    "cargo"
                  ];
                  targets = [
                    "wasm32-wasip2"
                  ];
                };
          })
        ];
        pkgs = import nixpkgs { inherit overlays system; };
      in
      {
        devShells.default =
          with pkgs;
          mkShell rec {
            packages = [
              pkg-config
              openssl

              rustToolchain
              cargo-binstall
              pre-commit
              just

              libiconv
              glib
              gtk3
              libsoup_3
              webkitgtk_4_1
              xdotool
            ];

            env = {
              LD_LIBRARY_PATH = lib.makeLibraryPath packages;
              RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            };

            shellHook = ''
              export CARGO_HOME="$PWD/.cargo"
              export PATH="$CARGO_HOME/bin:$PATH"
            '';
          };
      }
    );
}
