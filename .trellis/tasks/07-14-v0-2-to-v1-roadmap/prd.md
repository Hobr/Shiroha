# Shiroha v0.2.0 to v1.0.0 Roadmap

## Goal

Create a durable, capability-gated development roadmap from the completed
Shiroha `v0.1.0` library release through the first production-usable release,
`v1.0.0`. The roadmap must make `v0.2.0` actionable, preserve the current
Host/WASM architecture, order later work by real dependencies, define
production readiness with testable gates, and list future implementation task
candidates without creating those tasks now.

## Background

- The workspace version is `0.1.0` (`Cargo.toml:20-25`).
- The current release is a local Rust library with a deterministic flat FSM,
  Host-owned atomic state/context commits, typed Wasmtime Component execution,
  canonical WIT, a Rust guest SDK, finite limits, tracing, tests, benchmarks,
  and a runnable example (`README.md:3-23`).
- Controller services, stateless Nodes, distributed scheduling, `sctl`, text
  adapters, plugins, task authorization, and configurable capability policy are
  explicit pre-v1 milestones (`README.md:25-27`, `README.md:158-164`).
- Pre-v1 Host IR and WIT may change incompatibly. v1 is the first stable
  production contract (`README.md:152-156`; archived foundation
  `design.md:615-628`).
- Multi-Controller consensus and failover are intentionally post-v1
  (`README.md:166-167`).
- The completed foundation forbids placeholder Controller/Node/plugin APIs
  without an executable consumer (archived foundation `prd.md:266-284`).
- On 2026-07-14, `nix develop -c just test` rebuilt the example Component and
  passed 41 of 41 tests. This is the current verification baseline.
- A deleted pre-remake roadmap is useful only for its product principles:
  deliver an executable early, validate distributed execution before broad
  plugin abstractions, and give every release one primary proof. Its version
  completion claims and old crate layout are not current project status
  (`git show 0dd118a:.trellis/docs/version-roadmap.md`).

## Requirements

### R1. Evidence And Current-State Accuracy

The roadmap must use the current source, tests, manifests, README, active
specs, and archived v0.1 foundation task as its source of truth. Historical
pre-remake code may explain prior product intent but must not be presented as
implemented functionality.

Every completed status in `ROADMAP.md` must point to current executable
evidence. Planned releases remain planned until their release gates pass.

### R2. Public Roadmap Contract

The public artifact must be a Simplified-Chinese root-level `ROADMAP.md`, linked
from a Simplified-Chinese README. Code samples, commands, identifiers, protocol
names, crate names, and relative links remain unchanged. Both documents must be
understandable without Trellis context.

For every release, it must state:

- the primary goal and user-visible value;
- included capability groups and explicit non-goals;
- dependencies on prior releases;
- one end-to-end validation point;
- objective release gates; and
- bounded future task candidates.

Patch releases are compatible stabilization trains, not feature milestones.
The roadmap must contain no release dates, quarters, duration estimates, or
calendar commitments. Scheduling happens only after a version is decomposed
and contributor capacity is known.

### R3. Release Sequence

The dependency-ordered release gates are:

1. `v0.2.0` - installable local executable and control loop.
2. `v0.3.0` - HSM semantics, then `redb` durability, full REST control API,
   and operational `sctl`.
3. `v0.4.0` - trusted-caller and Node authentication foundations,
   framework-level request validation, and configurable WASI capabilities.
4. `v0.5.0` - stateless Nodes and single remote Action execution over
   Protobuf/gRPC.
5. `v0.6.0` - capacity-aware scheduling, fan-out/fan-in aggregation,
   at-least-once delivery, idempotency-aware retries, and recovery.
6. `v0.7.0` - JSON/TOML adapters, plugin registry, independent WASM Action and
   Aggregator Components, and a production-quality HTTP Action plugin.
7. `v0.8.0` - OpenTelemetry, operational hardening, role-scoped builds,
   packaging, and supported deployment workflows.
8. `v0.9.0` - public API/WIT/IR/protocol freeze, migrations, compatibility
   testing, upgrade rehearsal, and release-candidate hardening.
9. `v1.0.0` - promotion of the qualified stable production contract.

#### R3.1. v0.2.0 Boundary

`v0.2.0` must turn the library into a locally operable vertical slice:

- a `shirohad` executable running the current Host/WASM runtime in local mode;
- an in-memory Controller-owned task lifecycle that loads a Component, starts
  a task, dispatches input, reports committed state, and stops the task;
- a small local REST API and a basic `sctl` client that uses it;
- structured process logging and clean startup/shutdown behavior; and
- process-level end-to-end tests driven through `sctl`.

Persistence, restart recovery, HSM, remote Nodes, distributed scheduling,
production authentication, configurable WASI grants, and the plugin system are
explicit v0.2 non-goals.

### R4. Cross-Version Product Contracts

#### R4.1. State-Machine Evolution

`v0.2.0` exposes the completed flat FSM. `v0.3.0` must add HSM semantics before
the durable Controller schema settles. HSM support must include nested states,
deterministic active leaf/path snapshots, initial-child and terminal
propagation, ancestor-aware transition selection, least-common-ancestor
exit/entry paths, fixed callback/action ordering, atomic commits, and restart
recovery of the exact active path.

Full Statechart semantics, including concurrent regions and history states,
are post-v1. Pre-v1 must not add non-functional placeholder fields for them.

#### R4.2. Controller, REST, And Persistence

The supported v1 topology is one authoritative Controller using a local
persistent volume plus zero or more stateless Nodes. `redb` is the sole required
database through v1; SQLite, PostgreSQL, shared storage, and a portable
multi-backend contract are post-v1.

From v0.3 onward, the Controller must durably preserve authoritative workflow
state, including definitions and artifact identity, committed HSM snapshots,
pending work, retry/idempotency state, and recovery bookkeeping. The storage
contract must cover atomic transactions, explicit schema versions, forward
migrations, crash recovery, backup/restore, integrity checks, bounded
retention/compaction, and tests against real `redb` files.

Applications, browser-facing services, and `sctl` call a versioned REST/JSON
Controller API described by OpenAPI. Controller-to-Node communication uses a
separate versioned Protobuf/gRPC protocol. Both map through shared domain types
without exposing storage or runtime internals. Public gRPC and gRPC-Web are not
required before v1.

#### R4.3. Security Responsibility Boundary

The Controller is a framework capability endpoint, not an application identity
or business authorization system. It must not define users, roles, RBAC,
organizations, tenants, or product permissions. Web/App integrations own user
authentication and business authorization.

Framework-level security must include:

- TLS plus configurable, rotatable high-entropy Bearer Service Tokens on
  production REST endpoints; tokens prove service trust only and carry no
  Shiroha roles or scopes;
- explicit loopback-only opt-in for unauthenticated local development;
- mandatory mTLS and distinct Node identity for Controller-to-Node gRPC;
- request, artifact, size, state, replay, and idempotency validation;
- finite resource limits and fail-closed import/capability checks; and
- security-relevant audit records that do not interpret application roles or
  log credentials/payloads.

#### R4.4. Task/WASI Capability Policy

Operators define named Capability Profiles. A Task/Component declares its
required capabilities, and the Controller accepts the task only when the
request fits an operator profile. Profiles are runtime sandbox bundles, not
application roles.

The policy must default deny, bind the request to an artifact digest, validate
imports, constrain supported Host/WASI authority, issue an integrity-protected
execution grant, and require the Node to verify that grant and prove it can
enforce every capability. Missing, stale, tampered, undeclared, unsupported, or
over-broad grants must fail closed. Service Tokens do not select or limit
profiles; Web/App decides which users may request them while Controller enforces
the operator's maximum sandbox boundary.

#### R4.5. Distributed Delivery And Recovery

Remote Actions use at-least-once delivery. Every execution has a durable
Activity ID and idempotency key. Controller persistence must ensure scheduling
intent is committed before dispatch and an accepted result advances workflow
state at most once.

Only Actions explicitly declared idempotent may be retried automatically.
Retries are bounded by attempts and deadlines and use backoff/jitter. When a
non-idempotent Action may have produced an external effect but no authoritative
result is available, the workflow enters an operator-visible `outcome unknown`
condition. Cancellation is best effort and never claims to roll back external
effects. Shiroha must not claim universal exactly-once side effects.

#### R4.6. Fan-Out, Aggregation, And Scheduling

The v1 scheduler supports one eligible Node, an explicit replica count, or all
eligible Nodes. Built-in aggregation policies are `first-success`, `all`, and
`quorum(k)`, where quorum means a count of accepted successful results rather
than an undefined equality vote over opaque payloads. A bounded,
side-effect-free custom WASM Aggregator is also required.

Node eligibility is determined from heartbeat lease, protocol/runtime
compatibility, labels, executor kinds, enforceable capabilities, and bounded
capacity. Eligible Nodes are ranked by available capacity with round-robin as
a tie-breaker. Nodes enforce bounded concurrency and queues and report
backpressure. Candidate and selected Nodes are persisted in an immutable
dispatch plan before execution.

Aggregation must define stable input identity/order and deterministic behavior
for partial success, timeout, Node loss, cancellation, late results, duplicate
results, and Controller restart.

#### R4.7. Extension Platform

`v0.7.0` must provide JSON and TOML definition adapters that produce the same
validated Host IR as the WASM adapter, registry contracts populated from
startup configuration, independent WASM Action/Aggregator Components, and one
production-quality HTTP Action plugin. The HTTP plugin must exercise network
capability authorization, configuration, deadlines, bounded payloads,
diagnostics, and safe retry behavior.

Native Rust dynamic libraries, Bash, YAML, NATS plugins, hot reload, plugin
marketplaces, and broad built-in Action catalogs are not required before v1.

#### R4.8. Operations And Distribution Artifacts

Before v1, Shiroha must provide OpenTelemetry-compatible traces and metrics,
safe structured logs, audit export, health/readiness endpoints, graceful
shutdown, backup/restore and upgrade runbooks, load/soak/fault tests, and
operator diagnostics.

The release publishes:

- signed and checksummed Linux `x86_64`/`aarch64` `shirohad` and `sctl`
  binaries;
- reproducible `full`, `controller`, and `node` Cargo-feature builds;
- non-root role-specific OCI images with SBOMs and signatures;
- Docker Compose examples, hardened systemd examples, and a versioned Helm
  Chart; and
- the Nix Flake as the supported development/reproducible-build environment.

A Kubernetes Operator, autoscaling, non-Linux production binaries, and a
managed cloud service are post-v1.

#### R4.9. Stable Rust And Compatibility Surface

The intended stable crates.io surface is:

- `shiroha` for the async Host facade;
- `shiroha-core` for runtime-neutral IR and intentional Adapter/Executor SPI;
- `shiroha-guest` for Machine Component authoring; and
- a focused v0.7 SDK surface for independent Action/Aggregator Components.

Wasmtime, Controller, Node, scheduler, `redb`, server, and transport
implementations remain internal. The package graph must be restructured before
publication so a public crate does not accidentally expose an internal crate as
a stable direct-dependency API.

`v0.9.0` freezes the public Rust APIs/features, REST/OpenAPI, Node Protobuf,
WIT/IR, persisted schema/migrations, CLI/config, MSRV policy, deprecations, and
compatibility matrix. v1 adds no new feature after that freeze; it promotes the
release candidate after all gates pass.

### R5. Roadmap Task Decomposition

This Trellis task owns only `ROADMAP.md`, its README link, and planning
evidence. The roadmap must list bounded future task candidates for every
release, but this task must not create child or implementation tasks. Future
tasks are created one at a time after the roadmap is approved and the owning
release begins. Dependencies must be written explicitly in each future task,
not inferred from task-tree position.

## Acceptance Criteria

- [x] **AC1:** Root `ROADMAP.md` marks v0.1.0 complete, v0.2.0 next, and
      v1.0.0 the first production-usable release.
- [x] **AC2:** Every release from v0.2.0 through v1.0.0 includes goal, user
      value, scope, non-goals, dependencies, validation point, release gates,
      and future task candidates.
- [x] **AC3:** Every deferred pre-v1 capability in `README.md:158-164` and the
      archived foundation `prd.md:266-281` is assigned or explicitly deferred
      with rationale.
- [x] **AC4:** The dependency order places HSM before durable schema, security
      before remote execution, basic remote execution before retries/fan-out,
      and operational hardening before compatibility freeze.
- [x] **AC5:** The roadmap preserves Core neutrality and Host-owned atomic state
      across REST, `redb`, gRPC, Node, plugin, and WASM boundaries.
- [x] **AC6:** v1 readiness gates cover `redb` durability/recovery/migrations,
      distributed failure semantics, security/capabilities, observability,
      operations, compatibility, packaging, documentation, and role-specific
      tests.
- [x] **AC7:** The security section assigns application roles to Web/App and
      limits Controller security to trusted-service/Node authentication,
      framework validation, resource bounds, and Capability Profiles.
- [x] **AC8:** At-least-once semantics, idempotency-aware retries, unknown
      outcomes, best-effort cancellation, deterministic aggregation, and the
      absence of a general exactly-once claim are explicit.
- [x] **AC9:** Public crates and every REST/Protobuf/WIT/IR/storage/config
      contract have a v0.9 freeze and compatibility gate.
- [x] **AC10:** Multi-Controller operation and all other post-v1 exclusions are
      explicit, and no deleted prototype is presented as implemented.
- [x] **AC11:** README links to `ROADMAP.md` without contradicting its detailed
      release sequence.
- [x] **AC12:** No dates are invented and no future implementation or child
      tasks are created by this planning task.
- [x] **AC13:** Documentation formatting, links, terminology checks, and
      `git diff --check` pass after implementation.
- [ ] **AC14:** README and `ROADMAP.md` explanatory prose, headings, tables,
      statuses, and release gates are fully translated into Simplified Chinese
      while code, commands, identifiers, and links retain their literal forms.

## Out Of Scope

- Implementing any product capability planned for v0.2.0 through v1.0.0.
- Creating future release implementation tasks or child tasks.
- Calendar commitments or effort estimates.
- Full Statecharts with concurrent regions/history before v1.
- Multi-Controller consensus, failover, active-active operation, or shared
  database support.
- Controller-owned users, roles, RBAC, tenants, organizations, or business
  authorization.
- Universal exactly-once external side effects.
- Advanced scheduler priority/preemption, complex affinity, resource
  reservation, or autoscaling.
- Streaming DAG/MapReduce or general distributed-compute APIs.
- SQLite/PostgreSQL backends before v1.
- Native Rust dynamic plugins, Bash, YAML, NATS plugins, hot reload, or a plugin
  marketplace before v1.
- Kubernetes Operator, managed cloud service, and non-Linux production targets.
- Compatibility with deleted pre-remake prototypes.
