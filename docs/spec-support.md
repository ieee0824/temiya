# ECMAScript support matrix

Temiya targets [ECMA-262, 17th edition (ECMAScript 2026)](https://tc39.es/ecma262/2026/).
The machine-readable source is [`spec-support.json`](../spec-support.json), schema 2.
The proposal snapshot remains TC39 proposals commit
`07002d4d999558b9c7326f98e5134e0222b13808` (2026-10-14).
The [SpiderMonkey target](spidermonkey-target.md) independently fixes the comparison
build and ECMA-402 boundary. Changing those pins requires an explicit rebaseline.

## What a status means

Each status applies only to the entry's stated scope:

| Status | Meaning |
|---|---|
| `implemented` | The entire explicitly bounded contract has executable evidence. A successful example alone is insufficient. |
| `partial` | Some observable behavior works; missing behavior or broader conformance remains unqualified. |
| `unconnected` | Syntax, a placeholder intrinsic, or a host component exists without the intended execution/API connection. |
| `unimplemented` | The specific behavior or own API member is absent. An inherited generic method is not a dedicated implementation. |
| `out-of-scope` | An embedding/other-standard boundary or an unselected proposal. |

Schema 1 used `implemented` for broad groups even when only a subset worked.
Schema 2 keeps the old group IDs, but reclassifies their incomplete scope as
`partial`. Class evaluation and the callable Promise placeholder are explicitly
`unconnected`. Engine runtime GC is integrated and no longer described as an
unconnected collector; incremental/generational collection is still separate.

## Reading the data

- `features` contains 38 overview groups, their test paths and related Issue numbers.
- `capabilities` contains 365 individual entries: 41 syntax forms, 264 methods,
  7 accessors, 23 constructors and 30 globals. Each has related Issues and one or
  more executable JavaScript probes, with separate expected parser, AST, explicit
  Bytecode and Auto outcomes. This is an enumerated inventory, not an exhaustive
  listing of every ECMA-262 production and member.
- `components` records twelve host/runtime/tooling boundaries. A Rust collection iterator,
  weak-handle primitive or Promise state machine does not prove that the matching
  JavaScript constructor or prototype method is exposed.
  The Engine-backed Test262 runner is one tooling boundary; its focused original
  cases do not qualify the whole corpus. Async completion has a separate partial
  component; complete Promise globals and general resource limits remain open.
- `test262.status: "not-run"` means the entire entry has no corpus qualification.
  It never means pass or skip. Focused, historical suite results retain their own
  scope in [`test262/`](../test262/) and [qualification records](runtime-ownership.md).
  They do not qualify an entire group, method or backend.

Probe outcomes are observations of the checked example. `true` can mean that an
absence check succeeded; it does not make the corresponding API implemented.
The separate `evidence_kind` distinguishes `absence`, `stub`,
`unconnected-boundary`, `examples` and `known-gap`. None qualifies a complete
contract, so this snapshot contains no `implemented` capability entries. The
validator rejects changing an observation into a completed implementation claim.
`false` deliberately records a known semantic gap, such as `(1.25).toFixed(1)`
returning `"1.2"` instead of `"1.3"` (#225), or Map ignoring iterable input (#228).
`ReferenceError`, `TypeError` and `RangeError` record particular runtime failures,
including expected language errors and explicitly documented implementation gaps.
An engine failure is accepted only with the specified boundary diagnostic;
unexpected compiler, verifier and runtime failures fail the test.

Examples from the actual public Engine paths:

| Entry/example | Parser | AST | Explicit Bytecode | Auto |
|---|---|---|---|---|
| Ordinary closure returning 42 | accept | true | true | true |
| Interpolated template string | accept | true | unsupported at compile | true |
| Generator yielding 42 | accept | true | unsupported at compile | true |
| Class declaration | accept | class engine failure | unsupported at compile | class engine failure |
| Promise constructor call returns undefined | accept | true (stub observation) | true (stub observation) | true (stub observation) |
| Array.prototype.map has no own implementation | accept | true (absence) | true (absence) | true (absence) |

Auto selects a supported whole Script before execution and may fall back to AST.
An Auto success is never evidence of Bytecode support. These authored probes do
not load the Test262 harness, run modules, or qualify JIT execution. Actual host
module evaluation, microtask checkpoints and GC have their linked integration
tests in `components`/`features`; the Script probes do not stand in for them.

## Running and updating the checks

```sh
cargo test --locked --test spec_support -- --nocapture
SPEC_SUPPORT_REPORT=/tmp/spec-support-observations.json \
  cargo test --locked --test spec_support \
  individual_probes_run_through_parser_and_public_engine_backends -- --nocapture
```

The runner parses every source and creates a fresh Engine/Realm for each backend
and probe. It checks exact Boolean results or the expected error category and
keeps explicit Bytecode rejection distinct from AST/Auto execution. No probe is
skipped. The optional report records each entry, probe index/source, path,
expected result, observed result and error diagnostic. A mismatched observation
fails the test, even when the new behavior is an improvement: update the entry's
status, rationale, test evidence and Issue reference together after reviewing it.
The normal CI stable/MSRV test jobs run this integration test.

Schema checks reject invalid states, duplicate IDs, empty Issue references,
missing test paths/probes, unknown or missing execution paths, unsupported corpus
pass claims and Auto-as-Bytecode evidence. Mutation tests exercise those checks.
Boundary probes include TDZ, const writes, missing arguments, optional-chain
side effects, empty arrays and incompatible built-in receivers. Additional tests
linked per entry exercise descriptors, coercion, exceptions and runtime GC.

For example, list missing method implementations directly from the JSON:

```sh
node --input-type=module -e 'import fs from "node:fs"; const m=JSON.parse(fs.readFileSync("spec-support.json")); for (const c of m.capabilities.filter(c=>c.kind==="method" && c.status==="unimplemented")) console.log(c.id, c.issues);'
```

The #143 change updates metadata, authored integration probes and documentation.
It changes no production Rust source, Cargo inputs, optimization eligibility or
benchmark protocol. The runtime tree is identical to the qualified
`dd7df4455bbcfcf8764464ff2185eb4650f1372a` source retained in
[the numeric-storage qualification](../benchmarks/managed-packed-numbers/experiments/fused-dd7/README.md):
all 33 fixed original-eleven comparisons were faster than the pinned actual
modified Boa on each recorded ARM64, AMD and Intel host. This metadata change is
not a new speed measurement or a claim about unmeasured JavaScript workloads.
