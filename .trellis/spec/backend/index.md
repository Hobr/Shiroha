# Backend Development Guidelines

> Executable conventions for Shiroha's Rust runtime workspace.

## Overview

Shiroha v0.1 is a Rust library workspace with a runtime-neutral state-machine
Core, a typed Wasmtime Component adapter, a Rust guest SDK, and a public facade.
These files document conventions established by the executable v0.1 code.

## Guidelines Index

| Guide | Description | Status |
|---|---|---|
| [Runtime and Component Contract](./runtime-contract.md) | Cross-crate signatures, state semantics, validation, and WASM boundary | Active |
| [Directory Structure](./directory-structure.md) | Workspace layers and dependency direction | Active |
| [Persistence Boundary](./database-guidelines.md) | Explicit v0.1 non-persistence scope | Active |
| [Error Handling](./error-handling.md) | Typed errors, rollback, and classification | Active |
| [Quality Guidelines](./quality-guidelines.md) | Toolchain, formatting gates, module-scoped commits, tests, and forbidden patterns | Active |
| [Logging Guidelines](./logging-guidelines.md) | Structured spans, fields, and sensitive data rules | Active |

## Reading Order

Start with the runtime contract for any cross-layer change. Then read the
structure and topic-specific file for the package being changed. Controller,
Node, scheduler, CLI, persistence, plugin, and capability work must update these
specs when those features gain executable implementations.

---

**Language**: All documentation should be written in **English**.
