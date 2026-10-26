//! Temiya is a Rust-native JavaScript engine under development.
//!
//! The initial crate establishes the engine boundary. Parsing, bytecode,
//! execution, garbage collection, and embedding APIs will be added in small,
//! independently testable stages.

pub mod ast;
pub mod benchmark;
pub mod bytecode;
pub mod differential;
pub mod embedding;
pub mod ffi;
pub mod gc;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantics;
pub mod source;
pub mod test262;
pub use embedding::{Engine, EngineBuilder, EngineOptions};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_preserves_its_configuration() {
        let options = EngineOptions {
            development_checks: true,
        };
        let engine = Engine::new(options.clone());

        assert_eq!(engine.options(), &options);
    }
}
