//! Temiya is a Rust-native JavaScript engine under development.
//!
//! [`Engine`] selects verified whole-Script [`bytecode`] execution for supported
//! Scripts and selects the AST interpreter before evaluation for other Scripts.
//! Explicit backend selection is available through [`EngineBuilder`]. The
//! optional `jit` backend is not selected by the embedding API. See the repository
//! README for runnable examples and current integration limits.

pub mod ast;
pub mod benchmark;
pub mod bytecode;
pub mod debug;
pub mod differential;
pub mod embedding;
pub mod ffi;
pub mod gc;
#[cfg(feature = "jit")]
pub mod jit;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantics;
pub mod source;
pub mod test262;
pub mod tiering;
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
