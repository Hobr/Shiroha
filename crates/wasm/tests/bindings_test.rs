//! Test to verify bindgen generates correct types.

use shiroha_wasm::bindings::*;

#[test]
fn test_bindings_exist() {
    // This test will fail to compile if bindgen doesn't generate the expected types
    // Just verify the types exist by using them in unused code

    let _: Option<shiroha::sm::types::ActionKind> = None;
    let _: Option<shiroha::sm::types::ActionRef> = None;
    let _: Option<shiroha::sm::types::HistoryKind> = None;
    let _: Option<shiroha::sm::types::State> = None;
    let _: Option<shiroha::sm::types::Transition> = None;
    let _: Option<shiroha::sm::types::EventDef> = None;
}
