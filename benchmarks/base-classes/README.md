# Base class qualification evidence

`initial` retains source `3bc78ae`, including unfavorable measurements. The
unchanged original-eleven workloads have 33 faster decisions against actual
modified Boa on the recorded ARM host and 33 before/after decisions within
noise. The same-binary control has 31 noise, one faster and one slower decision.
This result alone does not establish class performance.

The separate authored class suite was frozen before execution. Construction,
method and static-loop cases produce nine slower decisions; the definition case
produces three within-noise decisions. Its same-binary control has 11 noise and
one faster decision. These results prevent claiming that this initial class
implementation is faster than Boa. Counts, bodies, oracles and eligibility must
remain fixed during subsequent optimization comparisons.

Both workers validate every warmup and all 15 samples in each of three
alternating process pairs. The same 180-second and 512-MiB external watchdogs
apply. Each timed pass parses and evaluates the invocation; startup, class/shared
setup and warmup are excluded. Class fixtures are separate from the original
eleven, not a replacement. No JIT or whole-engine superiority follows.

```sh
node benchmarks/base-classes/verify-initial.mjs
```

The verifier checks all compressed/decoded identities, reconstructs original and
class measurements and statistical decisions, verifies worker identity and
checks the retained 725-test run and the failed-then-fixed caller regression.
The source, raw data and conditions remain available even after later fixes.
Integrity success preserves slow results; it is not performance acceptance.
