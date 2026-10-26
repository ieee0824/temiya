//! Temiya is a Rust-native JavaScript engine under development.
//!
//! The initial crate establishes the engine boundary. Parsing, bytecode,
//! execution, garbage collection, and embedding APIs will be added in small,
//! independently testable stages.

pub mod ast;
pub mod differential;
pub mod gc;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod semantics;
pub mod source;
pub mod test262;

/// Configuration used when constructing an [`Engine`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EngineOptions {
    /// Enables additional runtime checks intended for engine development.
    pub development_checks: bool,
}

/// An isolated Temiya engine instance.
#[derive(Debug)]
pub struct Engine {
    options: EngineOptions,
}

impl Engine {
    /// Creates an engine with the supplied configuration.
    #[must_use]
    pub fn new(options: EngineOptions) -> Self {
        Self { options }
    }

    /// Returns the configuration selected for this engine.
    #[must_use]
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EngineOptions::default())
    }
}

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
