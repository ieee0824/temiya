# Array literals, holes and iterable spread

AST and VM now accumulate array elements through shared operations. An empty
Array with the Realm's Array prototype is created first. Elisions advance the
next index without creating own properties, values create ordinary data
properties, and a final length write preserves trailing holes and empty spreads.
Nested arrays and spread elements are lowered without evaluating AST inside VM.

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
retain six complete Test262 programs in strict/sloppy variants under both AST
and explicit VM (24 outcomes per toolchain), all passing on stable and Rust
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
