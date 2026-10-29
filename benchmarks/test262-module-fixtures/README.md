# Module fixture qualification (in progress)

Issue #149 validates real module resolution/link/evaluation through the public
Engine and the actual Test262 CLI. This candidate rejects unpaired UTF-16
specifier aliases at UTF-8 host boundaries, keeps valid Unicode names working,
and validates named indirect exports before any module body executes.

A before-fix authored corpus imported `./\uD800.js` but loaded a real U+FFFD
filename and passed. Its retained corpus revision is null; it is not an original
Test262 test. The actual CLI regression now rejects both unpaired surrogate
ranges, retains U+FFFD and valid surrogate-pair names, and prevents the host error
from satisfying a language TypeError negative. Public Engine tests also ensure
the UTF-8 source callback never receives a lossy alias.

Linking now validates plain, renamed and circular missing re-exports, caches
SyntaxError failures and executes no module body on failure. Relative fixture
canonicalization, single evaluation of duplicate normalized imports and actual
cyclic live bindings are checked through the CLI.

Initial recorded checks:

- Full native suite: 699 passed / 26 ignored, 91 result groups. Formatting and
  all-target/all-feature Clippy with warnings denied passed.
- The new fixed selection executes nineteen original Module variants for each
  of AST and Auto, all successful. These cover cyclic graphs, missing exports,
  resolution SyntaxErrors and basic TLA. Module bodies use the AST module path;
  this does not qualify Bytecode module execution or full async evaluation.
- A broader 76-file original instantiation audit records 55 pass / 21 fail / zero
  skip. Missing class evaluation, the Function constructor required by the
  unchanged fnGlobalObject harness, and anonymous default-function initialization
  remain failures. No substitute harness or constructor is injected.
- The first eighteen-case audit before the link fix had fourteen pass/four fail
  for each backend. The two missing-indirect-export failures motivate the real
  linker fix; the TLA thenable/Promise gaps remain separately unqualified.

The fixed pin is d53ebc8b1931d9f1541e43d59650776e4b766248. Includes and fixtures
are read unchanged from Git objects. Gzip files preserve raw command/report bytes;
`evidence.json` records compressed and uncompressed SHA-256 values.

Strict Miri with collection at every allocation, the wider TLA audit, final CI
and new performance qualification are pending. The freshly built default Engine
worker differs from the previous source, so earlier Boa timing reports do not
qualify this candidate's speed. This draft is not ready to merge.
