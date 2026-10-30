# Super property benchmark inputs

`workloads-v3` was fixed before the #156 runtime implementation. The manifest
and reference module retain exact sources, iteration counts and numerical
oracles for object/class super calls, accessors, computed updates and arrows
whose HomeObject prototype changes. Each of the five fixtures was checked twice
against the actual pinned Boa embedding before implementation.

Use `scripts/compare-class-workloads.mjs --super` and independently replay its
reports with `scripts/audit-class-workload-records.mjs --super`. Qualification
uses fifteen samples and three alternating pairs, the existing ten-percent or
three-relative-MAD threshold, and the same external watchdog for both engines.
Startup, shared setup and warmup are reported separately from timed samples.
Every warmup and sampled invocation must return the fixed expected result.
Identical-candidate and before/after controls retain noisy and adverse outcomes.
Successful replay establishes record integrity, not a speed acceptance claim.

The computed-update fixture uses a primitive name with a counted key-expression
evaluation and checks receiver state. Object-valued key conversion remains a
separate correctness requirement: the converted key is retained for an update.
An authored super-key probe exposed two conversions in the pinned Boa embedding,
while its ordinary-key probe performed one. Node24 also performed two conversions
in both probes. Those engine observations must not replace the one-conversion
oracle from the specification and pinned Test262. The rejected v2 assertion
copied the observed two-conversion behavior and is not used by this suite.

No benchmark results or performance qualification are established by these inputs.
