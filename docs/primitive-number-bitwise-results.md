# Primitive Number bitwise candidate measurements

The experimental x64 candidate passes the original-eleven actual-Boa gate:
**33 faster / 0 within noise / 0 slower**. This is evidence for the exact
experimental source, mode and hosted machine below. Production defaults and the
new default-API integration candidate are separate; this record does not adopt
runtime changes or establish application-wide superiority.

| Machine | Before/after | Actual omoikane Boa |
| --- | --- | --- |
| hosted x64 | 33 / 0 / 0 | 33 / 0 / 0 |
| ARM64 | 32 / 1 / 0 | 17 / 16 / 0 |

Counts use the unchanged threshold max(0.10, 3 * (MADa/mediana + MADb/medianb)).
Within-noise outcomes are not wins, so ARM64 does not pass either all-eleven
faster gate. All source/baseline identities and unsuccessful earlier candidates
remain preserved separately; the earlier ARM64 String-method slow observation
is not erased or causally explained by this measurement.

The measured source is 836996c86e929fe2c7dc5714ed59db3973da102f, explicitly configured
as Auto Script+loop. Both AST baselines are a23e82b8cd8230e4924747e5f09a55007024e550,
including the merged Number fast path. This is a cumulative VM implementation;
the before/after comparison does not isolate bitwise operations from earlier
optimizations. The changed primitive-Number path directly computes bitwise and
shift results, using exact bounded truncation/low-bit conversion and the old
conversion for larger/nonfinite inputs. Generic ToUint32 remains unchanged;
the separate held #485 experiment is not adopted.

An independent BigInt oracle derives ToUint32 from IEEE-754 bits, with 4,138
boundary/random inputs across eleven operators in AST and VM (91,036 results).
Additional numeric edge cases and conversion/reentry tests pass. Source
[CI](https://github.com/ieee0824/temiya/actions/runs/40512510354) and
[security](https://github.com/ieee0824/temiya/actions/runs/40513326198) passed.
These correctness checks alone are not performance evidence.

The hosted measurement is [run 40513332008](https://github.com/ieee0824/temiya/actions/runs/40513332008).
All four reports use the unchanged original eleven workload bodies, iteration
counts and oracles from omoikane 007556adccb8fb4329236932ca8175f8e8d119d2,
fifteen timed samples plus one checked warmup, three alternating serial pairs,
and common 512 MiB / 180-second guards. Actual Boa retains its application
embedding, lockfile and features. Invocation parsing/evaluation/Script destruction
are timed; startup/setup/warmup are excluded from samples. The ARM64 comparison
runs Boa first and before/after second, as recorded in its notes.

All 264 workers and 4,224 results passed validation. All medians/MADs and 132 pair
decisions were independently recomputed. Original manifest/oracles, full unique
coverage, limits, modes, compiler/build identities, exact hosted head/artifact
and local executable hashes were checked. [Provenance](../benchmarks/number-bitwise-provenance.json)
records these details and each raw report's SHA-256. Raw filenames use
number-bitwise-hosted or number-bitwise-arm64 plus -beforeafter.json or -boa.json.

The subsequent integration removes the unadopted AST-function loop tier and
changes Engine's default selection, so it requires its own exact-source CI,
security and default-worker timing. #427 and broader async/RegExp/GC/SpiderMonkey
suite work remain open; this x64 result does not complete the overall goal.
