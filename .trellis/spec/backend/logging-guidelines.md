# Logging Guidelines

> Structured tracing without application payload disclosure.

## Overview

Libraries emit structured `tracing` spans/events and never install a subscriber
or print operational diagnostics. The embedding application chooses formatting,
export, and OpenTelemetry integration.

## Spans And Levels

| Name / level | Required use |
|---|---|
| `shiroha.prepare` | Component preparation; field `artifact_bytes` |
| `shiroha.validate` | Definition validation; machine ID and state count |
| `shiroha.start` | Initial machine entry; machine ID |
| `shiroha.dispatch` | Public input run; machine/instance/state/sequence |
| `shiroha.step` | One run-to-completion microstep |
| `shiroha.guest.guard` | Guard call; function locator |
| `shiroha.guest.action` | Action call; function locator |
| `shiroha.guest.callback` | Callback call; function locator |
| `info!` | Successful preparation and committed transitions |
| `warn!` | Validation warnings and runtime step faults |
| `debug!` | Unhandled inputs and low-level executor state |

Do not emit `error!` merely because a library returns an error; the application
decides whether the returned failure is operationally fatal.

## Structured Fields

Use identifiers and bounded scalar metadata: machine ID, instance ID, state,
sequence, transition index, function locator, import/state count, artifact
length, fault kind, and executor-poisoned flag. Use `#[instrument(skip_all)]`
and declare safe fields explicitly.

## What Not To Log

Never record payload bytes, application context, guest error payloads, WASI
environment values, credentials, or inherited process data. Do not derive a
field by formatting a whole `HostInput`, snapshot, or hook input.

```rust
// Wrong: context may contain secrets.
debug!(?input, ?snapshot, "dispatch");

// Correct: bounded identifiers only.
debug!(state = %snapshot.state, sequence = snapshot.sequence, "dispatch");
```

Tests and manual review must confirm payload data is absent from default spans.
