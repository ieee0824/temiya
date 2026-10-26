# Release process

Temiya uses Semantic Versioning. Before 1.0, minor releases may change public
APIs; patch releases remain backward compatible. `Cargo.toml` is authoritative
for the version and MSRV. A release tag must be an exact `v<package-version>`
match and must point at the commit being packaged.

## Preparing a release

1. Move relevant entries from `Unreleased` in `CHANGELOG.md` to a dated version.
2. Update the package version and, when required, `rust-version` in `Cargo.toml`.
3. Run stable and MSRV CI, security CI, and `scripts/release check vX.Y.Z`.
4. Create an annotated `vX.Y.Z` tag on the reviewed main commit and push it.

The tag workflow repeats all publication checks before producing files. It
never publishes the crate automatically: maintainers first download and verify
the artifacts, then explicitly publish the exact `.crate` file.

## Reproducible output

`scripts/release package vX.Y.Z <directory>` derives `SOURCE_DATE_EPOCH` from
the tagged commit, builds with `--locked`, normalizes tar ownership, ordering,
and timestamps, and emits:

- the Cargo `.crate` archive;
- a Linux native archive containing the C header, shared library, and static
  library;
- an SPDX 2.3 SBOM generated from the locked Cargo package graph;
- `SHA256SUMS` covering every distributable artifact.

Run the package command twice from clean checkouts of the same tag and compare
`SHA256SUMS` to audit reproducibility. Rust compiler and runner images also
affect native code, so the workflow records tool versions and GitHub's build
provenance attestation. Release artifacts are signed through Sigstore-backed
GitHub artifact attestations using short-lived OIDC credentials; no long-lived
signing key is stored in the repository.

Verify downloaded files with:

```sh
sha256sum --check SHA256SUMS
gh attestation verify --repo ieee0824/temiya temiya-*
```

The MSRV is Rust 1.85.0. Raising it requires a minor version change before 1.0,
a changelog entry, and synchronized updates to `Cargo.toml` and CI.
