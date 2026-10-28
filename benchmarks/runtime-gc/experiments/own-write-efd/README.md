# Direct ordinary data writes, source efd0de5

An existing own writable data descriptor with the same ordinary receiver can
update its stored value without reviving/reconstructing complete descriptors.
The strong-edge barrier and same-heap check precede mutation. Descriptor flags
remain unchanged. Accessors, proxies, different receivers, arrays, string/view/
buffer objects and pending function prototypes retain the generic Set path.

The unchanged original11 workload suite uses15 samples and3 alternating pairs,
original counts/oracles and512MiB/180-second limits. Both release workers share
Rust1.99.1 and guest CPU0 on local ARM64; agent threads use guest CPUs1–3.
No build/test/profiler overlaps competitive timing. Guest affinity does not
establish host physical-core isolation; agent affinity is restored afterward.

- `beforeafter.json`: all66 workers/1056 values correct,4 faster/29 noise/0
  slower beyond the unchanged10%/MAD gate versus413. All three prop-mono pairs
  take0.698–0.754 times the prior duration. This is not universal no-regression
  evidence.
- `boa.json`: all66 workers/1056 values correct, all33 comparisons faster than
  actual pinned omoikane modified Boa through its public JsRuntime embedding.
  Time ratios0.391–0.718. This claim is bounded to the recorded local condition.
- Independent oracle/median/MAD/order/decision recomputation is recorded in
  `provenance.json`; no unfavorable pair or sample is discarded.
- Native657 passed/21 ignored; Clippy succeeded. Selected fixed Test262 object,
  array, function and coercion checks passed. Two public Object integration
  tests passed strict-provenance/symbolic-alignment Miri with0 ignored.
  CI40752694920 passed all4 stable/MSRV, quality and C ABI jobs.
- All7 strict ownership fixtures release while retained handles/clones work.
  The separate instrumented original11 `runtime-profile.json` validates every
  oracle and leaves zero live/rooted cells with balanced root registration/
  removal counts. Diagnostic one-pass timing is not competitive speed evidence;
  managed-cell counts exclude ordinary Rust backing allocations.

Current-source full library Miri40752694688 and hosted comparison40752838591
remain pending. Old source413's unfinished Miri was cancelled because this
measured implementation superseded it; its partial results and reason remain
in the adjacent413 directory. No full119 pass or cross-machine qualification
is inferred from local or earlier-source successes. Whole-application and
SpiderMonkey superiority remain unproved.
