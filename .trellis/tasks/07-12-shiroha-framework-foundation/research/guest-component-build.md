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
- Wasmtime 46.0.1's `wasmtime_wasi::p2::add_to_linker_async` registrations are
  generated from `wasi:cli/imports@0.2.12`. The policy must mirror its exact
  stable interface names and supported 0.2.x patches; matching only family
  prefixes would misclassify an unknown interface such as
  `wasi:cli/not-real@0.2.9` as baseline WASI.
- With Rust 1.97.0 and `wit-bindgen` 0.59.0, even a minimal custom-world
  Component using ordinary Rust `std` declares WASI 0.2 imports. The observed
  set includes `wasi:io`, `wasi:clocks/monotonic-clock`, and `wasi:cli`
  streams, environment, exit, and terminal interfaces at version 0.2.9.
- A `no_std + alloc` guest can be made zero-import by supplying an allocator,
  panic handler, and the Canonical ABI `cabi_realloc` export. That profile is
  technically viable but adds authoring and SDK complexity that v0.1 no longer
  requires.
- A runtime need not know which compiler target produced a Component. Final WIT
  compatibility and the import policy are the enforceable artifact contract.

## Selected v0.1 Policy

1. The official Rust guest target is `wasm32-wasip2`.
2. The canonical Shiroha world declares no imports in v0.1.
3. The Host registers Wasmtime's standard WASI 0.2 interfaces using a minimally
   configured context with no explicit Host directory preopens, inherited
   environment/arguments, or networking.
4. Preparation records declared imports and accepts only exact stable
   interfaces registered by the pinned Preview 2 linker through version
   0.2.12. Unknown interfaces, newer patches, and non-WASI imports are rejected
   as `UnsupportedImports` before linker/interface loading.
5. The official example is validated with `wasm-tools component wit` and a
   baseline-WASI-linker smoke test.
6. Components produced through other targets/toolchains remain eligible when
   their final Component implements the Shiroha world and the active Host
   profile satisfies their imports.
7. Per-task authorization and configurable WASI capability grants remain pre-v1
   work; the v0.1 linker/context construction is kept behind a policy boundary.

## Phase 0 Proof

The minimal custom-world Rust Component builds successfully with
`wasm32-wasip2`, validates with `wasm-tools`, and exposes the expected `echo`
export plus the standard WASI imports listed above. A separate `no_std` probe
proved that zero-import output is possible, but the user selected the simpler
ordinary-`std` profile for v0.1.

The Host smoke test now proves that the ordinary-`std` Component instantiates
and its `echo` export runs after `wasmtime_wasi::p2::add_to_linker_sync` is
registered against an unmodified `WasiCtxBuilder::new()` context. Wasmtime 46
documents this default as closed stdin, sink stdout/stderr, no environment,
arguments, or preopens, and network addresses denied by default.

The original negative Component adds the custom `wasi:probe/clock` interface
and fails with the baseline linker, proving that standard WASI registration
does not silently install trap stubs. The final adapter adds preparation tests
for an unknown interface inside an otherwise allowed family
(`wasi:cli/not-real@0.2.9`), a version newer than the pinned linker
(`wasi:io/poll@0.2.13`), and a non-WASI interface (`example:host/api`). All
three must return `UnsupportedImports` before typed interface loading. Phase 0
and the final regression tests therefore validate the selected v0.1 profile.
