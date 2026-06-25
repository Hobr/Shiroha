# Research: Transport (gRPC default + abstract Transport trait)

- **Query**: Confirm tonic (gRPC) for the default `Transport` impl with bidirectional streaming (orchestrator→worker action dispatch, worker→orchestrator result回流); sketch proto/service shape; shape the abstract `Transport` trait so libp2p/QUIC/custom can replace it; confirm prost for protobuf.
- **Scope**: external (verified via docs.rs) + analysis
- **Date**: 2026-06-25

## Findings

### Crate selection (verified)

| Crate | Version | License | Role | Notes |
|---|---|---|---|---|
| `tonic` | 0.14.6 | MIT | gRPC over HTTP/2 server + client, bidi streaming | built on tokio/hyper/tower; `transport` feature default; TLS via rustls (`tls-ring`/`tls-aws-lc`) |
| `prost` | 0.14.4 | Apache-2.0 | protobuf codec | tokio-rs org; MSRV 1.85; "passively maintained" but de-facto standard paired with tonic; `prost-build` needs `protoc` |
| `tonic-build` | 0.14.x | MIT | build-time codegen | wires `.proto` → prost + tonic service traits |

**Confirmed: tonic + prost for the default gRPC `Transport`.** Bidirectional streaming is a first-class tonic primitive (`Streaming<T>` for the response stream; `IntoStreamingRequest` for the request stream; `streaming` RPC kind generates `impl Stream<Item=Req>` client + `Request<Streaming<Req>>` server).

### Why tonic fits Shiroha's L2 scheduler transport

- **Bidirectional streaming RPC** = exactly the orchestrator↔worker channel: orchestrator streams `ActionDispatch` requests out; worker streams `ActionResult` responses back over the *same* long-lived RPC. No per-action round-trip connection setup.
- **Stateless workers (R4.2)** map onto a tonic server with no per-session state; the orchestrator is a tonic client holding N open bidi streams to N workers.
- **TLS optional (R5.5)** — `tls-ring`/`tls-aws-lc` + `tls-webpki-roots`/`tls-native-roots` features; mTLS feasible for orchestrator↔worker trust.
- **tokio-native** — shares the single runtime with wasmtime async + OTLP exporter (see `02-async-runtime.md`).
- **Load balancing / reconnect / keepalive / timeouts** built into `tonic::transport::Channel` — useful for worker pool management.

### Proto / service sketch

```proto
syntax = "proto3";
package shiroha.scheduler.v1;

// One bidi stream per worker connection: orchestrator pushes dispatches,
// worker pushes results back on the SAME stream.
service Dispatch {
  rpc Dispatch(stream ActionDispatch) returns (stream ActionResult);
}

message ActionDispatch {
  string task_id     = 1;   // state-machine instance id
  string action_ref  = 2;   // {kind, ref} resolved to a callable, e.g. wasm export name
  bytes  input       = 3;   // opaque action input (canonical action ABI payload)
  // capability requirements so the worker can validate it can execute this action
  repeated string required_capabilities = 4;
  uint32 deadline_ms = 5;   // per-action timeout hint
  // for fan-out sharded distributed actions (R4.1)
  optional ShardHint shard = 6;
}

message ActionResult {
  string task_id    = 1;
  string action_ref = 2;
  oneof outcome {
    Done  done  = 3;   // -> engine raises done.<action>
    Erred error = 4;   // -> engine raises error.<action>
  }
  message Done  { bytes payload = 1; }
  message Erred { string code = 1; string message = 2; }
}

message ShardHint { uint32 shard_index = 1; uint32 shard_count = 2; }
```
Notes:
- `action_ref` + `input` keep the wire payload **opaque to the transport** — workers execute via their own action ABI (wasm `list<u8> -> result<list<u8>, string>` per `01-wasm-runtime.md`). The transport never decodes action semantics.
- For **aggregation (R4.4: all/any/quorum(n)/first-success)** the orchestrator (not the worker) correlates `ActionResult`s by `task_id`+`action_ref` and applies the strategy; the proto stays strategy-agnostic.
- `required_capabilities` lets a worker reject an action it can't fulfill before executing (action-capability validation, R5.5).

### Abstract `Transport` trait (so libp2p/QUIC/custom can replace tonic)

Keep the gRPC details **behind** a transport-agnostic trait in `shiroha-transport`; `shiroha-transport-grpc` is the default impl.

```rust
// shiroha-transport — no tonic dependency here
#[async_trait::async_trait]
pub trait Transport: Send + Sync + 'static {
    type DispatchSink: Sink<ActionDispatch, Error = TransportError> + Send + Unpin + 'static;
    type ResultStream: Stream<Item = Result<ActionResult, TransportError>> + Send + Unpin + 'static;

    /// Open (or return a pooled) bidi channel to a worker endpoint.
    async fn connect(&self, endpoint: &Endpoint) -> Result<(Self::DispatchSink, Self::ResultStream), TransportError>;
}

pub struct Endpoint { pub addr: String, pub tls: TlsConfig }   // abstract enough for gRPC / libp2p multiaddr / QUIC
```
- The scheduler holds `Box<dyn Transport<...>>` (or a generic `T: Transport`) and drives `DispatchSink`/`ResultStream` with tokio. Swapping in libp2p/QUIC = implement `Transport` over their stream/sink types.
- `ActionDispatch`/`ActionResult` are **transport-domain types** (plain Rust structs in `shiroha-transport`), *not* prost types — `shiroha-transport-grpc` maps them to/from the generated prost types at the boundary. This keeps the trait prost-free and lets a non-prost transport (raw QUIC + postcard, libp2p + CBOR) reuse the same domain types.

### Recommendation

- **Default `Transport` impl: tonic 0.14 + prost 0.14** (`shiroha-transport-grpc`), bidi `Dispatch` streaming RPC per the sketch.
- **Abstract `Transport` trait** in `shiroha-transport` (no prost/tonic dep); domain `ActionDispatch`/`ActionResult` structs live here.
- **prost confirmed** for protobuf; `tonic-build` for codegen; `protoc` required at build time.
- TLS via tonic's rustls features (`tls-ring` or `tls-aws-lc`), optional per R5.5.

**Runner-up transport**: raw **QUIC** (`quinn` + `tokio`) or **libp2p** — implement the same `Transport` trait when NAT-traversal/multiparty/P2P worker discovery is needed. Both are tokio-native and can carry the same domain types with a different codec (postcard/CBOR). Not needed for the MVP single-orchestrator topology (R6.5).

### Risks / Caveats

- **`prost` "passively maintained"** — still the de-facto Rust protobuf and the only first-class tonic codec; bug/security fixes continue. The maintainer expects Google's official `protobuf` Rust crate eventually, but it is not tonic-ready today. Accept the risk; the abstract trait isolates the wire format anyway.
- **`protoc` build dependency** — `prost-build` invokes `protoc` (no longer bundled). Ensure `protoc` is in the build image / `devshell`; alternatively vendor a `protoc` via `protoc-bin-vendored` for hermetic CI.
- **Bidi stream backpressure** — tonic streams have tokio backpressure; the scheduler must respect `Sink::poll_flush`/credit or it can stall a worker. Design the correlation loop carefully.
- **Worker authentication** — for MVP use a shared API token in gRPC metadata (R5.5); mTLS for the transport-encryption-enabled mode.

## External References

- [tonic docs (0.14.6)](https://docs.rs/tonic/latest/tonic/) — `transport` feature, `Streaming<T>`, TLS feature flags.
- [prost docs (0.14.4)](https://docs.rs/prost/latest/prost/) — derive-based protobuf, MSRV 1.85, `prost-build` + `protoc`, maintenance status.

## Related Specs

- None yet; feeds R6.4 + AC5 (scheduler transport/dispatch contract) in `design.md`.
