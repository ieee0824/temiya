# Checked component phase records

Frozen source6ec1c50 measures public parser/Engine compile/startup, actual JavaScript object properties, and standalone tracing-Heap graph lifecycles. [Full boundaries and reproduction](../../docs/component-benchmarks.md) explain the different APIs.

Hosted run [40620663377](https://github.com/ieee0824/temiya/actions/runs/40620663377) records one x64 Intel Xeon Platinum8573C and one ARM64 host; local ARM64 is retained separately. Each of the three reports has18 isolated workers,15 checked samples plus warmup, and three declared serial order rotations. All54 workers /864 pass-level oracles are correct. Each report retains every raw nanosecond sample, result, exit status, worker/build/source hash, compiler and machine; round medians/MAD and sampled process HWM remain separate per host. ARM CPU models are recorded as unknown. Unobserved process peaks remain null. No cross-machine absolute times or comparison ratios are used to claim a speedup.

Standalone Heap really retains rooted cycles, reclaims unrooted cycles, verifies weak handles/graph integrity and preserves young targets through old-to-young barriers. **Engine reclamation readiness remains false on all three hosts**: self-cycle, two-object-cycle and escaped-prototype remain alive after teardown. The exact public-Engine probe raw output and strict exit2 are retained. No standalone timing turns those Engine failures into a pass.

Artifacts x64 12787486219 / ARM64 12787027801; source-verification.json records production equality with main2f0f4c2. There is no engine optimization or change to the original-eleven, authored async/RegExp inputs or noise threshold in this work. Existing actual-Boa and SpiderMonkey comparisons are separate evidence; these component records add no cross-engine victory claim.

Run from the repository root:

```sh
node benchmarks/components/verify-integrity.mjs
node scripts/check-component-records.mjs
```

The inventory verifies both stored and decompressed bytes. The offline verifier independently recalculates medians/MAD, checks exact three-round coverage, raw results/build provenance and actual reclamation readiness. Seven corrupted complete records must be rejected. The native worker checks also retain12 checked results and42 rejected result mutations plus6 rejected commands per build; those diagnostic checks are not counted in the864 timed-pass oracles.
