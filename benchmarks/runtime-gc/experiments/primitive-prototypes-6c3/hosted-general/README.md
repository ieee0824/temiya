# Snapshot6b1 hosted original-eleven follow-up

Run40773535786 completed both independent machines. Runtime source is6c3;
snapshot6b1 has identical src/examples/Cargo/runner objects. The policy-test-only
successor4b2 also has identical runtime/worker sources. Reports and controls
retain original11 inputs/counts/oracles,15 samples and3 alternating pairs,
512MiB/180-second watchdogs and max(10%,3 times summed relative MAD) gate.

| Host | Candidate/actual Boa | Time ratios | Main/Boa | Same executable | Candidate/main |
| --- | --- | --- | --- | --- | --- |
| AMD EPYC9V45 |33 faster|0.408–0.810|33 faster|33 noise|9 faster/17 noise/7 slower|
| AMD EPYC7763 |33 faster|0.417–0.738|33 faster|32 noise/1 slower|9 faster/17 noise/7 slower|

All8 reports each validate66 workers/1056 values. The actual-Boa comparisons
qualify every33 decision on each host; no pair/sample or unfavorable control is
discarded. Same-executable faster/slower labels diagnose variability and are
not code changes. The complete main regressions remain in beforeafter.json;
this is not a regression-free result. These cohorts do not establish arbitrary
machine/application superiority or erase the local32-faster/1-noise result.

The statistical verifier at the parent experiment recomputes every observation.
verify-identities.mjs separately checks report/build/control/manifest/compiler,
source and binary identities, original adapter source hash and actual vendored
features/lock. Full current library Miri remains a separate gate.
