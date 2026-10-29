# Real Test262 runner qualification

Issues #145 and #146 add an executable Engine adapter and unchanged harness loading.
The adapter lives in the CLI client. Core metadata parsing stays unchanged; the
only new core operation is the borrowed ModuleHandle::record accessor. The
runner distinguishes parse, resolution and runtime failures, with a fresh
Engine/Realm per variant. It never accepts harness failures or unsupported host
hooks as matching language negatives.

The measured implementation is 01858c32c0123d980fa313dc24a08137efaf0abf;
the before-change reference is main 1d664b67bf7bbaf6aa517febca1267f81f6b944e.
Evidence-only commits must preserve the recorded source input hashes.

## Functional evidence

- Full native all-feature suite: 675 passed, 23 ignored, 90 suites. Ignored
  tests are not counted as passes. Formatting and all-target/all-feature Clippy
  pass. CI verifies stable, Rust 1.85, formatting/Clippy and C ABI.
- Fixed Test262 d53ebc8b1931d9f1541e43d59650776e4b766248: nine original
  files, 13 variants pass, including parse/runtime/resolution negatives and
  strict/noStrict/raw hashbang cases. The integration test poisons local harness
  files to prove pinned mode reads immutable Git objects.
- Every one of the 30 old audit files tracked by #146 is rerun with unchanged
  assert.js, sta.js and propertyHelper dependencies: AST 60/60 and Auto 60/60.
- The broader set of 63 formerly failing Array/String files yields 66 passed,
  60 failed, zero skipped variants. Those 60 failures remain in the raw report;
  they are other API gaps, not new passes or whole-feature completion.
- A selected module-link boundary test passes Miri with strict provenance,
  symbolic alignment and isolation. It checks no body side effects during link,
  repeat linking, link failure and rejection of another Engine's handle.
  This is one selected test, not a new full-library Miri qualification.
- Latest published Nayuta v0.3.0 / 2ab292848fb73c7e9d4d4570485880ae22f4e43f
  was built and its 14 tests passed. The final three-file scan reports 30
  heuristic complexity candidates, not 30 verified bugs; no new tool defect
  was established. The CLI's variant loop is bounded by two variants.

## Performance protocol and retained experiments

All reports use the original eleven workload bodies, setup, iteration counts
and numerical oracles. Each comparison has 15 timed samples, one checked warmup,
three alternating pairs, 66 processes and 1056 checked values. Limits stay
180 seconds/512 MiB. Decisions retain the original max(10%, three times the
sum of relative MADs) threshold. Instrumented times never qualify speed.

The actual modified Boa embedding is pinned to Omoikane
007556adccb8fb4329236932ca8175f8e8d119d2 with its original dependency features
and locks. The recorded newer main has identical engine/build/dependency
objects, as documented in validation/boa-reference-drift.json. This does not
claim all changes in that application have identical performance.

Each hosted cohort runs serial before/after, candidate/Boa, baseline/Boa and
same-executable controls, without choosing a CPU model. The local ARM64 protocol
pins timing workers to guest CPU0 and the agent's own threads to CPUs1-3; the
saved trap restores their original affinity. This is guest placement, not an
exclusive physical-core guarantee.

Both earlier implementation trials remain complete under experiments/:

| Trial | Cohort | Before/after decisions | Actual Boa decisions |
| --- | --- | --- | --- |
| sdk-560 | ARM64 | 33 within threshold | 33 Temiya faster |
| sdk-560 | AMD EPYC 9V45 | 32 within, 1 object-alloc slower | 33 Temiya faster |
| sdk-560 | AMD EPYC 7763 | 33 within | 33 Temiya faster |
| cli-8b | ARM64 | 33 within | 33 Temiya faster |
| cli-8b | Intel Xeon Platinum 8573C | 33 within | 33 Temiya faster |
| cli-8b | AMD EPYC 7763 | 31 within, 2 array slower | 33 Temiya faster |

The SDK allocation diagnostic retains both full Callgrind vectors as lossless
compressed files, symbols, assembly and process results. It uses the exact
native x64 SDK worker SHA (91bd8b...) and original object-alloc input. Simulated
whole-process instructions changed by 0.99294x, simulated indirect branch misses
by 1.08334x. These are model counters, not hardware events or a proven cause of
the native timing difference. The earlier CI enabled the empty default feature
and produced a different ELF; explicit --no-default-features restored the native
feature/hash match. Both logs are retained, not attributed to a different OS.

The final ARM64 before/after report has 32 within-threshold decisions and one
improvement. Its initial actual-Boa report has 32 faster decisions and one
inconclusive object-alloc pair (ratio 0.67678, noise threshold 0.46139). A single
predetermined replay of the entire unchanged original suite has 31 faster and
two inconclusive decisions (array pair2 and proto-method pair2). Both reports
retain false all-eleven speed gates; neither establishes all-eleven ARM64
superiority. Large MADs remain part of the observation, never weakened gates.

The final hosted run [40836094219](https://github.com/ieee0824/temiya/actions/runs/40836094219)
completed on both unfiltered cohorts:

| Final cohort | Before/after | Candidate/actual Boa | Baseline/Boa | Same executable |
| --- | --- | --- | --- | --- |
| Intel Xeon Platinum 8573C | 33 within threshold | 33 faster | 33 faster | 33 within threshold |
| AMD EPYC 7763 | 33 within threshold | 33 faster | 33 faster | 33 within threshold |

Both candidate ELF hashes are
6368d0ee00b8e5aac80fe3b7066cbfc8e18e70eb1025cd7f7aa84478ee237628,
matching the stable CI worker at the same source/profile/features. This supports
all-eleven superiority on these two hosted environments; it does not upgrade the
ARM64 inconclusive decisions or prove that no regression is possible.

Final native observations and their exact source/compiler/worker identities are
recorded in provenance.json and the raw reports. Verification recomputes every
median, MAD, oracle, process count and decision, checks cross-report identities,
and hashes the preserved evidence and uncompressed diagnostic profiles:

```sh
node benchmarks/test262-runner/verify.mjs
```

Performance claims apply to these recorded workloads and environments, not every
JavaScript program, async/RegExp suites or a JIT comparison. Host hooks (#147),
asynchronous completion, resource supervision and whole-corpus baseline remain
separate issues.
