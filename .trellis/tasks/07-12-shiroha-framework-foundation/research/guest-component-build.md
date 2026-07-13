# WASIp2 Guest And Import-Policy Research

## Sources

- [Rust `wasm32-wasip2` target documentation](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip2.html)
- [Rust `wasm32-unknown-unknown` target documentation](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html)
- [`wit-bindgen` project documentation](https://github.com/bytecodealliance/wit-bindgen)
- [`wasm-tools` project documentation](https://github.com/bytecodealliance/wasm-tools)
- [Wasmtime v46 Component Linker](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/runtime/component/linker.rs)

## Findings

- `wasm32-wasip2` is a native Rust Component target and supports custom WIT
  worlds through `wit-bindgen`.
- A compilation target does not itself grant Host capabilities. The final
  Component's imports state what the Host must satisfy.
- Wasmtime's Component Linker rejects missing imports by default during
  instantiation. It also offers an explicit unknown-import-as-trap mechanism,
  which Shiroha v0.1 must not enable.
- Rust `std` operations that require an operating environment may add WASI
  imports. The final artifact must be inspected; source-level conventions alone
  do not prove a Component is zero-authority.
- A runtime need not know which compiler target produced a Component. Final WIT
  compatibility and the import policy are the enforceable artifact contract.

## Selected v0.1 Policy

1. The official Rust guest target is `wasm32-wasip2`.
2. The canonical Shiroha world declares no imports in v0.1.
3. The Host registers no `wasmtime-wasi` interfaces.
4. Preparation rejects every `wasi:*` import as `WasiNotEnabled` and every other
   unsupported import as `UnsupportedImport` before instantiation.
5. The official example is validated with `wasm-tools component wit` and an
   empty-linker smoke test.
6. Components produced through other targets/toolchains remain eligible when
   their final Component implements the Shiroha world and passes the same import
   policy.
7. Future WASI support is introduced through task authorization and explicit
   capability grants, not by changing the official Rust target.

## Phase 0 Proof

Build a minimal custom-world Rust Component with `wasm32-wasip2`, inspect its
imports, and instantiate it through an empty Wasmtime Linker. Also build a
negative Component with a WASI import and prove that preparation rejects it.

The proof records which Rust `std` facilities keep the final import set empty.
If the official example cannot remain zero-import, revise the SDK/example before
implementing the full adapter rather than silently registering WASI.
