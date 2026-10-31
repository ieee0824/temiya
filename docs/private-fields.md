# Private instance fields

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

Private methods, accessors and static private elements remain rejected before
class side effects; those are #160. Auto falls back to AST for private operations
that the bytecode compiler cannot represent. Public API integration and GC tests
are in tests/private_fields.rs. Fixed Test262 results, complete conformance,
performance qualification and merge approval checks remain pending.

Auto private-expression plans bind immutable private names per closure. Shared
FunctionCode stores syntax only, so repeated class evaluations keep separate
brands. Names have no JavaScript edges; captured environments remain traced.
Prepared simple base constructors avoid loading unused lexical-this roots while
retaining field initialization, ordinary setters and caller/body exception realms.
