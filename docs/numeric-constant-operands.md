# Number literal operands

The compiler can emit one `BinaryNumberConstant` instruction for a binary
expression whose right operand is a Number literal, including parentheses.
The pool retains the original f64 value. This removes the separate right-hand
constant load and its temporary VM stack slot, without folding or reordering
arithmetic. A BigInt literal, conditional, assignment or function call on the
right keeps its original instruction sequence.

The verifier requires one left operand, a valid pool index and an actual Number
constant. The instruction has zero stack delta and retains the 16-byte portable
instruction size. Existing control-flow, scope, pending-reference and call-state
checks still apply. This is an instruction-set extension; bytecode producers
must use the verifier rather than constructing a proof wrapper.

Primitive Number pairs use the existing shared number table. For other left
operands or operators outside that table, the VM uses the existing Realm-aware
binary operation. For example, `object * 7` still calls the object's conversion
hooks, `"v" + 2` concatenates, and mixed BigInt arithmetic still throws. A literal
Number has no observable right-hand evaluation or reference root. Values below
the left operand remain on the stack. No numeric reassociation or contraction
is introduced.

The optional x86_64 baseline JIT emits the same separate constant load and SSE
operation for its existing add/subtract/multiply/divide subset, retaining the
register limit. Other operators remain explicit unsupported-JIT results. The
normal engine does not enable that experimental backend.

Local release all-feature tests, the independent arithmetic and IEEE-bit oracles,
three dedicated verifier/public-API regressions, fixed Test262 20 executions and
Clippy passed. Native x86_64 code generation requires the hosted CI validation;
ARM64's unsupported-JIT platform tests do not prove native x86_64 execution.
Performance qualification is pending. Earlier typed-dispatch measurements do not
establish the speed of this instruction extension.
