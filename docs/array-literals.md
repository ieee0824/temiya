# Array literals, holes and iterable spread

AST and VM accumulate array elements through shared operations. The AST defers
allocating its unpublished Array until the first value is stored or the literal
finishes. A spread that fails before yielding a value needs no destination
Array. The VM creates its Array/index operand pair eagerly. Both use the Realm's
intrinsic Array prototype and preserve fresh identity. Elisions advance the
next index without creating own properties, values create ordinary data
properties, and a final length write preserves trailing holes and empty spreads.
Nested arrays and spread elements are lowered without evaluating AST inside VM.
Yielded values remain rooted across destination allocation. Public Ast/Auto
tests collect during initial expressions, iterator calls and value getters,
then check order, trailing holes, distinct empty arrays and release of roots.

Array/call spread consumes `@@iterator`, captures `next` once, and reads each
result's done then value. It does not read an arbitrary source's length, and
strings yield Unicode code points. A hole in a source Array is visited by its
iterator and becomes an explicit undefined element in the destination. Object
spread uses own-key enumeration separately.

Abrupt iterator method/next/done/value evaluation propagates immediately,
does not evaluate later array elements and does not call return. A consumer
failure, including the existing implementation argument limit, closes the
iterator while retaining the original throw completion. No iterator or object
borrow survives callbacks. The accumulator's representable index is bounded by
2^53-1; final Array length outside the uint32 range raises RangeError. The VM
checks target/index values as well as the verifier's scope and operand checks.

Instructions: CreateArray pushes an Array/index pair, AppendArray and SpreadArray
consume a value above that pair, ElideArray advances the pair, and FinishArray
replaces the pair with the Array after writing length. These instructions require
an active Realm scope. Malformed operand types are runtime errors.

Tests compare AST and explicit VM for holes, trailing commas, empty/nested
spreads, astral strings, custom iterators, next replacement, abrupt completion,
and length overflow. [Pinned observations](../test262/array-literals-audit.json)
retain seven complete Test262 programs (including a custom iterator) in strict/sloppy variants under both AST
and explicit VM (28 outcomes per toolchain), all passing on stable and Rust
1.85. The original sta/assert harnesses run in AST; no original program is
wrapped, shortened, skipped or redirected to AST on lowering failure.

The existing complete loop audit still has 35 passes and 11 failures. Array
lowering now reaches the original eval call in for/scope-body-var-none.js;
that remains a failure for missing eval (#305), not a new pass. The boundary
checker was updated to require that precise diagnostic. Whole Test262 and
runtime iterator integration (#161/#172), object method/HomeObject support
(#289), and overall Boa superiority (#427) remain separate open work.

Primary semantics: [ArrayAccumulation](https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#sec-runtime-semantics-arrayaccumulation)
and [ArgumentListEvaluation](https://tc39.es/ecma262/multipage/ecmascript-language-expressions.html#sec-argument-lists-runtime-semantics-argumentlistevaluation).

The original six-file observation is retained in the audit record. The seventh
custom-iterator program was added after integration with object data lowering;
its whole original source now compiles to VM without AST fallback.

## Integrated default-AST performance observation

[Hosted run 40479086606](https://github.com/ieee0824/temiya/actions/runs/40479086606)
measured d086fff3bb82685a78d696a05e0c06c098fc8920 against main
c218bc518e0da4beb1ec88f46bd5ed44d07959d5 after object-data integration.
Both Temiya workers use the production default AST interpreter. All 33
before/after comparisons were within the fixed noise threshold; none showed
an improvement or regression beyond that threshold. Actual omoikane Boa was
faster beyond threshold in all 33 comparisons. This does not establish a VM
speedup or Boa superiority and does not close #427.

The original eleven workloads, iterations and oracles are unchanged, with
15 timed samples, one checked warmup and three alternating pairs, serial
512 MiB/180-second guards. Each report contains 66 successful worker processes
and 1,056 checked values. All medians/MAD, ratios, thresholds and outcomes were
independently recalculated; original manifest/oracle coverage, worker identities,
compiler and artifact build metadata were checked. Artifact: 12723148662.

Raw [before/after](../benchmarks/array-integrated-hosted-beforeafter.json),
[Boa](../benchmarks/array-integrated-hosted-boa.json), and
[provenance](../benchmarks/array-integrated-provenance.json) are retained.
The following report/documentation commit changes no runtime code. The measured
revision's CI 40479039746 passed all four jobs; runtime revision
9c60dbc513323922daa31005d0d4e4dee56b309a passed all three security jobs in
40477102132. Subsequent source changes added only the seventh upstream test and
its observation record, with 28 original AST/VM outcomes on each toolchain.
