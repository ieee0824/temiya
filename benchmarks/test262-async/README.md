# Test262 async completion: initial qualification and open regressions

This record accompanies #148. Implementation and conformance checks are available;
initial CI is green at abbeac6b36767c0df6565bcce7e8b066bdaa0c06. Hosted
measurements are complete and retain baseline regressions. This initial candidate
is not ready to merge: its reproducible x64 slowdowns require investigation and
correction, even though both recorded x64 candidate/Boa cohorts pass.

The actual CLI loads unchanged Test262 harness files, monitors print notifications,
drives the shared Agent job queue and supervises each async variant in a child
process with a finite wall-clock deadline. See [runner documentation](../../docs/test262.md).
The normal regression suite uses authored cases with an unchanged upstream harness;
these are not relabeled as original upstream test cases.

Recorded checks:

- All-feature native suite: 695 passed, 25 ignored, 91 test/doc-test result groups.
- Formatting and all-target/all-feature Clippy with warnings denied: passed.
- Rust 1.85 all-target/all-feature local check: passed. The separate initial CI
  run 40858410171 passed all four jobs, including full stable and MSRV test/build
  paths, original corpus regressions, formatting/Clippy and C ABI.
- Local C ABI header, C example and exported-symbol check: passed.
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

## ARM64 native observations

The changed worker measures abbeac6b36767c0df6565bcce7e8b066bdaa0c06 against
main 858afcce22c9ec50bb94f7630fad0f6f298ce821. Both were freshly built with the
same Rust 1.99.1 compiler, release profile and no optional features. The original
eleven bodies, setup, iteration counts and numeric oracles are unchanged: each
report retains 15 samples, one checked warmup, three alternating pairs, 66 worker
processes and 1056 validated values. The 180-second / 512-MiB limits and
max(10%, three times the sum of relative MADs) gate are unchanged.

| ARM64 comparison | Decisions |
| --- | --- |
| Candidate / actual modified Boa | 31 faster, 2 inconclusive |
| Baseline / actual modified Boa | 33 faster |
| Candidate / baseline, initial | 32 within noise, 1 slower |
| Candidate / identical executable | 33 within noise |
| Candidate / baseline, one planned replay | 28 within noise, 5 slower |

The initial proto-method pair 1 regression (2.00024 ratio) did not reproduce:
all three proto-method pairs are within the gate on the replay. The replay has
other slower comparisons: prop-mono pair 0, call pair 0, array pairs 0/1 and
primitive-string-method pair 1. Both complete reports are retained; the replay
does not replace the initial result. There is no absence-of-regression claim.
ARM64 also does not qualify all 33 candidate/Boa comparisons.

## Hosted native observations

Both independent, unfiltered hosted machines report AMD EPYC 7763. The worker
inputs, compiler, original Boa features/locks and statistical decisions have
been independently checked across all eight reports.

| Cohort | Candidate / actual Boa | Baseline / actual Boa | Candidate / baseline | Identical-executable control |
| --- | --- | --- | --- | --- |
| Copy 1, AMD EPYC 7763 | 33 faster | 33 faster | 14 within, 19 slower | 33 within |
| Copy 2, AMD EPYC 7763 | 33 faster | 33 faster | 22 within, 11 slower | 33 within |

String concatenation is approximately 1.37–1.44 times the baseline across all
three pairs on both machines; primitive string-property access is approximately
1.28–1.31 times baseline. Property megamorphism also regresses in all three
pairs on both machines. These repeatable losses, alongside clean same-binary
controls, cannot be dismissed by pointing to the successful Boa comparisons.
An additional full-input diagnostic of string-concat, primitive-string-property,
prop-mega and array is in progress. No extra timing replay replaces these
observations.

## Matched instruction diagnostics

Four x64 Callgrind runs use the full original proto-method and call inputs, with
one checked warmup and one checked pass. Whole-process instruction ratios
(candidate/baseline) are 0.999995715 and 0.999997124. Conditional-branch miss model
ratios are 1.496275 and 1.992703; every event counter and raw vector is retained.
These models include startup/setup and are not hardware events or native speed.
They do not establish the cause of the ARM64 timing differences. The candidate
ELF hash matches the initial stable CI worker and both hosted native cohorts.

## Nayuta

Latest used v0.3.2 / 0dd74932cc2623b4a715213c96dec1077518f049 passes
17 tests and scans the four selected changed Rust files without parse errors
(11 heuristic candidates, not 11 confirmed defects). Both the const declaration
reproducer from Nayuta #20 and the array-length/const-generic reproducers from
[Nayuta #22](https://github.com/ieee0824/nayuta/issues/22) are fixed in the real
CLI. Earlier v0.3.1 failing examples and the v0.3.2 corrected output are both
preserved. No additional Nayuta defect was confirmed in this update.

The gzip files contain lossless command output, reports and pre-fix reproductions.
`evidence.json` gives the compressed and uncompressed SHA-256 digests, the parent
commit and immutable Test262 corpus revision. `provenance.json` pins the initial
candidate and parent sources, every retained file, all thirteen native reports
and all observed outcomes. Run `node benchmarks/test262-async/verify.mjs` to
recompute raw-sample decisions, original execution totals and source/compiler/
binary identity checks. Successful verification preserves the regressions; it
does not mean they have been fixed or that all JavaScript is faster. Previous
host-hook benchmarks qualify their own source revisions only.
