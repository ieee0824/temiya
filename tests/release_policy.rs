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
    assert!(
        WORKFLOW
            .contains("actions/attest-build-provenance@977bb373ede98d70efdf65b84cb5f73e068dcc2a")
    );
    assert!(WORKFLOW.contains("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02"));
    for revision in WORKFLOW
        .lines()
        .filter_map(|line| line.trim().strip_prefix("uses: "))
        .filter_map(|action| action.split('@').nth(1))
        .map(|revision| revision.split_whitespace().next().unwrap())
    {
        assert_eq!(revision.len(), 40);
        assert!(revision.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
