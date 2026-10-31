# Synchronous iterator operations

Array literal and argument spread share iterator acquisition, a captured next
method, ordered done/value reads and completion-aware closing. Each record owns
rooted iterator/method values, and releases object-state borrows before calling
user code. Stable string keys are prepared per record rather than allocated for
every step.

Iterator method and next-result values must be objects. Abrupt next, done or value
reads mark the record done and propagate without calling return. Consumer errors
close the iterator once. An existing JavaScript throw takes priority over cleanup
errors; normal closing validates the return result. Host invariant errors remain
host errors and cannot be hidden by JavaScript cleanup.

Public Engine Ast/Auto integration and reentrant GC coverage are in
tests/iterator_operations.rs; explicit VM spread controls remain in
tests/bytecode_calls.rs. Direct operation tests cover optional next arguments,
normal/throw closing and completion identity.

The target is the [ECMAScript2026 iterator operations](https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-getiterator).
For-of integration is #171, array binding patterns #162. Global Iterator APIs and
asynchronous iteration retain their own issues.

The fixed suite records 235 passes, 1983 failures and 1662 timeouts in each of
Ast and Auto, with no missing outcomes. Full Test262 records 29701 passes, 61923
failures, 18 skips and 8569 timeouts across 100211 variants, preserving every
prior pass. These results do not establish complete language conformance.

All 52 fixed native workloads qualify against pinned Boa on each of two
independent x64 hosts, Intel8370C and AMD7763. All six iterator cases win every
one of the three alternating pairs on both hosts under the unchanged sample,
noise, watchdog and RSS gates. The results cover those workloads and conditions.
See [native records](../benchmarks/iterator-operations/hosted-dense-marks-v1/evidence.json),
[full capture](../benchmarks/iterator-operations/full-dense-marks-v1/evidence.json)
and [security replay](../benchmarks/iterator-operations/security-dense-marks-v1/README.md).
