# No-WASI Rust Guest Build Research

## Sources

- [`wit-bindgen` project documentation](https://github.com/bytecodealliance/wit-bindgen)
- [`wit-bindgen` no-std Rust runtime fixture](https://github.com/bytecodealliance/wit-bindgen/blob/main/tests/runtime/rust/raw-strings/runner-nostd.rs)
- [`cargo-component` project documentation](https://github.com/bytecodealliance/cargo-component)
- [`wasm-tools` project](https://github.com/bytecodealliance/wasm-tools)

## Findings

- `wit-bindgen` generates guest bindings and export glue from a WIT world.
- Its official documentation describes a two-stage non-native build path:
  compile a core WebAssembly module, then wrap it with
  `wasm-tools component new`.
- `wit-bindgen` has no-std Rust fixtures using `alloc`, showing that generated
  guest bindings can avoid an operating-system/WASI dependency.
- `cargo-component` currently documents a WASI-based compilation/adaptation
  path for Rust components. It is not the preferred v0.1 path when the Host
  intentionally provides no WASI imports.
- The repository's current `wasm32-wasip2` target and `just build-example`
  command therefore conflict with the selected v0.1 sandbox contract.

## Recommended Proof Path

Before implementing the full adapter, create a minimal proof Component that:

1. uses the canonical Shiroha WIT world and `wit-bindgen`;
2. compiles for `wasm32-unknown-unknown` without WASI imports;
3. is wrapped by `wasm-tools component new`;
4. passes `wasm-tools component wit`/validation inspection; and
5. instantiates in a Wasmtime linker with no WASI interfaces registered.

The proof should determine whether the Rust guest SDK can use `std` safely on
`wasm32-unknown-unknown` or must be `no_std + alloc`. Do not commit the SDK API
or rewrite all build commands until this proof passes.

## Required Repository Changes If The Proof Passes

- Add `wasm32-unknown-unknown` to `rust-toolchain.toml` and Nix fallback targets.
- Add `wasm-tools` to development tooling or use a pinned equivalent command.
- Replace the current WASIp2 example recipe with the proven no-WASI pipeline.
- Keep `wasmtime-wasi` outside v0.1 runtime dependencies/features.
