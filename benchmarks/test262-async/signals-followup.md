# Lazy rejection/checkpoint storage candidate

The initial abbeac6 candidate has reproducible baseline regressions on both
recorded x64 machines. The preserved original timings and twelve instrumented
runs remain associated with that source and its exact ELFs.

This follow-up keeps ordinary job and active-job storage in the queue record,
while rejection checkpoints and the host rejection hook share a lazily allocated
sidecar. Realms that use neither do not allocate the sidecar. All checkpoint jobs
remain traced, including incoming payloads before their allocation safepoint,
and regular jobs still run before rejection checkpoints. Hook replacement and
callback invocation release queue borrows before user code or destruction.

This reduces always-present queue storage. Its effect on the observed native
regressions remains unqualified until new native reports are complete. No timing
input, iteration count, oracle, eligibility gate or noise threshold is changed.

Checks on this candidate:

- All-feature native suite: 695 passed, 25 ignored, 91 result groups.
- Formatting and all-target/all-feature Clippy with warnings denied: passed.
- Original pinned CLI selections: 209 executions passed, including eight async
  executions; original harness bytes remain checked against the fixed pin.
- Strict-provenance/symbolic-alignment Miri with isolation: 21 distinct queue,
  Promise and shared-Agent tests passed. The unpublished-payload regression now
  forces collection for both ordinary and checkpoint-only incoming queues.
- Nayuta v0.3.2 scans the same four Rust files successfully: 13 heuristic
  candidates, not confirmed bugs. No additional Nayuta defect was found.

The sidecar code and its validation logs accompany this document. MSRV, C ABI
and the exact committed-source CI checks will run before merge. New worker hashes
and native comparisons will be recorded separately from the initial candidate.
