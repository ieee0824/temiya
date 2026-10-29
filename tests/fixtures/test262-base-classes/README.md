# Fixed base class original selection

The selection was fixed before executing the implementation: all top-level
class declaration/expression files with empty or `class`-only feature metadata,
plus the entire class declaration `name-binding` directory. It contains 149
files and 294 variants at the recorded Test262 revision. It is a focused legacy
core selection, not a whole-class conformance suite. Source and harness hashes
are checked against immutable Git objects before execution, with unchanged
original bytes and the upstream license retained in the output.

At source `3bc78ae`, AST and Auto each record 256 pass and 38 fail, with no
skip/timeout/unreported. The 38 failures remain individually recorded with
variant, exact observed failure and diagnostic: 20 depend on derived classes
(#155), 16 on parameter/body environments (#165), and two on eval (#169).
Improvements can pass; a changed failure, newly failing known pass, timeout,
skip or missing result fails the integration check. A known dependency failure
is never counted as pass. Changing this policy requires a new update reason.

CI runs `scripts/test-base-class-originals.mjs` with the actual CLI on stable
and MSRV, keeping both mode captures, original inputs and diagnostics as
artifacts. Local invocation:

```sh
node scripts/test-base-class-originals.mjs target/release/temiya-test262 \
  path/to/test262 target/base-class-originals
```
