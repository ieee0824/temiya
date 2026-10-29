use std::{fs, process::Command};

#[test]
fn jit_is_optional_and_absent_from_the_default_feature_graph() {
    // libc also supplies OS limits to the Test262 CLI. Its presence no longer
    // indicates JIT selection; inspect Cargo's actual resolved features.
    let features = |extra: &[&str]| {
        let output = Command::new(env!("CARGO"))
            .args([
                "tree",
                "--offline",
                "--locked",
                "--no-default-features",
                "--depth",
                "0",
                "--format",
                "{f}",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(features(&[]), "");
    assert_eq!(features(&["--features", "jit"]), "jit");

    let library = fs::read_to_string("src/lib.rs").unwrap();
    assert!(library.contains("#[cfg(feature = \"jit\")]\npub mod jit;"));
}
