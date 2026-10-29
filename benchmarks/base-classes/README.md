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

The intermediate `auto-functions-v1` capture uses clean source `d818832`.
Eligible Auto function bodies now use verified bytecode, while explicit Ast
continues using AST evaluation. The fixed class suite improves in 11 of 12
comparisons against the initial implementation, with one within noise. Against
actual Boa, six comparisons are slower and six remain within noise. Its
same-binary control has twelve within-noise decisions. This intermediate version
still does not meet the class speed goal. All three raw reports, worker identity,
controller snapshots, and the 728-test log are retained. Functional result JSON
records 256 passes and 38 dependency failures in each backend and a passing
214-variant PR gate; the JSON alone is not a reconstruction of those CLI reports.

`node benchmarks/base-classes/verify-auto-functions.mjs` independently checks the
class measurements and their statistical decisions and validates archived file
identities and the recorded functional result summaries. Original-eleven
measurements are separate and cannot establish class speed.
