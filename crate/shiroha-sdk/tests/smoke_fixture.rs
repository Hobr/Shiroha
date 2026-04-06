use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex as StdMutex, OnceLock};

fn hash_file_if_present(path: &Path, hasher: &mut DefaultHasher) {
    path.hash(hasher);
    if let Ok(bytes) = fs::read(path) {
        bytes.hash(hasher);
    }
}

fn build_key(root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    hash_file_if_present(&root.join("src/lib.rs"), &mut hasher);
    hash_file_if_present(
        &root.join("test-fixtures/sdk-smoke/Cargo.toml"),
        &mut hasher,
    );
    hash_file_if_present(
        &root.join("test-fixtures/sdk-smoke/src/lib.rs"),
        &mut hasher,
    );
    format!("{:016x}", hasher.finish())
}

#[test]
fn builds_sdk_smoke_fixture_for_wasm32_wasip2() {
    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let _guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_manifest = root.join("test-fixtures/sdk-smoke/Cargo.toml");
    let target_dir = std::env::temp_dir()
        .join("shiroha-sdk-fixtures")
        .join(build_key(&root));
    let wasm_path = target_dir.join("wasm32-wasip2/release/sdk_smoke_component.wasm");

    if !wasm_path.exists() {
        let status = Command::new("cargo")
            .arg("build")
            .arg("--offline")
            .arg("--manifest-path")
            .arg(&fixture_manifest)
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--release")
            .env("CARGO_TARGET_DIR", &target_dir)
            .current_dir(&root)
            .status()
            .expect("run cargo build for sdk smoke fixture");

        assert!(status.success(), "sdk smoke fixture build failed");
    }

    let wasm_bytes = fs::read(&wasm_path).expect("read built sdk smoke fixture");
    assert!(
        !wasm_bytes.is_empty(),
        "sdk smoke fixture wasm should not be empty"
    );
}
