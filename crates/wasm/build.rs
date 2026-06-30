fn main() {
    println!("cargo:rerun-if-changed=../../wit");

    // No build script binding generation - we'll use inline bindgen! in the source
}
