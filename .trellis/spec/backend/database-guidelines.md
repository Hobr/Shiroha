# Persistence Boundary

> v0.1 deliberately has no database or durable storage implementation.

## Current Contract

The runtime returns a typed `MachineSnapshot`, but it does not serialize,
persist, migrate, lease, or replicate it. The embedding application owns any
temporary storage decision. Guest memory is never persistence.

The v0.x snapshot shape is not a stable wire or database schema. Do not derive a
durable format by serializing private Rust layout or debug output.

## Required Boundary

```rust
let snapshot: &MachineSnapshot = machine.snapshot();
// The caller may inspect/clone it. Shiroha v0.1 performs no database write.
```

Persistence belongs with the future Controller because it must coordinate task
authorization, lifecycle, idempotency, leases, and schema migration. Adding an
ORM or storage trait to Core before those contracts exist is forbidden.

## Future Trigger

When Controller persistence is implemented, replace this file with executable
contracts covering schema fields, indexes, transactions, optimistic/lease
concurrency, idempotency, migration/rollback commands, and snapshot versioning.
That work must include integration tests against the chosen database.

## Good / Bad

- Good: restore a machine from a typed snapshot produced by the same validated
  machine definition and release line.
- Bad: treat Component linear memory as durable state.
- Bad: add placeholder repository/database APIs with no v0.1 caller.

## Review Check

Any change that introduces a database dependency, serialized snapshot, or
storage abstraction is outside the current v0.1 scope and requires an approved
design plus a rewritten persistence spec.
