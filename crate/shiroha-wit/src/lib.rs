use std::path::{Path, PathBuf};

pub const WIT_FILES: &[&str] = &[
    "flow.wit",
    "net.wit",
    "store.wit",
    "network-flow.wit",
    "storage-flow.wit",
    "full-flow.wit",
];

pub fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub fn wit_dir() -> PathBuf {
    manifest_dir().join("wit")
}

pub fn wit_file(name: &str) -> PathBuf {
    wit_dir().join(name)
}
