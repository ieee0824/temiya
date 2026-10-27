# Typed primitive-Number dispatch candidate

The arithmetic diagnostic identified substantial instruction-reference counts
in VM binary dispatch, string operator matching and operand management. The
existing integer remainder fast path is retained. The diagnostic used a reduced
20,000-iteration input under Callgrind, so its instruction counts and instrumented
elapsed times cannot establish a speedup on the original two-million input.

The candidate shares one primitive-Number operation table between AST and VM.
The VM retains its existing typed `BinaryOperator` for Number pairs and reuses
their operand slots, preserving roots beneath them. Primitive arithmetic has
no JavaScript callbacks. Other operand kinds and operators outside that table
continue through the existing Realm-aware conversion and error paths. Operations
are evaluated separately, without floating-point reassociation or contraction.
No new syntax, fallback retry, skipped binding validation or benchmark-specific constant
is introduced.

Binding reads also copy an actual stored Number directly instead of dispatching
the full Value clone. Record borrowing, name matching, initialization/TDZ and
write/const checks remain in place. Other kinds keep their owned clones; there
is no cached type assumption or skipped lookup. Bit-preservation and captured
update regressions cover this path in both AST and explicit Bytecode.

Validation includes the existing 2,653-case independent JavaScript oracle in
AST/VM, the IEEE-bit/BigInt bitwise oracle, and public Engine regressions for
array/object/pending-call roots, signed-zero/NaN/Infinity/comparison results,
ordered object conversion, string concatenation and abrupt throw identity.
Five unmodified fixed Test262 files cover fractional remainder, infinite divisors,
signed zero, multiplication overflow and BigInt fallback. All 20 executions
(strict/sloppy and AST/Auto) passed locally; Auto is asserted to use actual
Bytecode for each test body. Corpus-dependent tests are explicitly run in CI:

```sh
TEST262_DIR=/tmp/temiya-test262 cargo test --locked --test typed_number_dispatch \
    pinned_test262_typed_number_dispatch --all-features -- --ignored --nocapture
```

The optional `include_controls` input of `omoikane-benchmark.yml` adds a direct
baseline/actual-Boa comparison and a same-executable candidate control after the
ordinary before/after and candidate/Boa reports. It requires an Auto Script
baseline and probes both modes before measurement. Same-binary faster/slower
labels measure variability, not code improvements. `two_runners` records two
independent hosted machines with separate artifacts; it does not select or
pretend to guarantee an Intel CPU. Default inputs retain the original two reports
and artifact name. Every comparison keeps the original eleven workloads,
15 samples plus checked warmup, three alternating pairs, oracle and watchdogs.

An optional `required_cpu_model` substring requests a specific target, such as
Intel Xeon 6973P-C. Each job records its actual models before building. If the
target is unavailable, it retains an availability record and explicitly reports
that no measurements ran; that job status is never a correctness or speed pass.
The default empty requirement accepts any recorded CPU. This selects declared
hardware, not a favorable performance outcome.

Performance acceptance remains pending. #510 requires the actual Intel target;
an AMD-only success or a reduced diagnostic cannot close it. Original and failed
observations must remain available rather than selecting a favorable run.
