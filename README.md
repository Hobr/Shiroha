# Shiroha

> A *WebAssembly*-extensible workflow orchestration engine built around *Finite-State Machines*.

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
