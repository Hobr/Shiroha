//! WASM Component Model bindings generated from WIT files.
//!
//! This module uses wasmtime's `bindgen!` macro to generate host-side bindings
//! for the state machine component interface defined in wit/state-machine.wit.

wasmtime::component::bindgen!({
    world: "state-machine",
    path: "../../wit",
});
