# Whole-Script Auto integration candidate

This branch integrates verified VM optimizations into the current main and makes
Engine::default, Engine::new and the default builder choose Auto. A supported
whole Script executes as verified bytecode; an unsupported Script is selected
for AST execution before any observable effect. Runtime errors are propagated
once and never trigger an AST retry. Explicit Ast/Bytecode configuration remains
available. Modules retain their existing module path.

The AST-function loop tier proposed in #491 is excluded: AST function creation
and calls remain the existing main implementation. There is no new TLS loop
policy, lazy loop compilation or per-AST-function cache. The old #492 performance
observation is retained and is not declared resolved by these source changes.
Supported functions in a selected VM Script still use verified VM function bodies.

The integrated runtime includes checked local binding/property hints, primitive
Number/String operations, guarded ordinary Array push, precise arguments-object
observation, prepared parameter records, local compound/update operations, inline
small call arguments and direct call-target reads. Regressions retain TDZ/const,
callback reentry, Proxy/getter order, receiver snapshots, iterator errors and
arguments/constructor behavior. Unsupported language forms remain unsupported;
this is not a whole-spec or GC completion claim.

The benchmark worker uses Engine::default and records actual setup/invocation
backends. Setup and invocation Script destruction occur before their respective
elapsed timestamps. Reports use mode Auto Script; the older experimental
Auto Script+loop reports are different source combinations and are not final
performance acceptance for this branch.

This candidate is not adopted until its own CI/security and unchanged original
11-workload before/after plus actual omoikane Boa measurements have been checked.
Older x64 results (31 faster / 2 within noise) and the ARM64 primitive-String-method
slow pair remain observations of the earlier candidate, not proof for this one.
#427, #291/#292 and the outstanding original-suite/compatibility work remain open.
