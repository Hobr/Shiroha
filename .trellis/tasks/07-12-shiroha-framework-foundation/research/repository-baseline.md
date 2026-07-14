# Repository Baseline

## Current State

- The Cargo workspace has no members and contains no Shiroha Rust source yet.
- `Cargo.toml` declares workspace version `0.1.0`, Rust edition 2024, and
  `rust-version = "1.97.0"`.
- `rust-toolchain.toml` pins Rust `1.97.0` and installs only the
  `wasm32-wasip2` guest target.
- `justfile` already names the future `sctl`, `shirohad`, and `example`
  packages, but those packages do not exist. Its example build targets
  `wasm32-wasip2`.
- Workspace dependencies already include Tokio, Wasmtime 46.0.1 Component
  Model support, `wasmtime-wasi`, tonic/prost, tracing, serde, clap, and the
  expected controller/networking libraries.
- The backend Trellis specification files are placeholders rather than
  established project conventions.

## Planning Implications

1. v0.1 implementation begins from a clean crate architecture rather than
   modifying an existing runtime.
2. The current `justfile` is aspirational and must be revised to match the
   core-first v0.1 scope; it is not evidence that controller/CLI crates already
   exist.
3. The existing `wasm32-wasip2` target matches the selected official Rust guest
   target. The Phase 0 spike confirms that an ordinary Rust `std` custom-world
   Component carries standard WASI 0.2 imports which the Host must satisfy.
4. `wasmtime-wasi` is therefore required by the v0.1 WASM adapter, but it must
   not leak into `shiroha-core` or imply configurable capability policy.
5. The workspace package requirement and pinned development toolchain both use
   Rust `1.97.0`.
