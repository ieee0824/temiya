Complete security capture for integrated source `6c65967973ad4de72986db751d7c7e30547251c2`.

All 19 jobs passed. The verifier checks every retained log hash, all 16 exact-test plans and executions, their complete disjoint partition, strict Miri flags, ASan test summaries and dependency-audit command. This validates this recorded source only and does not qualify native Boa speed. Later runtime changes need their own safety run.

Replay from the repository root:

```sh
node benchmarks/array-binding/security-current-integrated-v1/verify.mjs benchmarks/array-binding/security-current-integrated-v1 6c65967973ad4de72986db751d7c7e30547251c2
```
