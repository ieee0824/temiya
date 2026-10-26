# Lexer and parser fuzzing

Install `cargo-fuzz`, then run `scripts/fuzz`. The `lexer_parser` target accepts
arbitrary bytes, processes every valid UTF-8 input through tokenization and the
expression, Script, and Module parsers, and validates every successful AST and
every success/error byte span against the original source.

The default run is deterministic in its resource envelope: 60 seconds total,
a five-second per-input timeout, and a 2048 MiB RSS limit. Override these with
`FUZZ_SECONDS`, `FUZZ_TIMEOUT_SECONDS`, and `FUZZ_RSS_MB`.

Seeds live under `fuzz/corpus/lexer_parser`. Minimized crashes must be copied to
`fuzz/regressions/lexer_parser`; those fixtures also run under ordinary
`cargo test`, so fixed crashes remain covered without requiring libFuzzer.

The `gc_interpreter` target maps bytes to bounded sequences of allocation,
rooting, barriered mutation, full/minor collection, Script evaluation,
exceptional jobs, and job-queue checkpoints. It runs the heap and write-barrier
verifiers after every transition. Its corpus and promoted regressions use the
same directories under `fuzz/corpus/gc_interpreter` and
`fuzz/regressions/gc_interpreter`; run it as `scripts/fuzz gc_interpreter`.
