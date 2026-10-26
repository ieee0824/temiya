//! Stable, thread-confined Rust embedding boundary.

use std::{
    any::Any,
    fmt,
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::runtime::{Completion, Realm, RuntimeError, RuntimeErrorKind, ScriptRecord, Value};

/// Configuration used when constructing an [`Engine`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EngineOptions {
    /// Enables additional runtime checks intended for engine development.
    pub development_checks: bool,
}

/// Builder that transfers configuration and optional host-owned state into an engine.
pub struct EngineBuilder {
    options: EngineOptions,
    host_state: Option<Box<dyn Any>>,
}

impl EngineBuilder {
    /// Creates a builder with stable default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: EngineOptions::default(),
            host_state: None,
        }
    }

    /// Enables or disables additional development checks.
    #[must_use]
    pub fn development_checks(mut self, enabled: bool) -> Self {
        self.options.development_checks = enabled;
        self
    }

    /// Transfers arbitrary thread-local host state into the engine.
    ///
    /// The state is dropped with the engine and can be borrowed only for the
    /// duration of a call to [`Engine::host_state`].
    #[must_use]
    pub fn host_state<T: Any>(mut self, state: T) -> Self {
        self.host_state = Some(Box::new(state));
        self
    }

    /// Builds a thread-confined engine.
    #[must_use]
    pub fn build(self) -> Engine {
        Engine::from_parts(self.options, self.host_state)
    }
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// An isolated, thread-confined Temiya engine instance.
///
/// `Engine` deliberately does not implement `Send` or `Sync`. All realms,
/// compiled scripts, and values must be used through the engine that created
/// them. Owned handles avoid exposing runtime borrows across API calls.
pub struct Engine {
    id: u64,
    options: EngineOptions,
    host_state: Option<Box<dyn Any>>,
    thread_confined: PhantomData<Rc<()>>,
}

impl Engine {
    /// Creates an engine with the supplied configuration.
    #[must_use]
    pub fn new(options: EngineOptions) -> Self {
        Self::from_parts(options, None)
    }

    /// Starts configuring an engine.
    #[must_use]
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    fn from_parts(options: EngineOptions, host_state: Option<Box<dyn Any>>) -> Self {
        static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ENGINE.fetch_add(1, Ordering::Relaxed),
            options,
            host_state,
            thread_confined: PhantomData,
        }
    }

    /// Returns the configuration selected for this engine.
    #[must_use]
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }

    /// Borrows host state when its concrete type matches `T`.
    #[must_use]
    pub fn host_state<T: Any>(&self) -> Option<&T> {
        self.host_state.as_ref()?.downcast_ref()
    }

    /// Creates a fresh Realm owned by this engine.
    #[must_use]
    pub fn create_realm(&self) -> RealmHandle {
        let realm = Realm::ordinary();
        RealmHandle {
            engine_id: self.id,
            realm_id: realm.identity(),
            realm,
        }
    }

    /// Compiles Script source for a Realm, rejecting cross-engine handles.
    pub fn compile(&self, realm: &RealmHandle, source: &str) -> Result<CompiledScript, EmbedError> {
        self.require_engine(realm.engine_id)?;
        ScriptRecord::parse(realm.realm.clone(), source)
            .map(|script| CompiledScript {
                engine_id: self.id,
                realm_id: realm.realm_id,
                script,
            })
            .map_err(EmbedError::Compile)
    }

    /// Evaluates a compiled Script and returns an owned value or exception.
    pub fn evaluate(&self, script: &CompiledScript) -> Result<ValueHandle, Exception> {
        if script.engine_id != self.id {
            return Err(Exception::engine(
                "compiled script belongs to another engine",
            ));
        }
        match script.script.evaluate().map_err(Exception::runtime)? {
            Completion::Normal(value) => Ok(ValueHandle {
                engine_id: self.id,
                realm_id: script.realm_id,
                value: value.unwrap_or(Value::Undefined),
            }),
            Completion::Throw(value) => Err(Exception {
                kind: ExceptionKind::ThrownValue,
                message: "JavaScript throw completion".into(),
                thrown: Some(ValueHandle {
                    engine_id: self.id,
                    realm_id: script.realm_id,
                    value,
                }),
            }),
            _ => Err(Exception::engine(
                "unexpected unresolved abrupt completion at Script boundary",
            )),
        }
    }

    fn require_engine(&self, engine_id: u64) -> Result<(), EmbedError> {
        if engine_id == self.id {
            Ok(())
        } else {
            Err(EmbedError::EngineMismatch)
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        EngineBuilder::new().build()
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("id", &self.id)
            .field("options", &self.options)
            .field("has_host_state", &self.host_state.is_some())
            .finish()
    }
}

/// Owned Realm identity accepted by the stable embedding API.
#[derive(Clone, Debug)]
pub struct RealmHandle {
    engine_id: u64,
    realm_id: u64,
    realm: Realm,
}

/// Owned compiled Script tied to its creating engine and Realm.
#[derive(Clone, Debug)]
pub struct CompiledScript {
    engine_id: u64,
    realm_id: u64,
    script: ScriptRecord,
}

/// Owned JavaScript value handle with engine and Realm provenance.
#[derive(Clone, Debug)]
pub struct ValueHandle {
    engine_id: u64,
    realm_id: u64,
    value: Value,
}

impl ValueHandle {
    /// Borrows the runtime value for the duration of this Rust borrow.
    #[must_use]
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the handle and returns its runtime value.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.value
    }

    /// Returns whether two handles came from the same engine and Realm.
    #[must_use]
    pub fn shares_realm_with(&self, other: &Self) -> bool {
        self.engine_id == other.engine_id && self.realm_id == other.realm_id
    }
}

/// Failure while compiling at the embedding boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbedError {
    /// A handle was passed to an engine that did not create it.
    EngineMismatch,
    /// Parsing or static semantics rejected source text.
    Compile(RuntimeError),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineMismatch => formatter.write_str("handle belongs to another engine"),
            Self::Compile(error) => write!(formatter, "compile failed: {error}"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// Stable exception categories exposed to embedders.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExceptionKind {
    /// A JavaScript `throw` completion carrying a value.
    ThrownValue,
    /// TypeError.
    TypeError,
    /// SyntaxError.
    SyntaxError,
    /// ReferenceError.
    ReferenceError,
    /// RangeError.
    RangeError,
    /// Engine invariant or handle failure.
    Engine,
}

/// Owned exception information returned by evaluation.
#[derive(Clone, Debug)]
pub struct Exception {
    kind: ExceptionKind,
    message: String,
    thrown: Option<ValueHandle>,
}

impl Exception {
    /// Returns the stable exception category.
    #[must_use]
    pub fn kind(&self) -> ExceptionKind {
        self.kind
    }

    /// Returns the human-readable diagnostic.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the thrown JavaScript value when this came from `throw`.
    #[must_use]
    pub fn thrown(&self) -> Option<&ValueHandle> {
        self.thrown.as_ref()
    }

    fn runtime(error: RuntimeError) -> Self {
        let kind = match error.kind() {
            RuntimeErrorKind::TypeError => ExceptionKind::TypeError,
            RuntimeErrorKind::SyntaxError => ExceptionKind::SyntaxError,
            RuntimeErrorKind::ReferenceError => ExceptionKind::ReferenceError,
            RuntimeErrorKind::RangeError => ExceptionKind::RangeError,
            RuntimeErrorKind::InternalError => ExceptionKind::Engine,
        };
        Self {
            kind,
            message: error.message().to_owned(),
            thrown: None,
        }
    }

    fn engine(message: impl Into<String>) -> Self {
        Self {
            kind: ExceptionKind::Engine,
            message: message.into(),
            thrown: None,
        }
    }
}

impl fmt::Display for Exception {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Exception {}
