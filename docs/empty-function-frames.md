# Empty function environments

Verified function bodies can execute against their captured outer environment
when their own empty function environment has no observer in the supported
execution contract. This is an allocation optimization of ordinary calls; the
[ECMA-262 call operations](https://tc39.es/ecma262/2026/#sec-prepareforordinarycall)
still define the observable Realm, receiver, binding and completion behavior.

The verifier records eligibility only after ordinary bytecode validation. Every
scope must have no bindings or declarations. Direct this reads use the receiver
passed explicitly to the VM. Other activation reads, direct eval and
arguments names, function/class creation instructions and unknown instructions
are excluded. Object-initializer metadata is scanned recursively; this, super,
meta-properties, arrows, classes, imports, async/generator functions and unknown
metadata forms exclude the optimization. The compiler separately scans object
metadata before deciding whether an implicit arguments object is needed.

Calls additionally require no parameters, no arguments object, no arrow semantics
and no construction newTarget. Receiver conversion still happens first; the
converted receiver and function root remain live across callbacks. Strict null,
undefined and primitive receivers remain distinct; sloppy receivers retain the
normal global substitution and boxing behavior. The VM runs
without scalar-local clearing or loop-cell reuse of the captured outer record.
Public caller-prepared function frames retain their existing execution path.

Tests verify static rejection, actual entry into the optimized path, captured
writes, distinct escaping local captures, reentrant GC, exception identity and
explicit this forwarding and fallback arguments/newTarget behavior. Ast arrow new.target is an existing
separate gap (#168), reproduced with the pre-change09766bb binary; nested
new.target metadata is statically excluded here. Corpus and independent hosted
Boa speed qualification remain separate requirements.
