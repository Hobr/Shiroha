{
  description = "Rust Shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flakelight.url = "github:nix-community/flakelight";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    { flakelight, ... }@inputs:
    flakelight ./. {
      inherit inputs;

      withOverlays = [
        inputs.rust-overlay.overlays.default
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
              rust.stable."1.94.1".default.override {
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

      devShell =
        pkgs: with pkgs; rec {
          packages = [
            pkg-config
            openssl
            rustToolchain
            cargo-binstall
            pre-commit
            protobuf
            just
          ];

          env = {
            LD_LIBRARY_PATH = lib.makeLibraryPath packages;
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            PROTOC = "${protobuf}/bin/protoc";
            PROTOC_INCLUDE = "${protobuf}/include";
          };

          shellHook = ''
            export CARGO_HOME="$PWD/.cargo"
            export PATH="$CARGO_HOME/bin:$PWD/target/debug:$PATH"
          '';
        };
    };
}
