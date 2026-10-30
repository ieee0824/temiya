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

Intermediate ARM64 observations are retained in class-bridge-v1 and
ast-arguments-v1. The first has no significant Boa wins and9 slower comparisons.
Conservative arguments-object elision improves11 of15 before/after comparisons,
but still has6 slower Boa comparisons. Both retain the full120 worker runs and
1920 checked values, including unchanged-binary and baseline/Boa controls.
Neither candidate is speed-qualified. Run verify-performance.mjs to reconstruct
all raw values and statistics and independently replay both601-variant captures
against their preceding outcomes and failed diagnostic signatures.

The normalized non-strict leaf follow-up is retained in sloppy-leaf-v1. It
improves11 of15 comparisons against the arguments-elision candidate, with
3 significant Boa wins,9 within noise and3 slower. It is still not qualified.
All791 native tests pass, with26 ignored. The archive also retains base and
derived originals and the216-variant PR gate. The first receiver test used an
unavailable globalThis builtin; its rejected log is retained, and the corrected
test compares against captured top-level this.

The object-initializer bridge at c78b9ba is retained in object-bridge-v1. It has
5 significant Boa wins and10 noisy comparisons. Against a4c2577 it has7
improvements,7 noisy comparisons and1 slowdown; that adverse comparison remains
in the archive. The identical-candidate control has15 noisy comparisons. All
798 native tests pass, with26 ignored; the fixed super and derived outcomes and
failed signatures are unchanged. This candidate still does not qualify the
entire super suite. verify-performance.mjs reconstructs all four intermediate
candidates rather than selecting only their favorable observations.
