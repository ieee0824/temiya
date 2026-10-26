# Numeric literal normalization

The AST interpreter borrows a numeric literal's original spelling when it has
no `_`. Literals with separators still use the existing removal algorithm.
Parsing, radix selection, BigInt conversion and error creation remain at the
same evaluation point. This does not cache literals at compile time or change
the owned AST, numeric representation, or public API.

The separate `numeric_allocation_probe` parses its fixtures once and repeatedly
evaluates them through the public `ExpressionEvaluator`. It compares allocator
requests with the preceding normalization/parse algorithm, warms each operation
128 times, and excludes parsing, Realm setup and report construction. Results
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
