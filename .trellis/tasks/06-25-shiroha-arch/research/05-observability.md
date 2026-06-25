# Research: Observability (OpenTelemetry Rust)

- **Query**: Confirm the right OpenTelemetry Rust crates for the controller's per-task span/metric/log integration; note current maturity caveats (0.27+ restructuring, tracing-opentelemetry compatibility).
- **Scope**: external (verified via docs.rs) + analysis
- **Date**: 2026-06-25

## Findings

### Crate lineup (verified, all version-locked at 0.32)

| Crate | Version | License | Role |
|---|---|---|---|
| `opentelemetry` | 0.32.0 | Apache-2.0 | **API** crate (trace/metrics/logs-bridge). MSRV 1.70. |
| `opentelemetry_sdk` | 0.32.x | Apache-2.0 | **SDK** (providers, samplers, processors, exporters wiring) |
| `opentelemetry-otlp` | 0.32.x | Apache-2.0 | **OTLP exporter** (gRPC/HTTP) → collector |
| `opentelemetry-semantic-conventions` | 0.32.x | Apache-2.0 | Standard attribute names (`service.name`, RPC, messaging, …) |
| `tracing` | 0.1.x | MIT | Instrumentation API (spans/events) used throughout Shiroha |
| `tracing-opentelemetry` | 0.33.0 | MIT | **Bridge**: `tracing` spans → OTel spans; `MetricsLayer` for metrics. **Pins `opentelemetry ^0.32`** — must stay in lockstep. |
| `opentelemetry-appender-tracing` | 0.32.x | Apache-2.0 | **Logs bridge**: `tracing` events → OTel logs (tracing-opentelemetry does *not* do logs) |

### The 0.27+ restructuring — confirmed and understood

The Rust OTel SDK went through a major restructure around 0.27: the old single `opentelemetry` crate was **split into API (`opentelemetry`), SDK (`opentelemetry_sdk`), semantic conventions (`opentelemetry-semantic-conventions`), and per-exporter crates**, all now versioned in lockstep at **0.32**. Practical consequences:
- `opentelemetry` is **API-only** (no providers inside). You build providers via `opentelemetry_sdk::trace::SdkTracerProvider`, `SdkMeterProvider`, `LoggerProvider`.
- Exporters (`opentelemetry-otlp`, `opentelemetry-stdout`, `opentelemetry-prometheus`, `opentelemetry-zipkin`) are separate crates.
- **Version lockstep is mandatory**: `tracing-opentelemetry 0.33` depends on `opentelemetry ^0.32`; `opentelemetry-otlp`/`-semantic-conventions`/`-appender-tracing` must all be `0.32.x`. Bump them together or you get trait-mismatch compile errors. Pin a single `0.32.x` across the workspace.

### Maturity caveats (flagged)

- **`tracing-opentelemetry` docs state**: "The OpenTelemetry tracing specification is stable but the underlying opentelemetry crate is not [stable] so some breaking changes will still occur in this crate as well. **Metrics are not yet fully stable.**"
- So: **traces = production-ready**, **metrics = usable but expect churn**, **logs = bridge-only** (via `opentelemetry-appender-tracing`, experimental-ish).
- Every 0.x bump of `opentelemetry` is a **breaking change** and historically requires `tracing-opentelemetry` + exporter upgrades in lockstep. Budget for periodic migration. Isolate OTel setup in `shiroha-otel` so the rest of the workspace only touches `tracing`.

### How Shiroha uses it (controller per-task span/metric/log)

- **Instrumentation everywhere = `tracing`** (`#[tracing::instrument]`, `tracing::info_span!`, `tracing::info!`). Application code never imports `opentelemetry` directly.
- **`shiroha-otel` crate** owns the subscriber stack:
  - `tracing_subscriber::Registry` + `tracing_opentelemetry::layer().with_tracer(tracer)` (traces)
  - `tracing_opentelemetry::MetricsLayer` (metrics, behind feature)
  - `opentelemetry_appender_tracing::layer` (logs bridge)
  - Exporter: `opentelemetry_otlp` (OTLP/gRPC to a collector) — reuses tonic/tokio.
- **Per-task span ownership (R5.4)**: each state-machine instance (task) gets a root `tracing::Span` with `task_id` + `machine_name` attributes; child spans for each transition + each action execution; distributed actions carry the span context over the wire (W3C traceparent via `opentelemetry` propagation inject/extract in the transport layer). Result: a full trace tree per task spanning orchestrator + workers.
- **Metrics**: counters (`shiroha.tasks.created`, `shiroha.actions.dispatched`), histograms (`shiroha.action.duration`, `shiroha.transition.duration`), up-down counter (`shiroha.tasks.active`) — attribute by `task_id`/`machine_name`/`action_ref`/`worker_id`.
- **Logs**: `tracing` events flow to OTel logs via the appender; structured fields become OTel log attributes.

### Recommendation

- **`tracing` 0.1** as the sole instrumentation API across the whole workspace.
- **`opentelemetry` 0.32 + `opentelemetry_sdk` 0.32 + `opentelemetry-otlp` 0.32 + `opentelemetry-semantic-conventions` 0.32 + `tracing-opentelemetry` 0.33 + `opentelemetry-appender-tracing` 0.32**, all pinned to one `0.32.x` (and `tracing-opentelemetry 0.33.x`).
- **Isolate all OTel setup in `shiroha-otel`**; everything else depends only on `tracing` (+ `tracing-subscriber` where needed).
- **Traces now, metrics next, logs via appender** — reflect the maturity gradient in rollout order.
- Propagate trace context over the gRPC transport (inject `traceparent` into tonic metadata on dispatch; extract on the worker) so distributed-action traces stitch together.

**Runner-up**: none — `tracing` + OTel is the standard Rust observability stack. (If OTLP/gRPC collector is unavailable, swap `opentelemetry-otlp` for `opentelemetry-stdout` or `opentelemetry-prometheus` without touching instrumentation.)

### Risks / Caveats

- **Fast-moving 0.x** — every minor bump of `opentelemetry` breaks `tracing-opentelemetry` + exporters; pin and upgrade deliberately. Confirmed by docs: tracing stable, opentelemetry crate not stable, metrics not fully stable.
- **Metrics API churn** — prefer traces for critical observability MVP; treat metrics as best-effort until 1.0.
- **Logs bridge is experimental** — fine for structured logs to a collector, but don't build hard correctness on OTel logs yet.
- **Span context propagation over the wire** requires explicit inject/extract at the transport boundary — wire it into `shiroha-transport-grpc` metadata, not the abstract trait.

## External References

- [opentelemetry docs (0.32.0)](https://docs.rs/opentelemetry/latest/opentelemetry/) — API/SDK split, related crates, MSRV 1.70, feature flags.
- [tracing-opentelemetry docs (0.33.0)](https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/) — pins `opentelemetry ^0.32`; stability status (traces stable, metrics not fully stable); logs → use `opentelemetry-appender-tracing`; special `otel.*` fields.

## Related Specs

- None yet; feeds R5.4 + AC6 (controller OTel integration) in `design.md`.
