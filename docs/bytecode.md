# Portable bytecode

`temiya::bytecode::Bytecode` is a pointer-free stack IR. Constants contain only
portable ECMAScript primitives, instruction operands are fixed-width `u32`
indices, and branch operands are absolute instruction indices. Runtime objects,
native addresses, and platform-sized operands are intentionally excluded.

| Instruction | Stack effect | Validation |
| --- | ---: | --- |
| `LoadConstant(i)` | `+1` | `i < constants.len()` |
| `LoadLocal(i)` | `+1` | `i < local_count` |
| `StoreLocal(i)` | `-1` | one input and valid local |
| `Duplicate` | `+1` | one input |
| `Pop` | `-1` | one input |
| `Add`, `Subtract`, `Multiply`, `Divide`, `StrictEqual` | `-1` | two inputs, one result |
| `LogicalNot` | `0` | one input, one result |
| `Jump(target)` | `0` | target is an instruction |
| `JumpIfFalse(target)` | `-1` | one input and valid target |
| `Return`, `Complete`, `Throw` | `-1` | exactly one value, terminal |

`Bytecode::verify` performs a work-list traversal before dispatch is allowed.
It validates pool and frame operands, rejects stack underflow, requires equal
stack depth at control-flow joins, proves every path terminates with `Return`,
rejects unreachable instructions, and computes the maximum stack allocation.
Only the resulting `VerifiedBytecode` is intended for execution by the VM.

`BytecodeCompiler` lowers the baseline AST subset (primitive literals, locals,
assignment, arithmetic and strict equality, conditional expressions, blocks,
`if`, `while`, `return`, and `throw`) and immediately verifies its output.
Unsupported syntax returns a source-spanned `CompileError` rather than falling
back silently. `VirtualMachine` accepts only `VerifiedBytecode`, evaluates
operands left-to-right, uses the runtime's ECMAScript conversions, and returns
the same `Completion` variants as the AST statement evaluator.

## Experimental baseline JIT

The `jit` Cargo feature enables `BaselineJit`; it is absent from default builds
and `libc` is an optional dependency activated only by that feature. On
x86-64 Unix, verified straight-line numeric bytecode is lowered to SSE2 machine
code. Executable memory follows a W^X transition: bytes are written to an RW
mapping, then the mapping becomes RX before execution. Unsupported platforms,
instructions, completions, or register pressure return typed `JitError`s so the
embedder can explicitly select the bytecode interpreter fallback. This is an
experimental baseline backend, not an optimizing tier.
