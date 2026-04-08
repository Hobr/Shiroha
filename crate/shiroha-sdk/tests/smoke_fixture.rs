use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex as StdMutex, OnceLock};

const CANONICAL_WIT_FILES: &[&str] = &[
    "flow.wit",
    "net.wit",
    "store.wit",
    "network-flow.wit",
    "storage-flow.wit",
    "full-flow.wit",
];

struct FixtureCase {
    manifest_path: &'static str,
    wasm_name: &'static str,
}

fn hash_file_if_present(path: &Path, hasher: &mut DefaultHasher) {
    path.hash(hasher);
    if let Ok(bytes) = fs::read(path) {
        bytes.hash(hasher);
    }
}

fn tracked_input_paths(root: &Path, fixture_path: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        root.join("Cargo.toml"),
        root.join("build.rs"),
        root.join("src/lib.rs"),
        root.join(fixture_path).join("Cargo.toml"),
        root.join(fixture_path).join("src/lib.rs"),
        root.join("../shiroha-wit/Cargo.toml"),
        root.join("../shiroha-wit/src/lib.rs"),
    ];

    paths.extend(
        CANONICAL_WIT_FILES
            .iter()
            .map(|file| root.join("../shiroha-wit/wit").join(file)),
    );
    paths
}

fn build_key(root: &Path, fixture_path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    for path in tracked_input_paths(root, fixture_path) {
        hash_file_if_present(&path, &mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn fixture_target_dir(root: &Path, fixture_path: &str) -> PathBuf {
    let fixture_key = fixture_path.replace('/', "-");
    std::env::temp_dir()
        .join("shiroha-sdk-fixtures")
        .join(fixture_key)
        .join(build_key(root, fixture_path))
}

fn build_fixture(case: &FixtureCase) -> Vec<u8> {
    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let _guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_manifest = root.join(case.manifest_path).join("Cargo.toml");
    let target_dir = fixture_target_dir(&root, case.manifest_path);
    let wasm_path = target_dir.join(format!("wasm32-wasip2/release/{}.wasm", case.wasm_name));

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

    fs::read(&wasm_path).expect("read built sdk smoke fixture")
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, contents).expect("write file");
}

fn assert_crate_has_no_vendored_wit_files(crate_dir: &Path, crate_name: &str) {
    let wit_dir = crate_dir.join("wit");
    let vendored_files = if wit_dir.exists() {
        fs::read_dir(&wit_dir)
            .expect("read crate wit dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .count()
    } else {
        0
    };

    assert!(
        vendored_files == 0,
        "{crate_name} should consume shared WIT instead of vendoring its own copy"
    );
}

fn stage_workspace(root: &Path, fixture_path: &str) {
    write_file(
        &root.join("Cargo.toml"),
        "[package]\nname = \"shiroha-sdk\"\nversion = \"0.2.0\"\n",
    );
    write_file(
        &root.join("build.rs"),
        "fn main() {\n    let _ = shiroha_wit::wit_dir();\n}\n",
    );
    write_file(&root.join("src/lib.rs"), "pub fn placeholder() {}\n");
    write_file(
        &root.join(fixture_path).join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.2.0\"\n",
    );
    write_file(
        &root.join(fixture_path).join("src/lib.rs"),
        "pub fn fixture_placeholder() {}\n",
    );
    write_file(
        &root.join("../shiroha-wit/Cargo.toml"),
        "[package]\nname = \"shiroha-wit\"\nversion = \"0.2.0\"\n",
    );
    write_file(
        &root.join("../shiroha-wit/src/lib.rs"),
        "pub fn wit_dir() -> &'static str { \"wit\" }\n",
    );

    for file in CANONICAL_WIT_FILES {
        write_file(
            &root.join("../shiroha-wit/wit").join(file),
            &format!("package shiroha:flow@0.2.0;\n// {file}\n"),
        );
    }
}

#[test]
#[ignore = "publishing smoke check; keep out of the default edit-compile-test loop"]
fn cargo_package_includes_shiroha_wit_sources() {
    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let _guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("cargo")
        .arg("package")
        .arg("-p")
        .arg("shiroha-wit")
        .arg("--allow-dirty")
        .arg("--list")
        .current_dir(root.join("../.."))
        .output()
        .expect("run cargo package --list");

    assert!(
        output.status.success(),
        "cargo package --list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let listed = String::from_utf8(output.stdout).expect("package list should be utf-8");
    for file in CANONICAL_WIT_FILES {
        assert!(
            listed.contains(&format!("wit/{file}")),
            "cargo package output should include shiroha-wit wit/{file}, got:\n{listed}"
        );
    }
}

#[test]
fn sdk_no_longer_vendors_wit_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_crate_has_no_vendored_wit_files(&root, "shiroha-sdk");
}

#[test]
fn wasm_runtime_no_longer_vendors_wit_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_crate_has_no_vendored_wit_files(&root.join("../shiroha-wasm"), "shiroha-wasm");
}

#[test]
fn sdk_build_script_uses_shiroha_wit_dependency() {
    let source = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("read sdk build script");

    assert!(
        source.contains("shiroha_wit::wit_dir"),
        "sdk build script should resolve WIT through the shiroha-wit build dependency"
    );
}

#[test]
fn sdk_build_script_declares_all_world_macros() {
    let source = fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build.rs"))
        .expect("read sdk build script");

    for (macro_name, world) in [
        ("generate_flow", "flow"),
        ("generate_network_flow", "network-flow"),
        ("generate_storage_flow", "storage-flow"),
        ("generate_full_flow", "full-flow"),
    ] {
        assert!(
            source.contains(macro_name),
            "sdk build script should declare macro `{macro_name}`"
        );
        assert!(
            source.contains(world),
            "sdk build script should generate bindings for world `{world}`"
        );
    }
}

#[test]
fn build_key_changes_when_sdk_manifest_changes() {
    let temp_root = std::env::temp_dir()
        .join("shiroha-sdk-build-key-tests")
        .join("manifest-change")
        .join("shiroha-sdk");
    let fixture_path = "test-fixtures/sdk-smoke";
    stage_workspace(&temp_root, fixture_path);

    let before = build_key(&temp_root, fixture_path);
    write_file(
        &temp_root.join("Cargo.toml"),
        "[package]\nname = \"shiroha-sdk\"\nversion = \"0.2.0\"\n",
    );
    let after = build_key(&temp_root, fixture_path);

    assert_ne!(before, after, "build key should include sdk Cargo.toml");
}

#[test]
fn build_key_changes_when_shiroha_wit_changes() {
    let temp_root = std::env::temp_dir()
        .join("shiroha-sdk-build-key-tests")
        .join("wit-change")
        .join("shiroha-sdk");
    let fixture_path = "test-fixtures/sdk-smoke";
    stage_workspace(&temp_root, fixture_path);

    let before = build_key(&temp_root, fixture_path);
    write_file(
        &temp_root.join("../shiroha-wit/wit/flow.wit"),
        "package shiroha:flow@0.2.0;\n// shiroha-wit flow changed\n",
    );
    let after = build_key(&temp_root, fixture_path);

    assert_ne!(
        before, after,
        "build key should include shared shiroha-wit files"
    );
}

#[test]
fn build_key_changes_when_shiroha_wit_manifest_changes() {
    let temp_root = std::env::temp_dir()
        .join("shiroha-sdk-build-key-tests")
        .join("shiroha-wit-manifest-change")
        .join("shiroha-sdk");
    let fixture_path = "test-fixtures/sdk-smoke";
    stage_workspace(&temp_root, fixture_path);

    let before = build_key(&temp_root, fixture_path);
    write_file(
        &temp_root.join("../shiroha-wit/Cargo.toml"),
        "[package]\nname = \"shiroha-wit\"\nversion = \"0.2.0\"\n",
    );
    let after = build_key(&temp_root, fixture_path);

    assert_ne!(
        before, after,
        "build key should include shiroha-wit Cargo.toml"
    );
}

#[test]
#[ignore = "heavy wasm32 compile smoke; run explicitly when validating sdk fixture builds"]
fn builds_flow_sdk_smoke_fixture_for_wasm32_wasip2() {
    let wasm_bytes = build_fixture(&FixtureCase {
        manifest_path: "test-fixtures/flow-smoke",
        wasm_name: "sdk_flow_smoke_component",
    });
    assert!(
        !wasm_bytes.is_empty(),
        "sdk flow smoke fixture wasm should not be empty"
    );
}

#[test]
#[ignore = "heavy wasm32 compile smoke; run explicitly when validating sdk fixture builds"]
fn builds_full_sdk_smoke_fixture_for_wasm32_wasip2() {
    let wasm_bytes = build_fixture(&FixtureCase {
        manifest_path: "test-fixtures/sdk-smoke",
        wasm_name: "sdk_smoke_component",
    });
    assert!(
        !wasm_bytes.is_empty(),
        "sdk full smoke fixture wasm should not be empty"
    );
}
