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
