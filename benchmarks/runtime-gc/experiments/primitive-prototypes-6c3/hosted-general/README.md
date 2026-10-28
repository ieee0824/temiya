# Snapshot6b1 hosted original-eleven follow-up

Run40773535786, copy1 completed; copy2 remains pending. Runtime source is6c3;
snapshot6b1 has identical src/examples/Cargo/runner objects. The policy-test-only
successor4b2 also has identical runtime/worker sources. Reports and controls
retain the original11 inputs/counts/oracles,15 samples and3 alternating pairs,
512MiB/180-second watchdogs and max(10%,3 times summed relative MAD) gate.

AMD EPYC9V45 copy1: all4 reports each validate66 workers/1056 values. Candidate
versus actual pinned omoikane modified Boa passes all33 speed decisions, time
ratios0.408–0.810. Main versus Boa also passes33; identical-executable control
has33 noise decisions. Candidate versus main has9 faster/17 noise/7 slower;
three prop-mega ratios1.255–1.259, three array ratios1.211–1.272 and one
primitive-string-method ratio1.101 are retained as regressions. This is not a
regression-free result or evidence for arbitrary applications/machines.

The statistical verifier at the parent experiment recomputes every observation.
verify-identities.mjs separately checks report/build/control/manifest/compiler,
source and binary identities, original adapter source hash and actual vendored
features/lock. One host passing does not erase old sources' misses or qualify
the still-pending second host. Full current library Miri remains separate.
