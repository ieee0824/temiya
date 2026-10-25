# Temiya

Temiya is an experimental JavaScript engine written in Rust.

The project is named after Temiya in Otaru, the starting point of Hokkaido's
first railway. That railway carried coal from Horonai to the Port of Otaru:
an engine route powered by coal, remembered in the name of a new engine built
with Rust.

## Goal

Temiya aims to provide a Rust-native alternative to SpiderMonkey. The project
will implement the engine as original Rust code rather than translate Mozilla
source code. Compatibility targets will be defined by public standards and
observable embedding contracts.

The initial milestones are:

1. ECMAScript source text, lexer, and parser
2. Values, objects, execution contexts, and an interpreter
3. Tracing garbage collector with explicit rooting
4. ECMAScript conformance through Test262
5. Stable Rust embedding API and C ABI
6. Bytecode and tiered optimization after interpreter correctness

Temiya is at the bootstrap stage and does not execute JavaScript yet.

## Development

```sh
cargo test --locked
cargo build --locked
```

Design decisions and compatibility scope will be documented before the
corresponding public APIs are stabilized.
