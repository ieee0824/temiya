# Test262 Realm and ArrayBuffer host hooks

Issue #147 connects real child Realms, Script evaluation, ArrayBuffer detachment
and Engine collection through `$262`. Each execution owns rooted Realm handles;
native callbacks retain only a weak owner. The original Script completions and
throw identities are preserved. The ordinary-buffer `byteLength` getter exposes
zero after actual detachment. Global `$262` is writable/configurable and
non-enumerable in both parent and child Realms.

The implementation is 31862e8ab4b23607834031ccc8d0aebfaee66ce2. Native timings
measure 3163c4a33fc7748d36135102572b01eacf69e84a against merged main
93c516b40ae512a62f757a6294ff7225204556e2. The later change fixes only CLI host
binding attributes and their regression test. All library Rust sources and the
benchmark worker remain unchanged; fresh ARM64 builds and both x64 CI builds
produce byte-identical measured Engine executables. Functional results and CI
are recorded at the implementation revision, not relabelled old CLI results.

## Functional scope

- Full all-feature native suite: 681 passed, 24 ignored, 90 suites. Formatting,
  all-target/all-feature Clippy, stable, Rust 1.85 and C ABI checks pass.
- The added fixed selection uses 17 original Test262 files, including both
  variants of the Array length cross-Realm case tracked by #147. AST and Auto
  each pass 34 variants. With the two previous selections, 201 original CLI
  executions pass. The original assert.js, sta.js and includes are unmodified.
- Selected strict-provenance/symbolic-alignment Miri with isolation passes two
  tests: weak host-owner lifetime/GC and detachment invalidating existing real
  runtime TypedArray/DataView reads and writes. This is not a new full-library
  Miri run.
- Broader retained audits remain partial: prior Array/String selection 66 pass /
  60 fail; byteLength directory 16 pass / 4 fail (missing Int8Array and
  SharedArrayBuffer globals); global Script declarations 16 pass / 9 fail
  (class/direct-eval and other API gaps). None of these failures is a pass.
- An authored unpaired-UTF-16 source fixture fails with EngineError and is
  tracked in [#527](https://github.com/ieee0824/temiya/issues/527). Its corpus
  revision is null; it is not labelled an original Test262 case. Source is never
  silently replaced or accepted as a matching language SyntaxError.
- The support matrix retains 365 capabilities with 1512 authored observations,
  38 feature groups and eight components. The getter/host component remain
  partial; whole clauses, TypedArray/SharedArrayBuffer/DataView APIs, async
  completion and process supervision remain separately tracked.
- Latest published Nayuta v0.3.0 / 2ab292848fb73c7e9d4d4570485880ae22f4e43f
  reports 28 heuristic candidates across the three scanned Rust files. These
  are review candidates, not verified defects. No new Nayuta defect was found.

## Native observations

All reports keep the original eleven bodies, setup, iteration counts and numeric
oracles. Each uses 15 samples, one checked warmup, three alternating pairs,
66 processes and 1056 validated values. Limits remain 180 seconds / 512 MiB and
the original max(10%, three times the sum of relative MADs) decision threshold.

The actual modified Boa embedding is pinned to Omoikane
007556adccb8fb4329236932ca8175f8e8d119d2, preserving its original dependency
features/locks. Both native machines were accepted without a CPU-model filter.

| Cohort | Candidate/actual Boa | Before/after | Identical-executable control |
| --- | --- | --- | --- |
| Intel Xeon 6973P-C | 33 faster | 32 within, 1 call pair0 slower | 33 within |
| AMD EPYC 9V74 | 33 faster | 33 within | 31 within, 2 slower |
| ARM64 initial | 31 faster, 2 inconclusive | 27 within, 5 slower, 1 faster | see replay control |
| ARM64 one planned replay | no new Boa run | 30 within, 3 slower | 32 within, 1 faster |

Both hosted baseline/Boa controls also have 33 faster decisions. Both x64
candidate hashes are
2fe702444d23dd3a039b0079b1f26fc91c894ce53d12483dff3baa70dcf4cf90,
matching the measured and implementation CI workers. The ARM64 candidate hash
is 5b4b309e60652e2338b6611c6a0f78f407a38718839649a832f1908758ca5ca9.

These observations qualify all-eleven superiority over actual Boa on the two
recorded x64 cohorts. They do not qualify all-eleven ARM64 superiority or prove
the absence of a baseline regression. The initial ARM slower cases did not
repeat in the one planned replay; different cases crossed the threshold there.
The Intel call pair0 ratio is 1.15510, while its other two call pairs are 0.99848
and 0.99704. Identical binaries also cross thresholds: AMD prop-mega pair1
1.16270 and object-alloc pair2 1.10564; ARM call pair0 0.87725. All original
outcomes, samples and false all-eleven flags remain unchanged. A threshold
crossing by itself does not establish which code change caused a difference.

## Matched instruction diagnosis

[Diagnostic run 40843760137](https://github.com/ieee0824/temiya/actions/runs/40843760137)
uses the exact native x64 candidate/baseline ELF hashes with original full-size
prop-mono, call, array and primitive-string-property inputs. Eight successful
instrumented processes each validate one warmup and one pass. The complete raw
Callgrind vectors, source/worker identities, annotations and disassembly are
preserved losslessly, without committing duplicate executable binaries.

Whole-process instruction ratios, including startup/setup, are respectively
1.000001794, 1.000002093, 1.000002310 and 1.000005234. This does not support a large
increase in executed instruction count. The model cache/branch counters also
change: array simulated D1 read/write misses are 1.11209x/1.22302x; other counters
can improve sharply with code layout. All counters are preserved, rather than
selecting only favorable ones. These simulated events are not actual hardware
counters, and instrumented times never qualify native speed. This diagnosis does
not prove the cause or absence of a native timing regression.

## Reverification

```sh
node benchmarks/test262-host-hooks/verify.mjs
```

The verifier hashes the compressed and original bytes, checks immutable Git
inputs and library-source equivalence, recomputes every original oracle,
median/MAD and native decision, cross-checks all hosted controls/compiler/build
identities, compares CI worker hashes and checks the full raw diagnostic counter
vectors. Native records and verbose logs use lossless gzip to keep code review
readable. The verifier unpacks its native inputs into a temporary directory and
cleans up afterward. A successful data audit alone is not a universal speed pass.

The preceding runner trials remain in `../test262-runner/`, with their own source
pins and rejected/inconclusive results; they are not this implementation's speed
results. Performance claims remain limited to the recorded workloads and
conditions, not every JS program, async/RegExp workload, JIT mode or full corpus.
