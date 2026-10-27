# Selected SpiderMonkey comparison target

Issue #144 remains open. The candidate source is Mozilla's official Firefox
repository at `5a78f4525f81f7323c2c044965096723318af6e6`, committed
2026-10-27T04:28:09Z. [The source commit](https://github.com/mozilla-firefox/firefox/commit/5a78f4525f81f7323c2c044965096723318af6e6)
is immutable; no moving branch or unrelated distribution binary is a substitute.
The [Mozilla source guide](https://firefox-source-docs.mozilla.org/contributing/jujutsu.html)
identifies this repository as a source checkout location.

The target remains ECMA-262 edition 17 (2026), as already recorded in
spec-support.json. Intl comparisons use [ECMA-402 edition 13 (2026)](https://402.ecma-international.org/13.0/).
This selection neither upgrades Temiya's implemented feature statuses nor
assumes that the selected SpiderMonkey implements every standard feature.
Existing proposal statuses retain snapshot 07002d4d999558b9c7326f98e5134e0222b13808;
[the audited classifications](../benchmarks/targets/proposal-audit.json) confirm Stage 4 and expected publication in 2027 for resource management, iterator chunking, iterator includes and iterator join. The checker fetches the immutable official finished-proposal table and verifies its SHA-256 and all four entries. Expected years can change; no engine feature availability is inferred from committee stage.

[The target manifest](../benchmarks/targets/spidermonkey.json) records the source,
standard editions, build flags and execution-mode flags. The JS-only optimized
build follows [Mozilla's optimized shell instructions](https://firefox-source-docs.mozilla.org/js/build.html#optimized-builds):
optimization is enabled and debug checks disabled, with two build jobs and a
separate object directory. Full compiler/configuration evidence is retained
from the actual build rather than inferred from the source selection.

The opt-in `spidermonkey-target.yml` workflow builds the selected source and runs
nine actual shell checks in each mode (ordinary evaluation, numerical/UTF-16
boundaries, Intl, exceptions and build settings), repeated from a verifier path
containing spaces. The built dist/bin tree is archived with file modes and
runtime-library contents, a portable archive checksum, compiler versions and
dynamic-library requirements for subsequent comparisons. The interpreter mode disables
Ion, Baseline and Baseline Interpreter; the verifier additionally checks the
actual `getJitCompilerOptions()` values. The flags and corresponding option keys
were checked in pinned [shell source](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/shell/js.cpp)
and [JSAPI source](https://github.com/mozilla-firefox/firefox/blob/5a78f4525f81f7323c2c044965096723318af6e6/js/src/jsapi.h).
Default-mode settings are recorded, but do not prove workload JIT execution.

The initial source build and eighteen normal-path checks passed in
[run 40512263632](https://github.com/ieee0824/temiya/actions/runs/40512263632) at tool revision 6cf957f.
The path-handling and archive additions require a new successful workflow and
artifact verification before this updated tool revision is qualified. These are target-selection smoke checks, not a full
Test262 result, a Temiya conformance claim or a performance comparison. The
actual eleven-workload SpiderMonkey adapter, async/RegExp/GC suites and JIT
execution evidence required by #427 remain separate unfinished work. The
mandatory actual omoikane Boa comparison remains unchanged.
