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
that any #160 runtime work is implemented.

The issue scope is private methods, private getters/setters and static private
elements, including readonly method writes, absent accessor halves, receiver
checks, per-class identities and inheritance. Tests must enter through public
Engine Ast/Auto and include exception order, repeated class evaluation,
reentrant GC, escaped functions and cycle reclamation. Static private brands
must be checked separately from ordinary prototype inheritance and Proxy
forwarding. Existing private instance field behavior and fixed performance
controls must remain qualified before merge.
