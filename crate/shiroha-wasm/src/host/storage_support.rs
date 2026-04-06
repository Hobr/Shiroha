use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::error::WasmError;

use super::ComponentStoreState;

type StoreMap = BTreeMap<(String, String), Vec<u8>>;

fn shared_store() -> &'static Mutex<StoreMap> {
    static STORE: OnceLock<Mutex<StoreMap>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn add_to_linker(
    linker: &mut wasmtime::component::Linker<ComponentStoreState>,
) -> Result<(), WasmError> {
    let mut inst = linker
        .instance("shiroha:flow/store@0.1.0")
        .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "get",
        |_caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key): (String, String)| {
            let store = shared_store()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok((store.get(&(namespace, key)).cloned(),))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "put",
        |_caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key, value): (String, String, Vec<u8>)| {
            shared_store()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .insert((namespace, key), value);
            Ok(())
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "delete",
        |_caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, key): (String, String)| {
            let deleted = shared_store()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&(namespace, key))
                .is_some();
            Ok((deleted,))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    inst.func_wrap(
        "list-keys",
        |_caller: wasmtime::StoreContextMut<'_, ComponentStoreState>,
         (namespace, prefix, limit): (String, Option<String>, Option<u32>)| {
            let store = shared_store()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut keys = store
                .keys()
                .filter(|(entry_namespace, key)| {
                    entry_namespace == &namespace
                        && prefix.as_ref().is_none_or(|prefix| key.starts_with(prefix))
                })
                .map(|(_, key)| key.clone())
                .collect::<Vec<_>>();
            if let Some(limit) = limit {
                keys.truncate(limit as usize);
            }
            Ok((keys,))
        },
    )
    .map_err(|e| WasmError::Instantiation(e.to_string()))?;
    Ok(())
}
