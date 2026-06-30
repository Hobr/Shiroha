# Shiroha

> A *WebAssembly*-extensible workflow orchestration engine built around *Finite-State Machines*.

## Quick Start

```bash
# Build the project
cargo build --release

# Build the example WASM component
cargo build --target wasm32-wasip2 -p shiroha-sm-example

# Run the daemon with the example component
./target/release/shirohad --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm
```

Expected output:
```
2026-06-30T15:53:49.019404Z  INFO shirohad: Starting Shiroha daemon
2026-06-30T15:53:49.019435Z  INFO shirohad: Component path: ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm
2026-06-30T15:53:49.019808Z  INFO shirohad: Loading component...
2026-06-30T15:53:49.366166Z  INFO shirohad: Component loaded: 3 states, 2 transitions, 2 events
2026-06-30T15:53:49.366192Z  INFO shirohad: Initial state: Idle
2026-06-30T15:53:49.685057Z  INFO shirohad: Creating task: id=default
2026-06-30T15:53:49.685152Z  INFO shirohad: Task created successfully
2026-06-30T15:53:49.685160Z  INFO shirohad: Daemon running, press Ctrl-C to stop
```

Press `Ctrl-C` to gracefully shut down the daemon.

## Dev Setup

```bash
# Environment(Nix)
apt install -y direnv
echo 'use flake' > .envrc
direnv allow

# Environment
apt install -y rustup cargo-binstall protobuf-compiler pre-commit just

# Dev Tools
just install-dev

# AI(Optional)
npm install -g @mindfoldhq/trellis@latest @colbymchenry/codegraph
trellis init -u <your-name>
codegraph install
codegraph init

# Build
just build

# Release Build
just release

# Check
just check

# Format
just fmt

# Test
just test

# Coverage
just coverage

# Update
just update
```
