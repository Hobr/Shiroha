#![allow(unsafe_code)]

wasmtime::component::bindgen!({
    path: "../../wit/shiroha-machine",
    world: "machine-component",
    exports: { default: async },
});
