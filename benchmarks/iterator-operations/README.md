# Synchronous iterator abstract operations

Before #161 implementation, selection.json freezes all1947 files and3880
variants in the metadata/path union recorded in selection-freeze.json. It is
selected from the complete pinned52133-file catalog without outcome filtering.
The immutable baseline is runtime622bc975 (pending #160), whose80 compiled
inputs are identical to this branch's starting point. This is not a claim that
#159 or #160 has merged or qualified for speed.

The implementation must share synchronous iterator acquisition, cached next
method calls, done/value reads and completion-aware closing. Existing array
literal and argument spread consumers must enter these operations through the
public Engine Ast/Auto paths. Keep read/call order, non-object validation, thrown
value identity, receiver identity and reentrant GC behavior covered.

The ECMAScript2026 operation definitions are the normative target:
https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-getiterator
https://tc39.es/ecma262/2026/multipage/abstract-operations.html#sec-iteratorclose

For-of integration remains #171, which explicitly depends on #161. Array binding
patterns remain #162; asynchronous protocol support and later builtins retain
their own issues. Their tests remain in this complete metadata selection, and
failures/timeouts cannot count as passes. No runtime implementation or new speed
qualification is claimed by this input freeze. Performance inputs must also be
frozen and checked with actual pinned Boa before first candidate timing.

The initial shared-record runtime is implemented locally. All878 native tests
pass (0failed26ignored), with Clippy, MSRV and stable i686 all-target checks.
Six10000-iteration workloads were frozen before implementation; actual pinned
Boa and the immutable baseline each validated every warmup/sample oracle. These
checks establish outputs only; no new native speed pass is claimed. The
1947-file/3880-variant baseline remains in progress.

validation-v1 retains the dedicated-target sourcec2bc478 build:878 native
passes,0 failures,26 ignored, Clippy/MSRV/i686 logs and36 successful output
validations across six cases and three engines/builds. verify-validation.mjs
checks source/input identities and preserves the rejected shared-target source
probe as unqualified. Use a separate CARGO_TARGET_DIR per worktree, then freeze
the verified executables before starting another build.

The comparison/audit tools accept --iterators. The hosted workflow records the
frozen suite when include_controls is true and its manifest exists. It retains
before/after, actual Boa, baseline/Boa and identical-binary controls with the
existing15sample/3pair policy; previous suites remain included. These wiring
and output checks do not establish native speed qualification.
