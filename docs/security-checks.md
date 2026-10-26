# Security and undefined-behavior checks

Normal pull requests run formatting, Clippy, tests, and builds on stable and the
MSRV. The separate `Security and undefined behavior` workflow runs every Monday
at 04:23 UTC and on manual dispatch. Its independent jobs are mandatory once
started: none use `continue-on-error`, and each has a hard timeout.

- `cargo-audit 0.22.2 --deny warnings` examines the committed root lockfile and
  fails for vulnerabilities, yanked crates, or unmaintained advisories.
- Miri runs library unit tests with strict provenance and symbolic alignment
  checks on pinned `nightly-2026-10-20`. Integration tests that spawn processes
  are intentionally outside this interpreter-only scope.
- AddressSanitizer runs integration tests on `x86_64-unknown-linux-gnu`, with
  immediate failure enabled, using the same pinned nightly. LeakSanitizer is
  explicitly disabled because Realm intrinsic graphs currently use intentional
  `Rc` cycles; managed-heap leak investigation is covered by GC diagnostics.

Dependabot checks Cargo and GitHub Actions weekly. Action revisions remain pinned
to full commits and updates are reviewed through ordinary CI. Failures require a
fix or an explicit reviewed policy change; the workflow has no silent allowlist.
The project currently contains no Rust `unsafe` block, but Miri and ASan remain
enabled so future unsafe or FFI boundaries enter continuous coverage immediately.
