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
