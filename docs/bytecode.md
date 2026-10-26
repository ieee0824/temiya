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
| `Add`, `StrictEqual` | `-1` | two inputs, one result |
| `Jump(target)` | `0` | target is an instruction |
| `JumpIfFalse(target)` | `-1` | one input and valid target |
| `Return` | `-1` | exactly one value, terminal |

`Bytecode::verify` performs a work-list traversal before dispatch is allowed.
It validates pool and frame operands, rejects stack underflow, requires equal
stack depth at control-flow joins, proves every path terminates with `Return`,
rejects unreachable instructions, and computes the maximum stack allocation.
Only the resulting `VerifiedBytecode` is intended for execution by the VM.
