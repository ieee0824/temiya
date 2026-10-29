# Module fixture qualification

Issue #149 validates real module resolution/link/evaluation through the public
Engine and the actual Test262 CLI. This candidate rejects unpaired UTF-16
specifier aliases at UTF-8 host boundaries, keeps valid Unicode names working,
and validates named indirect exports before any module body executes.

A before-fix authored corpus imported `./\uD800.js` but loaded a real U+FFFD
filename and passed. Its retained corpus revision is null; it is not an original
Test262 test. The actual CLI regression now rejects both unpaired surrogate
ranges, retains U+FFFD and valid surrogate-pair names, and prevents the host error
from satisfying a language TypeError negative. Public Engine tests also ensure
the UTF-8 source callback never receives a lossy alias.

Linking now validates plain, renamed and circular missing re-exports, caches
SyntaxError failures and executes no module body on failure. Relative fixture
canonicalization, single evaluation of duplicate normalized imports and actual
cyclic live bindings are checked through the CLI.

Initial recorded checks:

- Full native suite: 699 passed / 26 ignored, 91 result groups. Formatting and
  all-target/all-feature Clippy with warnings denied passed.
- The new fixed selection executes nineteen original Module variants for each
  of AST and Auto, all successful. These cover cyclic graphs, missing exports,
  resolution SyntaxErrors and basic TLA. Module bodies use the AST module path;
  this does not qualify Bytecode module execution or full async evaluation.
- A broader 76-file original instantiation audit records 55 pass / 21 fail / zero
  skip. Missing class evaluation, the Function constructor required by the
  unchanged fnGlobalObject harness, and anonymous default-function initialization
  remain failures. No substitute harness or constructor is injected.
- The first eighteen-case audit before the link fix had fourteen pass/four fail
  for each backend. The two missing-indirect-export failures motivate the real
  linker fix; the TLA thenable/Promise gaps remain separately unqualified.
- Repeating those eighteen cases after the fix gives sixteen pass/two fail for
  each backend. Both missing-indirect-export cases now pass; both TLA failures
  remain recorded.
- The complete 251-file top-level-await subtree audit executes 253 variants
  through Auto: 171 pass, 82 fail, zero skip. Every requested path has a result;
  all eleven chunk processes completed. The unchanged pinned harness and a
  1000 ms async deadline are used. Promise, thenable assimilation, dynamic import
  and other missing language support remain failures, including incomplete
  async notifications. This is an audit, not full TLA qualification.

The fixed pin is d53ebc8b1931d9f1541e43d59650776e4b766248. Includes and fixtures
are read unchanged from Git objects. Gzip files preserve raw command/report bytes;
`evidence.json` records compressed and uncompressed SHA-256 values.

CI passed at source 67f428322c5249c0422ad4b7e6be4bcb1951dab2 and again at
d25d974b9502b9382c02ce58a0985f4c33f28910, with stable, Rust 1.85.0,
formatting/Clippy and C ABI checks successful. The 71 engine/worker/build input
hashes match across these revisions; the latter adds evidence and documentation.
Strict-provenance, symbolic-alignment Miri passed all five module graph GC tests
(including allocation-by-allocation collection) and both public API regressions.
Their terminal logs and CI output are preserved and checked by `verify.mjs`.

The new default Engine worker is qualified independently in
[`performance/`](performance/). Sixteen complete reports preserve four
comparisons per environment: candidate/main, candidate/actual Boa, main/actual
Boa, and identical candidate executables. Inputs, oracles, 15 samples, three
alternating pairs, limits and thresholds are unchanged. All reports pass the
independent 66-worker/1056-value integrity and statistical checks.

| Environment | Candidate vs actual Boa | Candidate vs main | Identical executable control |
| --- | --- | --- | --- |
| ARM64 | 26 faster / 7 noise | 31 noise / 1 slower / 1 faster | 30 noise / 1 slower / 2 faster |
| x64 copy 1 | 33 faster | 30 noise / 3 slower | 32 noise / 1 faster |
| x64 copy 2 | 33 faster | 33 noise | 32 noise / 1 faster |
| One planned x64 repeat | 33 faster | 31 noise / 2 slower | 33 noise |

All three x64 cohorts report AMD EPYC 7763. No CPU model filter was used.
The repeat preserves the initial ELF hashes and follows the documented second-run
policy. The initial copy 1 primitive-string-method slowdown reproduces in two
repeat pairs (ratios 1.111175 and 1.104713, threshold 0.10); the third is within
noise (1.080293). The initial three slower pairs, repeat two slower pairs and ARM
call pair 2 slowdown remain unchanged in the records. Their physical cause is
unknown; neither Boa superiority nor variability in another control proves a fix.
The fixed eleven workloads qualify candidate/Boa speed on the three recorded
x64 cohorts only. ARM and whole-engine superiority are not qualified.

Run `node benchmarks/test262-module-fixtures/performance/verify.mjs` to verify
all sixteen reports, compiler/features/locks/binary identities and source hashes,
along with the functional evidence. Performance follow-up is tracked separately;
these module resolution/link fixes do not claim to eliminate baseline regressions.
