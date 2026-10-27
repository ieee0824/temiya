# Observable global name resolution and var initializers

Global bindings may be backed by a Proxy supplied through `Realm::new`. Their
HasBinding operation is observable and must occur before a read or assignment
RHS, even when the compiler has collected the name. The compiler therefore
retains direct slots for function/block bindings but uses name resolution for
global reads and assignments. `GetName { name, strict }` resolves and reads a
name using the source Reference's strictness; the preexisting `LoadName`
instruction retains its strict read contract for compatibility.

A var initializer resolves its target before evaluating the initializer, then
writes through that retained Reference. AST evaluation follows the same order.
This also makes a var declaration inside with update the matching object
binding, including when the initializer deletes the property before PutValue.
Lexical initialization continues to initialize its declared environment binding.

Regression tests record the observable Proxy has/RHS trace, strict/sloppy reads
after deletion, global assignment and var initialization, and with binding
selection. The fixed original Test262
`language/statements/variable/binding-resolution.js` is executed as a complete
AST program with the original sta/assert harness. VM still rejects with; that
is reported as unsupported, not a passing VM execution. Existing TDZ/const,
function, scope and abrupt-completion tests remain required.

This fix does not enable Auto by default or complete #307/#427. Changes to AST
var initialization may affect execution cost; the fixed workload comparisons
must be reported separately from correctness.

Primary semantics: [Variable Statement evaluation](https://tc39.es/ecma262/multipage/ecmascript-language-statements-and-declarations.html#sec-variable-statement-runtime-semantics-evaluation)
and [GetIdentifierReference](https://tc39.es/ecma262/multipage/executable-code-and-execution-contexts.html#sec-getidentifierreference).

## Completed default-AST comparisons

[Initial run 40476436210](https://github.com/ieee0824/temiya/actions/runs/40476436210)
measured ea931ad219db536095de79256a358d67044ec575 against main
eec364ea823abe85ae32ca709becef4b00c2664b (artifact 12722629383).
[Function fast-path run 40479609878](https://github.com/ieee0824/temiya/actions/runs/40479609878)
measured 4e01f072f54811fc883b4f14dec9d7c74ceb7bf9 against main
c218bc518e0da4beb1ec88f46bd5ed44d07959d5 (artifact 12725310063).
Both use the default AST interpreter. Each has 33 before/after comparisons
within the fixed noise threshold, with no detected regression or speedup.
Actual pinned omoikane Boa is faster beyond threshold in all 33 comparisons
in each run. The two runs use different source combinations and baselines;
they do not isolate the function fast path or establish a performance gain.

All original eleven workloads and iterations are retained with 15 timed samples,
one checked warmup and three alternating pairs, serial 512 MiB/180-second guards.
Every report contains 66 successful workers and 1,056 validated results. All
medians/MAD values and pair classifications were independently recomputed;
original manifests/oracles, unique coverage, limits, modes, exact revisions,
compiler and artifact build metadata were verified. Raw reports use the prefixes
global-resolution-initial and global-resolution-fast in benchmarks/.
[Provenance](../benchmarks/global-resolution-provenance.json) records report hashes
and build identities.

These observations predate the subsequent integration with array literal main
revision 674528b639e0b4b1b7c191efa94982754adffcc7. The integrated runtime at
e03c7e1c5a417ce16a1f1ee12c1fcb1d158b39a1 passed CI 40483634621 and security
40483627761; its separate final comparison is run 40483784397. That result must
be verified separately before merge. This report/documentation addition changes
no runtime code, does not enable Auto by default and does not close #307/#427.
