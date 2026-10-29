# Security and undefined-behavior checks

Normal pull requests run formatting, Clippy, tests, and builds on stable and the
MSRV. The separate `Security and undefined behavior` workflow runs every Monday
at 04:23 UTC, on manual dispatch, and on pull requests that change this workflow
the root `Cargo.lock`, benchmark examples, C ABI files, or the shard runner. Its independent jobs are mandatory once
started: none use `continue-on-error`, and each has a hard timeout.

- `cargo-audit 0.22.2 --deny warnings` examines the committed root lockfile and
  fails for vulnerabilities, yanked crates, or unmaintained advisories.
- Miri runs library unit tests with strict provenance and symbolic alignment
  checks on pinned `nightly-2026-10-20`. The actual test listing is partitioned
  into 16 disjoint shards; every selected test must execute with zero ignored
  tests, and the aggregate requires every shard to succeed. Each shard retains
  its 60-minute timeout. Integration tests that spawn processes
  are intentionally outside this interpreter-only scope.
- AddressSanitizer runs integration tests on `x86_64-unknown-linux-gnu`, with
  immediate failure enabled, using the same pinned nightly. LeakSanitizer is
  explicitly disabled; managed-heap leak investigation is covered by GC
  diagnostics and retained-handle tests. ASan success does not establish a
  LeakSanitizer pass.

Dependabot checks Cargo and GitHub Actions weekly. Action revisions remain pinned
to full commits and updates are reviewed through ordinary CI. Failures require a
fix or an explicit reviewed policy change; the workflow has no silent allowlist.
The project currently contains no Rust `unsafe` block, but Miri and ASan remain
enabled so future unsafe or FFI boundaries enter continuous coverage immediately.

The actual `-Zsanitizer=address` compilation profile is detected by Cargo's
encoded compiler flags. Test262 workers in that profile retain the requested
RSS watchdog, stack/core limits and wall deadline, while omitting RLIMIT_AS:
[ASan maps more than 16 TB of sparse virtual shadow space](https://clang.llvm.org/docs/AddressSanitizer.html#limitations).
Normal builds retain both RLIMIT_AS and RSS limits. This is not selected by a
runtime environment variable. Every instrumented integration test and worker
still runs; resource tests check the appropriate kernel policy. JSON explicitly
records `address_sanitizer` and an absent `address_space_limit_bytes` in this
profile, so its successful runs do not claim a virtual-address cap. Sanitizer
results are not native performance measurements.

A manual dispatch with `asan_only=true` runs this focused check. Ordinary PR and
scheduled security runs retain dependency audit and all Miri shards as well.
