use std::fs;

#[test]
fn jit_is_optional_and_default_build_has_no_jit_dependency() {
    let manifest = fs::read_to_string("Cargo.toml").unwrap();
    assert!(manifest.contains("default = []"));
    assert!(manifest.contains("jit = [\"dep:libc\"]"));
    assert!(manifest.contains("libc = { version = \"0.2.177\", optional = true }"));

    let library = fs::read_to_string("src/lib.rs").unwrap();
    assert!(library.contains("#[cfg(feature = \"jit\")]\npub mod jit;"));
}
