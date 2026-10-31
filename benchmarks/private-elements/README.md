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
timing must run without competing local captures/builds. No candidate timing
has been recorded yet.
