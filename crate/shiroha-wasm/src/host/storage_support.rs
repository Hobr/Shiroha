use crate::error::WasmError;

use super::ComponentStoreState;

pub(super) fn add_to_linker(
    linker: &mut wasmtime::component::Linker<ComponentStoreState>,
) -> Result<(), WasmError> {
    let mut inst = linker
        .instance("shiroha:flow/store@0.1.0")
        .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "get",
        |caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key): (String, String)| {
            let value = caller
                .data()
                .capability_store
                .get_value(&namespace, &key)
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok((value,))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "put",
        |caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key, value): (String, String, Vec<u8>)| {
            caller
                .data()
                .capability_store
                .put_value(&namespace, &key, &value)
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok(())
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "delete",
        |caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key): (String, String)| {
            let deleted = caller
                .data()
                .capability_store
                .delete_value(&namespace, &key)
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok((deleted,))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "list-keys",
        |caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, prefix, limit): (String, Option<String>, Option<u32>)| {
            let keys = caller
                .data()
                .capability_store
                .list_keys(&namespace, prefix.as_deref(), limit)
                .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
            Ok((keys,))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    Ok(())
}
