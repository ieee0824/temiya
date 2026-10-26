# Tiering, deoptimization, and OSR design

The interpreter owns canonical JavaScript state. Profiling counters may request
Interpreter → Baseline and Baseline → Optimizing transitions, but code is
installed only after immutable `TieringMetadata` validates. Compilation happens
off the active frame; publication swaps a versioned code pointer atomically.

Every call, allocation slow path, loop backedge, and debugger poll is a GC
safepoint. Its `StackMap` classifies every generated frame slot as a tagged
root, unboxed scalar, or dead. Generated code may not call the allocator, host,
or collector outside such a point. Moving GC rewrites tagged roots described by
the map before execution resumes.

Speculative guards refer to `DeoptMetadata` at the same safepoint. Recipes list
inlined frames outermost-first, bytecode resume offsets, locals, and operand
stack depths. Reconstruction completes fully before JavaScript execution
continues; partially materialized frames are never observable. A thrown
exception deoptimizes first, then follows the interpreter's completion and
finally-unwinding machinery, preserving exception identity and side effects.

OSR is allowed only at a validated loop-backedge safepoint whose bytecode offset
matches the interpreter frame. Values are imported according to its stack map;
failed guards abandon the entry without changing the interpreter frame.
Optimized-to-optimized OSR is excluded initially.

Attaching a debugger or enabling step/breakpoint observation forces deopt to
canonical interpreter frames. Debugger polls are safepoints, breakpoint changes
invalidate affected optimized code, and inspection never reads speculative or
unmaterialized values. GC may keep the current tier only after scanning its
stack map; guard failure, exception, and debugger reasons return to Interpreter.
