# Research: Async Runtime

- **Query**: Confirm tokio as the async runtime given the WASM runtime's async requirements and bidirectional-streaming transport; note constraints the chosen WASM runtime imposes.
- **Scope**: external (verified via docs.rs) + analysis
- **Date**: 2026-06-25

## Findings

### Constraint sources

1. **WASM runtime (`wasmtime` 46.x)** — `async` feature (on by default) makes guest calls `Future`s driven on a separate native stack (fiber-based stack switching). Docs state explicitly: *"Wasmtime won't manage its own thread pools or similar, that's left up to the embedder."* The futures are **runtime-agnostic** — any executor that polls them works. tokio is only a *dev*-dependency of wasmtime, not a runtime requirement.
2. **Transport (`tonic` 0.14.x)** — built on `hyper` + `tower` + **`tokio`**. The `transport` feature (default) hard-depends on tokio I/O reactor. Bidirectional gRPC streaming (`Streaming<T>`) is tokio-driven.
3. **OpenTelemetry exporter (`opentelemetry-otlp`)** — OTLP/gRPC export path uses tonic → tokio.
4. **Epoch driver** — `Engine::increment_epoch()` must be called periodically; a `tokio::time::interval` task is the idiomatic driver.

### tokio — RECOMMENDED (and effectively forced)

- Covers all four constraints above with a single runtime.
- wasmtime's `call_async` futures can be `tokio::select!`ed with `tokio::time::timeout` → clean per-action timeout (recommended wasmtime pattern for bounding malicious/long wasm).
- tonic's bidi streaming maps directly onto Shiroha's orchestrator↔worker action dispatch + result回流.
- Industry default for Rust async servers; deep integration with `tracing` (Shiroha's observability layer).

**Verdict: tokio (full `rt-multi-thread` + `macros` + `signal` + `time`).** No realistic alternative given tonic.

### Why not `smol` / `async-std`

- **Incompatible with tonic**: tonic's `transport` module is tokio-only (no `async-std` hyper backend). Replacing tonic would mean abandoning the chosen gRPC stack.
- wasmtime *would* technically run under smol/async-std (runtime-agnostic futures), but you'd still need tokio for the transport — running two reactors is pointless complexity.
- `async-std` itself is low-activity; `smol` is fine but niche for a server framework of this shape.

### Why not a custom/no-runtime executor

- Possible to poll wasmtime futures manually, but you'd lose tonic, OTLP exporter, and tokio timeout/interval — re-implementing all of them. Not justified.

### Constraints the WASM runtime imposes on the async runtime

- **Per-instance `Store` is `!Sync`** (wasmtime) → one `Store<T>` per state-machine instance (task). The engine loop that polls in-flight action futures must own stores behind a task-local or `tokio::task::LocalSet` if store access must be single-threaded per instance; for multi-instance concurrency, run the engine on the multi-thread runtime but keep each instance's store on one thread (e.g. `tokio::task::spawn_local` inside a `LocalSet`, or `Arc<Mutex<Store>>` for coarse sharing). This is the main design constraint to carry into `design.md`.
- **`call_async` must be used consistently**: once any async-configured feature is on (async host funcs, epoch async-yield, async resource limiter), *all* wasm entry points must use `*_async` variants — sync variants error out.
- **No wasmtime-internal thread pool**: the embedder (tokio) supplies the executor that polls wasm futures. This is a feature, not a limitation — it lets Shiroha bound wasm compute via tokio task budgeting + epoch deadlines.
- **Epoch increment driver** must be a standalone tokio task (`tokio::spawn` + `interval`) calling `Engine::increment_epoch()`; paired with `Store::epoch_deadline_async_yield_and_update` per store.

### Recommendation

**tokio** as the sole async runtime, `rt-multi-thread` + `macros` + `time` + `signal` features. It is the only runtime compatible with the chosen transport (tonic) and is the natural driver for wasmtime async + epoch interruption + OTLP export. wasmtime does not *force* tokio, but the rest of the stack does, so standardize on tokio everywhere.

**Runner-up**: none practical. (A hypothetical smol-based stack would require replacing tonic with a smol-compatible gRPC — no mature option exists.)

### Risks / Caveats

- `Store` `!Sync` ↔ multi-thread runtime tension (see above) — resolve in `design.md` (likely per-instance `LocalSet` or `Arc<Mutex>`).
- Keep wasmtime and tokio versions loosely aligned (wasmtime dev-deps track recent tokio 1.x); no hard pin needed but avoid very old tokio.

## External References

- [wasmtime crate docs — Async section](https://docs.rs/wasmtime/latest/wasmtime/index.html#async) — "won't manage its own thread pools"; epoch + `tokio::time::timeout` pattern; `*_async`-only rule.
- [tonic docs (0.14.6)](https://docs.rs/tonic/latest/tonic/) — `transport` feature built on tokio/hyper/tower.

## Related Specs

- None yet; feeds R6.2 (async runtime) in `design.md`.
