# Numeric literal normalization

The AST interpreter borrows a numeric literal's original spelling when it has
no `_`. Literals with separators still use the existing removal algorithm.
Parsing, radix selection, BigInt conversion and error creation remain at the
same evaluation point. This does not cache literals at compile time or change
the owned AST, numeric representation, or public API.

The separate `numeric_allocation_probe` parses its fixtures once and repeatedly
evaluates them through the public `ExpressionEvaluator`. It compares allocator
requests with the preceding normalization/parse algorithm, warms each operation
128 times, and excludes parsing, Realm setup and report construction. The [recorded observations](../benchmarks/numeric-normalization-allocations.json)
are allocation requests/bytes, not throughput, retained memory or RSS. Timing
workers do not link the counting allocator.

```sh
cargo run --locked --release --example numeric_allocation_probe > numeric.json
node scripts/check-numeric-allocations.mjs numeric.json
```

For 10,000 evaluations on the measured 64-bit target, the decimal, hexadecimal
and legacy-octal fixtures drop from 10,000 allocation requests to zero. The
BigInt fixture drops from 30,000 to 20,000: parsing the BigInt still allocates.
The three separator-containing fixtures retain the same requests and bytes.
The probe includes the allocator's actual requested capacity; it does not assume
that a String allocation is exactly the spelling's logical length.

Both numeric and existing string allocation budgets run on stable and MSRV in
CI. The probes share an unchanged System-forwarding counting allocator; the
string probe retains its own construction/clone/concatenation checks.

`tests/numeric_literals.rs` also evaluates decimal/exponent/radix forms,
separators, legacy octal, BigInt through 256 bits, signed zero, infinity,
subnormal and integer-rounding boundaries through the public Engine.
Existing lexical rejection tests remain in place. This optimization does not
claim new numeric syntax or full numeric conformance.

Full-engine performance must be evaluated separately with the original eleven
omoikane inputs and the existing result/noise/resource guards. Reducing an
allocator count does not establish a speedup against Boa or complete #427.
Immutable compile artifacts and function-code sharing remain separate work in
#412; this change only avoids a transient normalization buffer.

The [initial eleven-workload diagnostic](../benchmarks/numeric-normalization-diagnostic.json)
uses three samples and one pair, validates 88 results in 22 runs, and remains
diagnostic only. Some raw ratios are slower, including arithmetic at 1.18 and
closure allocation at 1.24. These observations are retained.

A [focused arithmetic followup](../benchmarks/numeric-normalization-arith-followup.json)
uses the same binaries and input with 15 samples and three alternating pairs.
Whole-process user+system CPU ratios are 0.96, 0.96 and 0.93; timed wall-median
ratios are 0.97, 0.95 and 0.92. Process CPU includes setup, warmup and reporting,
and does not replace timed medians. This focused run has a 180-second timeout
but no RSS watchdog; it cannot establish an all-eleven speedup or the absence
of regression. The full hosted comparison must be considered separately.


## Hosted x86_64 numeric-normalization comparison

[Run 40378506633](https://github.com/ieee0824/temiya/actions/runs/40378506633)
completed on Ubuntu 24.04, four logical CPUs. The
[provenance record](../benchmarks/numeric-normalization-hosted-provenance.json) retains
build identity and the original artifact reference. Both
[before/after](../benchmarks/numeric-normalization-hosted-beforeafter.json) and
[Boa](../benchmarks/numeric-normalization-hosted-boa.json) use the original eleven
workloads, 15 timed samples, one checked warmup and three alternating pairs.
Each mode has 66 successful runs and 1056 validated values. They run serially
after all builds, with the same compiler and existing 512 MiB/180-second guards.

The measured revision is `b2793d825fab8485e518106409515409b7f54553`;
the baseline is `2ba4d0b82a63069978b5d7b047ce4e7121f8ba2f`. The measured
runtime is identical to code revision `3bae87f905f172f9dbc742a1acccb3fdeb72ee57`.
Later ARM64 data commits do not modify executable source, worker scripts,
dependencies or the measurement workflow. Architectures are reported separately.

| Workload | Changed / prior Temiya time | Changed Temiya / Boa time |
| --- | ---: | ---: |
| arith | 0.95–0.97× | 3.53–3.63× |
| prop-mono | 1.01–1.02× | 2.72–2.77× |
| prop-mega | 0.98–0.99× | 2.42–2.46× |
| call | 0.97–0.98× | 2.37–2.41× |
| closure-alloc | 0.96–0.98× | 1.83–1.93× |
| object-alloc | 0.95–0.97× | 1.79–1.80× |
| string-concat | 0.98–0.98× | 3.16–3.19× |
| array | 0.98–0.99× | 2.37–2.40× |
| primitive-string-property | 0.96–0.99× | 2.91–2.92× |
| primitive-string-method | 0.94–0.96× | 2.19–2.21× |
| proto-method | 0.98–1.02× | 2.28–2.33× |

All 33 before/after pairs remain within the noise threshold; no throughput
speedup or regression beyond that threshold is established on this runner.
All 33 Boa pairs still find Temiya slower beyond the threshold. Both all-eleven
speed gates remain false. Allocation reductions do not satisfy the Boa goal
or complete #412/#427.
