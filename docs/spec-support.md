# ECMAScript support matrix

Temiya targets [ECMA-262, 17th edition (ECMAScript 2026)](https://tc39.es/ecma262/2026/),
published June 2026. The machine-readable source of truth is
[`spec-support.json`](../spec-support.json). Proposal status is frozen at TC39
proposals commit `07002d4d999558b9c7326f98e5134e0222b13808` from 2026-10-14;
new draft changes do not silently change this matrix.

Statuses have strict meanings:

- `implemented`: the tracked feature group has executable evidence linked in
  the JSON file.
- `unimplemented`: the behavior is in the selected standard/snapshot but its
  observable API is incomplete or absent.
- `out-of-scope`: the behavior belongs to an embedding/other standard, or a
  proposal has not been selected for implementation.

## Summary

| Area | Implemented | Unimplemented | Out of scope |
|---|---:|---:|---:|
| ECMA-262 core language/runtime | 10 | 1 | 0 |
| ECMA-262 built-ins | 5 | 2 | 0 |
| Annex B | 2 | 3 | 0 |
| Host hooks | 2 | 1 | 2 |
| Proposals after ES2026 | 0 | 3 | 1 |

The groups are deliberately observable and test-oriented rather than claims of
full clause conformance. For example, collector primitives are implemented but
the JavaScript `WeakRef` and `FinalizationRegistry` constructors remain marked
unimplemented. Similarly, parser support alone does not mark dynamic import as
implemented without its host loading hook.

Every feature change must update the JSON status/rationale and add or update its
test paths in the same pull request. Edition or proposal-snapshot changes are
explicit rebaselines and require reviewing every entry. Test262 uses the pinned
revision and skip policy documented in [the harness](test262.md).
