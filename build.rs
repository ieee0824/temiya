fn main() {
    println!("cargo::rustc-check-cfg=cfg(temiya_address_sanitizer)");
    println!("cargo::rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    let encoded = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let flags: Vec<_> = encoded.split('\x1f').collect();
    if flags.contains(&"-Zsanitizer=address")
        || flags
            .windows(2)
            .any(|pair| pair == ["-Z", "sanitizer=address"])
    {
        println!("cargo::rustc-cfg=temiya_address_sanitizer");
    }
}
