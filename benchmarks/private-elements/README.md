# Private element qualification inputs

Before implementing #160, selection.json freezes all4273 files and8488 variants
carrying any private field/method/static/brand metadata in the complete pinned
52133-file reference catalog. Selection uses metadata only, without filtering
execution outcomes. Existing instance fields remain included as regression
controls. Shared async, eval, generator and historical nonextensible dependencies
remain selected and must be reported separately from successes.

selection-freeze.json records the catalog/corpus identities and every input
hash. The baseline is immutable runtimeeaeaf711, the pending #159 implementation;
its binaries and build conditions are preserved in
/tmp/temiya-159-default-private-v1. This does not claim that #159 has merged or
that #160 has qualified for merge. Runtime implementation is committed at
6e3cbf46f58a68d702a60090fc137c998e670aa5; the fixed Ast/Auto captures
are in progress.

The issue scope is private methods, private getters/setters and static private
elements, including readonly method writes, absent accessor halves, receiver
checks, per-class identities and inheritance. Tests must enter through public
Engine Ast/Auto and include exception order, repeated class evaluation,
reentrant GC, escaped functions and cycle reclamation. Static private brands
must be checked separately from ordinary prototype inheritance and Proxy
forwarding. Existing private instance field behavior and fixed performance
controls must remain qualified before merge.

The six workload files were frozen before implementation in workload-freeze.json.
boa-oracles-v1 retains actual pinned Boa warmup/sample validations (two passes
per input), not a speed claim. The manifest and reference module preserve the
original file hashes, iteration counts and expected values. Run the existing
comparison and independent audit scripts with --elements,15 samples and3
alternating pairs. Require at least2 significant Boa wins per workload on each
host, with the existing noise policy and same-binary controls unchanged. Native
timing must run without competing local captures/builds. Native speed qualification remains pending; retain the initial hosted source
43b3cfd separately from newer runtime candidates.

Runtime622bc975 includes the private field optimizations from parenta65fa12
and prepares simple private calls, getter reads, parameter assignments and
setter readbacks. The shared code contains syntax/parameter positions; private
identities stay per closure. Optional calls, spreads, complex RHS expressions
and observable activations keep the ordinary evaluator path. Actual getters,
setters, receivers, thrown values and GC edges remain in the shared runtime.

private-plans-diagnostic-v1 retains870 passing native tests,0 failures and26
ignored tests; Clippy, MSRV and stable i686 checks; all six output oracles; and
complete Callgrind profiles. The fixed method-call input changes from256343432
to129864291 instructions, and accessor input from507019863 to177651256. Run
verify-private-plans.mjs to replay source/input identities and diagnostic totals.
These profiled counts do not establish a Boa speed pass. The newer fixed
Ast/Auto capture and final hosted/full-corpus/safety checks remain pending.

initial-ast-v1 retains the complete initial runtime6e3cbf4 Ast comparison:
2108 baseline passes preserved and1846 added (3954pass2506fail2028timeout).
verify-initial-ast.mjs reconstructs both captures and checks every prior pass.
Observed failure diagnostics are retained without waiving any result; this
Ast-only historical capture cannot qualify Auto or the newer optimized source.

full-private-plans-v1 retains source4d10d10 full Test262:29701pass61923fail
18skip8569timeout across100211variants,0missing. All27855 parent-candidate
passes remain with1846 additions and0regressions. verify-full.mjs replays all8
gates from the raw captures.

hosted-private-plans-v1 retains both hosts and all60 reports. All six new cases
record3/3 significant Boa wins on each host, but overall speed is not qualified:
private-invalid-receiver records0/3 on both, and copy2 derived-definition1/3.
verify-hosted.mjs recomputes every report, control and per-case gate, preserving
these failures. Parent159 changes and current-source requalification remain
required before merge.

auto-timeouts-v1 retains the complete optimized Auto capture:3886pass,2505fail,
2097timeout across8488 variants. Compared with the initial implementation,68
passes time out (26 also passed before private elements). All68 pass in a
separate same-binary, same1000ms/512MiB diagnostic across37 files; none of the69
Ast/Auto differences reproduces a timeout. This does not replace the original
outcomes or establish a root cause. Integrated-source full validation is required.

full-integrated-bindings-v1 retains sourcef60a02c full-corpus results:
29701pass61923fail18skip8569timeout,0missing across100211 variants; all29701
preceding passes remain and8 gates independently replay. Hosted run41172659495
in hosted-integrated-bindings-v1 qualifies all46 workloads on BOTH hosted jobs
(both EPYC9V45), with3/3 wins for all6 new cases on each. All60 reports replay.
Inputs,controls,noise/watchdogs remain unchanged; no CPU filter. Current-source
fixed Ast/Auto capture, safety and final evidence-head CI remain separate.
