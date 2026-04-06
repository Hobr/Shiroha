use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const GENERATED_MACROS: &[(&str, &str)] = &[
    ("generate_flow", "flow"),
    ("generate_network_flow", "network-flow"),
    ("generate_storage_flow", "storage-flow"),
    ("generate_full_flow", "full-flow"),
];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let staged_wit_dir = out_dir.join("sdk-wit");
    let source_wit_dir = shiroha_wit::wit_dir();

    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("Cargo.toml").display()
    );
    for file in shiroha_wit::WIT_FILES {
        println!(
            "cargo:rerun-if-changed={}",
            source_wit_dir.join(file).display()
        );
    }

    stage_wit_files(&source_wit_dir, &staged_wit_dir);
    fs::write(
        out_dir.join("generated_wit_macros.rs"),
        render_generate_macros(&staged_wit_dir),
    )
    .expect("write generated wit macros");
}

fn stage_wit_files(source_dir: &Path, target_dir: &Path) {
    fs::create_dir_all(target_dir).expect("create staged wit dir");
    for file in shiroha_wit::WIT_FILES {
        fs::copy(source_dir.join(file), target_dir.join(file))
            .unwrap_or_else(|err| panic!("copy {file} into staged wit dir: {err}"));
    }
}

fn render_generate_macros(staged_wit_dir: &Path) -> String {
    let path_literal = format!("{:?}", staged_wit_dir.display().to_string());
    let mut generated = String::new();

    for (macro_name, world) in GENERATED_MACROS {
        generated.push_str(&format!(
            r#"
#[macro_export]
macro_rules! {macro_name} {{
    () => {{
        #[allow(unused_extern_crates)]
        extern crate shiroha_sdk as wit_bindgen;
        $crate::__wit_bindgen::generate!({{
            path: {path_literal},
            world: "{world}",
        }});
    }};
}}
"#
        ));
    }

    generated
}
