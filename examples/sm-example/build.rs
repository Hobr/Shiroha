// Placeholder build script for sm-example
// Guest-side WIT bindings will be added after host-side bindgen is working
fn main() {
    println!("cargo:rerun-if-changed=../../wit");
}
