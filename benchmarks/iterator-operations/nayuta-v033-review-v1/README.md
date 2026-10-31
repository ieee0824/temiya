# Nayuta v0.3.3 review

The latest published release at evaluation time was v0.3.3, commit
a693764b9ee4a771254a2fde7d05e7235dc417fc. A locked release build scanned all
77 Rust source files at Temiya be89d98 without parse errors and reported 493
heuristic candidates. Fixed-size loops are still included by the tool's stated
limitations; these are not 493 confirmed defects or native performance results.

The saved boundary probes check that closure, async and local-function bodies
start at their own loop depth, ordinary direct nesting is preserved after those
bodies, and nested loops inside a closure are still detected. Both probes passed.
No reproducible Nayuta problem was observed in these checks, so no new issue was
filed. The manifest retains input/output hashes and both source revisions.
