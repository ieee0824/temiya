Complete security capture for integrated source `010ac60ddeb68ce702e1f3b2a4cfdd87345f2568`.

All 19 jobs passed. The verifier checks every retained log hash, all 16 exact-test plans and executions, their complete disjoint partition, strict Miri flags, ASan test summaries and dependency-audit command. This validates this recorded source only and does not qualify native Boa speed. Later runtime changes need their own safety run.

Replay from the repository root:

```sh
node benchmarks/object-binding/security-current-integrated-v1/verify.mjs benchmarks/object-binding/security-current-integrated-v1 010ac60ddeb68ce702e1f3b2a4cfdd87345f2568
```
