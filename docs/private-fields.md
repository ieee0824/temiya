# Private class elements

The initial #159 implementation executes private instance fields through Ast
and Auto. Each class evaluation creates distinct private names captured through
lexical environments. Field values live in a sparse, traced object payload;
ordinary property lookup, enumeration, prototype traversal and Proxy traps do
not expose or forward them.

Private fields initialize in declaration order alongside public instance fields,
using the existing base/derived constructor timing. Reads, writes, update and
compound assignment, calls and `#name in receiver` use the private identity.
Missing brands and non-object receivers throw TypeError. A repeated addition of
the same private name to a stamped receiver throws TypeError. Initialization may
add private fields to nonextensible ordinary objects, per the2026 specification.

Private methods and accessor pairs share their functions across instances and
install before instance fields. Accessors call their getter or setter with the
actual receiver; missing halves and writes to methods throw TypeError. Static
private methods install before static initialization. Static fields initialize
in declaration order. Static private brands belong to the defining constructor
and do not transfer to subclasses through prototype inheritance.

Auto falls back to AST for private operations that bytecode cannot represent.
Public Engine integration and reentrant GC tests are in tests/private_fields.rs
and tests/private_elements.rs. The fixed official-suite selection and performance
workloads for #160 are under benchmarks/private-elements; qualification is still
pending. These examples do not establish complete feature conformance.

Auto also prepares simple private calls and parameter-to-private assignments,
including a subsequent readback. Getters/setters still run normally against the
actual receiver, and readback invokes the getter after the setter returns.
Optional calls, spread arguments and effectful RHS expressions retain AST
evaluation. Shared code stores syntax while immutable names bind per closure.
Auto private-expression plans bind immutable private names per closure. Shared
FunctionCode stores syntax only, so repeated class evaluations keep separate
brands. Names have no JavaScript edges; captured environments remain traced.
Prepared simple base constructors avoid loading unused lexical-this roots while
retaining field initialization, ordinary setters and caller/body exception realms.

The sparse private payload holds its first slot directly and grows additional
slots geometrically. Ordinary object records keep the same optional-box field.
The collector traces both the first slot and all additional values. Prepared
constructor private targets bind per closure; captured environments load only
when a missing binding requires the ordinary fallback. Getter/setter effects and
assignment order remain in the shared runtime.
