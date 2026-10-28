# JavaScript shell

Build with `cargo build --locked --release --bin temiya`, or run through Cargo:

```sh
cargo run --locked --bin temiya -- -e 'print(40 + 2);'
cargo run --locked --bin temiya -- first.js -e 'print(shared);' second.js
cargo run --locked --bin temiya -- --module app/main.js
printf 'print("小樽😀");\n' | cargo run --locked --bin temiya --
```

`-e SOURCE` and filenames can be interleaved and repeated. `-` reads UTF-8
source from stdin once; no inputs also reads stdin. `--` ends option processing
(use `./-` for a file literally named `-`). `--script` selects Script mode,
which is the default. `--module` selects Module mode for all inputs; the last
mode option wins. `--help` and `--version` return without creating an Engine.
This shell is a batch runner, with no interactive prompt.

`print(...)` and `console.log(...)` join their arguments with spaces and write
one newline to stdout. `console.error(...)` writes the same format to stderr.
The wrappers capture the initial built-in `String` function and perform actual
JavaScript conversion in argument order. Conversion can execute user code and
throw; its throw value is preserved through JavaScript catch. Streams are UTF-8;
unpaired UTF-16 surrogates become replacement characters at the output boundary.
The shell also installs `globalThis` as the global object. These shell globals
are installed only by the binary, not by every embedded Engine.

Normal completion values are not printed automatically. Successful execution,
help and version exit with code 0. Source reads, invalid UTF-8, compilation,
module resolution/linking, uncaught JavaScript exceptions, uncaught output failures and
unhandled job/rejection errors exit with code 1 and diagnostics on stderr.
Unknown options, a missing `-e` argument, invalid Unicode inline arguments and
repeated stdin exit with code 2. An error stops later input evaluation. Throw
reporting avoids calling user-defined object conversion again.

## Execution and module identity

Scripts use the default public Engine Auto backend in one Realm and execute in
argument order. Global `var`, lexical bindings and ordinary objects persist
across files and inline inputs; a conflicting lexical declaration fails. After
each successfully evaluated Script, the shell drains that Realm's Promise jobs
to a checkpoint and reports unhandled rejections before proceeding. Startup
installs output helpers once; the shell adds no instrumentation or changes to
normal Script execution in embedded users.

Module inputs form one static graph through public
`Engine::load_module_with_resolver` / `evaluate_module`. A synthetic root imports
the entries in command-line order. All loading/linking completes before entry
evaluation, so a later entry's parse/link error prevents earlier entries from
executing. Modules share the Realm/global object, while each retains its own
module bindings. The graph caches a module by canonical filename and import
attributes; filename aliases and symlinks resolve to the same identity and do
not repeat its effects. Cyclic imports use the existing module linker.

Entry files use canonical filesystem paths. An imported `./` or `../` path is
relative to the canonical importing file's directory, including nested imports;
absolute paths are also accepted. Inline/stdin modules resolve relative paths
from the process working directory. No extension inference or package search is
performed. Bare imports, URLs, import attributes, invalid-Unicode module names
and specifiers containing unpaired surrogates produce errors. Module sources
must be UTF-8. Promise jobs drain after graph evaluation, and unhandled rejection
reporting uses the same nonzero-status policy as Script mode.

The shell inherits the engine's documented syntax/runtime limitations, including
its current module/async behavior. It does not add dynamic import, package
resolution, filesystem/network JavaScript APIs, a task/event loop, execution
budgets or an integrated collector. `jit` remains an explicit development
backend and is not selected by this shell. Unsupported input is a failure, not a
performance result. Neither CLI availability nor these tests establish full
application compatibility or a Boa/SpiderMonkey performance victory.

## Verification

`cargo test --locked --test cli` launches the actual binary and checks input
order/global sharing, lexical conflicts, Unicode/stdin, separate streams,
conversion/throw identity, checkpoint/rejection behavior, option/I/O/UTF-8 errors,
nested imports, alias caching, cycles and module parse/link/runtime failures.
Embedding regressions additionally verify canonical referrers/cache identity,
callback panic isolation, import-attribute rejection and cross-engine handles.
Existing module graph tests retain the old resolver contract.

The corpus-dependent test is run explicitly in CI after fetching pinned Test262:

```sh
TEST262_DIR=target/test262 cargo test --locked --test cli \
  pinned_test262_through_actual_shell --all-features -- --ignored --nocapture
```

It reads original files from git revision
`d53ebc8b1931d9f1541e43d59650776e4b766248`, including the unmodified `sta.js` and
`assert.js` harnesses. Throw and ideographic-whitespace files run in strict and
sloppy Script variants; default-function export and duplicate lexical binding
files run in Module mode, with the latter requiring a SyntaxError and failing
exit status. These are six actual executions, not whole-corpus conformance;
the ignored test does not count as passed unless explicitly executed.

Local validation of this change passed all six selected Test262 executions
(0 failed, 0 skipped), the CLI/embedding regressions and the full all-feature
test suite. The release shell also executed the original 2,000,000-iteration
arithmetic body and returned 64. This is a correctness smoke check, with no
elapsed-speed comparison inferred from the CLI invocation.
