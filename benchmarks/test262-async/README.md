# Test262 async completion qualification (in progress)

This record accompanies #148. Implementation and conformance checks are available;
performance qualification and final PR CI are still pending. This is not a claim
that the change is ready to merge or faster than Boa.

The actual CLI loads unchanged Test262 harness files, monitors print notifications,
drives the shared Agent job queue and supervises each async variant in a child
process with a finite wall-clock deadline. See [runner documentation](../../docs/test262.md).
The normal regression suite uses authored cases with an unchanged upstream harness;
these are not relabeled as original upstream test cases.

Recorded checks:

- All-feature native suite: 695 passed, 25 ignored, 91 test/doc-test result groups.
- Formatting and all-target/all-feature Clippy with warnings denied: passed.
- Rust 1.85 all-target/all-feature check: passed (not a full MSRV test execution).
- Pinned original CLI selections including the new async test: 209 successful
  executions. The new selection contributes eight original executions, four
  variants each for AST and Auto. Harness byte identity is checked against the pin.
- Wider four-file async-function audit: each backend has four passes, four
  failures, zero skips. The after-await cases require missing global Promise APIs
  (#179); their failures are preserved rather than bypassed with a harness stub.
- Miri with strict provenance, symbolic alignment and isolation: six public
  Promise queue tests, three shared-Agent embedding tests, five queue library
  tests and seven Promise library tests passed (21 distinct tests). This is not
  a rerun of the entire Miri suite.

Initial failing reproductions are preserved: a handler attached by another job
was incorrectly reported as unhandled, and discarded Promise facades lost the
rejection state across GC. Checkpoint scheduling and traced retained state fix
both. Current regression checks exercise the real queue, collection and CLI.

The gzip files contain lossless command output, reports and pre-fix reproductions.
`evidence.json` gives the compressed and uncompressed SHA-256 digests, the parent
commit and immutable Test262 corpus revision. No native speed conclusion is
available for this implementation yet; previous host-hook benchmarks qualify
their own source revisions only.
