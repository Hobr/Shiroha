# Backend Development Guidelines

> Best practices for backend development in this project.

---

## Overview

This directory contains guidelines for backend development. Fill in each file with your project's specific conventions.

---

## Guidelines Index

| Guide | Description | Status |
|-------|-------------|--------|
| [Rust Workspace Structure](./rust-workspace-structure.md) | Crate organization, dependency layers, workspace manifest, extension point placeholders | Active |
| [WASM Component Integration](./wasm-component-integration.md) | WIT contracts, host/guest bindings, type mapping, async strategy | Active |
| [Plugin Architecture](./plugin-architecture.md) | Two-layer ActionRef semantics, PluginRegistry pattern, extension points, ActionKind evolution | Active |
| [HSM Implementation Pattern](./hsm-implementation-pattern.md) | Hierarchical state machine runtime, RTC loop, do-activity lifecycle | Active |
| [Daemon Architecture](./daemon-architecture.md) | Multi-component management, control interface, concurrency model, shutdown coordination | Active |
| [Async Patterns](./async-patterns.md) | Tokio runtime guidelines, state access patterns, graceful shutdown | Active |
| [Quality Guidelines](./quality-guidelines.md) | Quality gates, forbidden patterns, testing requirements, code review checklist | Active |
| [Git Commit Conventions](./git-commit-conventions.md) | Commit cadence, message format, pre-commit gate | Active |
| [Cargo Conventions](./cargo-conventions.md) | Package init via cargo init, latest-version dependency policy | Active |
| [Directory Structure](./directory-structure.md) | Module organization and file layout | To fill |
| [Database Guidelines](./database-guidelines.md) | ORM patterns, queries, migrations | To fill |
| [Error Handling](./error-handling.md) | Error types, handling strategies | To fill |
| [Logging Guidelines](./logging-guidelines.md) | Structured logging, log levels | To fill |

---

## How to Fill These Guidelines

For each guideline file:

1. Document your project's **actual conventions** (not ideals)
2. Include **code examples** from your codebase
3. List **forbidden patterns** and why
4. Add **common mistakes** your team has made

The goal is to help AI assistants and new team members understand how YOUR project works.

---

**Language**: All documentation should be written in **English**.
