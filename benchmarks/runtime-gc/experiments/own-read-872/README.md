# Direct ordinary data reads, runtime source 8727ebc

Ordinary own data Get revives the current stored value without rebuilding a
complete descriptor. The position hint rechecks the full current key and
descriptor, never caching a Value. Named ordinary properties on String objects
can use this path; virtual length/indices, accessors, proxies, views/buffers and
pending function prototypes retain the generic path and original receiver.

The unchanged original11 suite uses15 samples and3 alternating pairs, original
counts/oracles and512MiB/180-second watchdogs. Both frozen release workers use
Rust1.99.1 and guest CPU0 on local ARM64; agent threads use guest CPUs1–3.
No local builds/tests/profilers overlap competitive timing. Guest affinity is
not host physical-core isolation; agents are restored to CPUs0–3 afterward.

- `beforeafter.json`: all66 workers/1056 values correct, all33 comparisons
  within the unchanged10%/MAD threshold versusefd. String-method nominal
  ratios0.917–0.984 are not a qualified improvement. No regression is detected
  beyond the gate, which does not establish universal absence of regressions.
- `boa.json`: all66 workers/1056 values correct; all33 faster beyond that gate
  versus actual pinned omoikane modified Boa. Time ratios0.356–0.739. This is a
  local-machine result, not a hosted or application-wide qualification.
- Native658 passed/21 ignored; Clippy and selected fixed Test262 passed.
  Three exact public integration Miri tests passed strict provenance/symbolic
  alignment with0 ignored, covering GC edges and AST/VM string-property
  redefinition, deletion, virtual properties and receiver behavior.
- Strict7 fixtures release while retained handles/clones remain usable. The
  separate instrumented original11 profile validates every oracle/count,
  zero final live/rooted cells, balanced root registration/removal and pause/
  remaining-wall accounting. Diagnostic timing is not competitive speed proof.
- `async/` retains all18 correct workers/288 values and14 adapter checks, but
  only2 of9 speed comparisons pass;7 remain within threshold. This does not
  qualify the full authored async suite. No samples or pairs are discarded.

An initial comparison was terminated for an incorrectly entered source revision
before qualification. Its raw partial stdout/progress and reason are retained;
no observation from it is accepted as a correctness or speed pass. Subsequent
runner source/binary identity matches the immutable frozen worker metadata.

Latest-source hosted/CI/full library Miri remain pending. The separate efd
record proves all119 library tests at that earlier source, not this runtime.
