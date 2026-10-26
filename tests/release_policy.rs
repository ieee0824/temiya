const CARGO: &str = include_str!("../Cargo.toml");
const RELEASE: &str = include_str!("../scripts/release");
const WORKFLOW: &str = include_str!("../.github/workflows/release.yml");
const DOCS: &str = include_str!("../docs/releasing.md");

#[test]
fn version_msrv_and_tag_contract_are_explicit() {
    assert!(CARGO.contains("version = \"0.1.0\""));
    assert!(CARGO.contains("rust-version = \"1.85\""));
    assert!(RELEASE.contains("tag $release_tag does not match Cargo version"));
    assert!(RELEASE.contains("tag $release_tag does not point at HEAD"));
    assert!(DOCS.contains("Semantic Versioning"));
}

#[test]
fn artifacts_are_locked_normalized_and_checksummed() {
    assert!(RELEASE.contains("cargo package --locked"));
    assert!(RELEASE.contains("cargo build --release --locked"));
    assert!(RELEASE.contains("SOURCE_DATE_EPOCH"));
    assert!(RELEASE.contains("target_directory=${CARGO_TARGET_DIR:-target}"));
    assert!(RELEASE.contains("--sort=name"));
    assert!(RELEASE.contains("--owner=0 --group=0 --numeric-owner"));
    assert!(RELEASE.contains("release/libtemiya.a"));
    assert!(RELEASE.contains("release/libtemiya.so"));
    assert!(RELEASE.contains("SPDXVersion: SPDX-2.3"));
    assert!(RELEASE.contains("sha256sum"));
}

#[test]
fn workflow_has_least_privilege_pinned_actions_and_attestation() {
    assert!(WORKFLOW.contains("tags:\n      - 'v*'"));
    assert!(WORKFLOW.contains("contents: read"));
    assert!(WORKFLOW.contains("id-token: write"));
    assert!(WORKFLOW.contains("attestations: write"));
    let actions = pinned_actions(WORKFLOW).expect("every Action must use a full commit SHA");
    for required in [
        "actions/checkout",
        "actions/attest-build-provenance",
        "actions/upload-artifact",
    ] {
        assert!(
            actions.contains(&required),
            "missing release Action: {required}"
        );
    }
}

// The release workflow uses plain `uses:` scalars. Reject malformed or moving
// references instead of silently skipping entries without an `@` separator.
fn pinned_actions(workflow: &str) -> Option<Vec<&str>> {
    let mut actions = Vec::new();
    for line in workflow.lines() {
        let line = line.trim();
        let line = line.strip_prefix("- ").unwrap_or(line);
        let Some(action) = line.strip_prefix("uses:") else {
            continue;
        };
        let (name, revision) = action.split_whitespace().next()?.split_once('@')?;
        if name.is_empty()
            || revision.len() != 40
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return None;
        }
        actions.push(name);
    }
    Some(actions)
}

#[test]
fn release_actions_reject_moving_missing_and_malformed_revisions() {
    for action in [
        "actions/upload-artifact",
        "actions/upload-artifact@v7",
        "actions/upload-artifact@main",
        "actions/upload-artifact@cf430e0",
        "actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cdg",
        "actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd99",
        "@cf430e030ddbb5b0abf93d22962f4752f3646cd9",
        "",
    ] {
        for prefix in ["uses: ", "- uses: "] {
            assert!(
                pinned_actions(&format!("{prefix}{action}")).is_none(),
                "{action}"
            );
        }
    }
    // A valid earlier action must not hide a later unpinned entry.
    assert!(pinned_actions(
        "uses: actions/checkout@1111111111111111111111111111111111111111\nuses: actions/upload-artifact@v7"
    ).is_none());
    assert_eq!(
        pinned_actions(
            "- uses: actions/upload-artifact@2222222222222222222222222222222222222222 # new reviewed pin"
        ),
        Some(vec!["actions/upload-artifact"])
    );
}
