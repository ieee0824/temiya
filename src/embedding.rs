//! Stable, thread-confined Rust embedding boundary.

use std::{
    any::Any,
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::runtime::{
    Completion, JsString, ModuleGraph, Object, Realm, RuntimeError, RuntimeErrorKind, ScriptRecord,
    SourceTextModule, Value,
};

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

    /// Roots a runtime value in an owned handle for a Realm.
    pub fn value(&self, realm: &RealmHandle, value: Value) -> Result<ValueHandle, EmbedError> {
        self.require_realm(realm)?;
        Ok(ValueHandle::new(self.id, realm.realm_id, value))
    }

    /// Installs a panic-isolated native function on a Realm's global object.
    ///
    /// Arguments and results are owned handles rooted by their runtime values.
    /// Reentrant calls into other embedding operations are supported, but a
    /// callback result from another engine or Realm is rejected.
    pub fn register_global_function<F>(
        &self,
        realm: &RealmHandle,
        name: &str,
        callback: F,
    ) -> Result<(), EmbedError>
    where
        F: Fn(HostCallContext, ValueHandle, Vec<ValueHandle>) -> Result<ValueHandle, HostError>
            + 'static,
    {
        self.require_realm(realm)?;
        let engine_id = self.id;
        let realm_id = realm.realm_id;
        let function = Object::native_function(move |this, arguments| {
            let this = ValueHandle::new(engine_id, realm_id, this);
            let arguments = arguments
                .iter()
                .cloned()
                .map(|value| ValueHandle::new(engine_id, realm_id, value))
                .collect();
            let context = HostCallContext {
                engine_id,
                realm_id,
            };
            let result = catch_unwind(AssertUnwindSafe(|| callback(context, this, arguments)))
                .map_err(|_| RuntimeError::internal_error("host function panicked"))?
                .map_err(HostError::into_runtime)?;
            if result.engine_id != engine_id || result.realm_id != realm_id {
                return Err(RuntimeError::type_error(
                    "host function returned a value from another engine or Realm",
                ));
            }
            Ok(result.value)
        });
        realm
            .realm
            .global_object()
            .create_data_property(name.into(), Value::Object(function))
            .map(|_| ())
            .map_err(EmbedError::Host)
    }

    /// Creates an owned ordinary object from host-provided properties.
    pub fn create_object<I, K>(
        &self,
        realm: &RealmHandle,
        properties: I,
    ) -> Result<ValueHandle, EmbedError>
    where
        I: IntoIterator<Item = (K, ValueHandle)>,
        K: Into<JsString>,
    {
        self.require_realm(realm)?;
        let object = Object::new();
        for (name, value) in properties {
            self.require_value(realm, &value)?;
            object
                .create_data_property(name.into().into(), value.value)
                .map_err(EmbedError::Host)?;
        }
        Ok(ValueHandle::new(
            self.id,
            realm.realm_id,
            Value::Object(object),
        ))
    }

    /// Defines an owned value as a writable global property.
    pub fn set_global(
        &self,
        realm: &RealmHandle,
        name: &str,
        value: ValueHandle,
    ) -> Result<(), EmbedError> {
        self.require_realm(realm)?;
        self.require_value(realm, &value)?;
        realm
            .realm
            .global_object()
            .create_data_property(name.into(), value.value)
            .map(|_| ())
            .map_err(EmbedError::Host)
    }

    /// Loads a module graph through a panic-isolated host source resolver.
    pub fn load_module<F>(
        &self,
        realm: &RealmHandle,
        specifier: &str,
        resolver: F,
    ) -> Result<ModuleHandle, EmbedError>
    where
        F: Fn(&str) -> Result<String, HostError> + 'static,
    {
        self.require_realm(realm)?;
        let graph = ModuleGraph::new(realm.realm.clone(), move |request| {
            let specifier = String::from_utf16_lossy(&request.specifier);
            catch_unwind(AssertUnwindSafe(|| resolver(&specifier)))
                .map_err(|_| RuntimeError::internal_error("module resolver panicked"))?
                .map_err(HostError::into_runtime)
        });
        let module = graph.load(specifier).map_err(EmbedError::Host)?;
        Ok(ModuleHandle {
            engine_id: self.id,
            realm_id: realm.realm_id,
            module,
        })
    }

    /// Links and evaluates a loaded module graph.
    pub fn evaluate_module(&self, module: &ModuleHandle) -> Result<(), Exception> {
        if module.engine_id != self.id {
            return Err(Exception::engine("module belongs to another engine"));
        }
        let exception = |error| Exception::runtime(error, self.id, module.realm_id);
        module.module.link().map_err(exception)?;
        module.module.evaluate().map_err(exception)?;
        Ok(())
    }

    /// Installs a panic-isolated callback for unhandled Promise rejections.
    pub fn register_job_error_callback<F>(
        &self,
        realm: &RealmHandle,
        callback: F,
    ) -> Result<(), EmbedError>
    where
        F: Fn(Exception) + 'static,
    {
        self.require_realm(realm)?;
        let engine_id = self.id;
        let realm_id = realm.realm_id;
        realm
            .realm
            .job_queue()
            .set_unhandled_rejection_hook(move |error| {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    callback(Exception::runtime(error, engine_id, realm_id))
                }));
            });
        Ok(())
    }

    /// Enqueues a panic-isolated host job on the Realm's FIFO job queue.
    pub fn enqueue_job<F>(&self, realm: &RealmHandle, callback: F) -> Result<(), EmbedError>
    where
        F: FnOnce() -> Result<(), HostError> + 'static,
    {
        self.require_realm(realm)?;
        realm.realm.job_queue().enqueue(move || {
            catch_unwind(AssertUnwindSafe(callback))
                .map_err(|_| RuntimeError::internal_error("host job panicked"))?
                .map_err(HostError::into_runtime)
        });
        Ok(())
    }

    /// Drains the Realm's Promise job queue to a microtask checkpoint.
    pub fn drain_jobs(&self, realm: &RealmHandle) -> Result<usize, Exception> {
        if realm.engine_id != self.id {
            return Err(Exception::engine("Realm belongs to another engine"));
        }
        realm
            .realm
            .job_queue()
            .drain()
            .map_err(|error| Exception::runtime(error, self.id, realm.realm_id))
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
        match script
            .script
            .evaluate()
            .map_err(|error| Exception::runtime(error, self.id, script.realm_id))?
        {
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

    fn require_realm(&self, realm: &RealmHandle) -> Result<(), EmbedError> {
        self.require_engine(realm.engine_id)
    }

    fn require_value(&self, realm: &RealmHandle, value: &ValueHandle) -> Result<(), EmbedError> {
        if value.engine_id == self.id && value.realm_id == realm.realm_id {
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

/// Owned loaded module tied to its creating engine and Realm.
#[derive(Clone, Debug)]
pub struct ModuleHandle {
    engine_id: u64,
    realm_id: u64,
    module: SourceTextModule,
}

/// Provenance-preserving factory available during a native host call.
#[derive(Clone, Copy, Debug)]
pub struct HostCallContext {
    engine_id: u64,
    realm_id: u64,
}

impl HostCallContext {
    /// Wraps a runtime value as an owned result in the current engine and Realm.
    #[must_use]
    pub fn value(self, value: Value) -> ValueHandle {
        ValueHandle::new(self.engine_id, self.realm_id, value)
    }
}

/// Owned JavaScript value handle with engine and Realm provenance.
#[derive(Clone, Debug)]
pub struct ValueHandle {
    engine_id: u64,
    realm_id: u64,
    value: Value,
}

impl ValueHandle {
    fn new(engine_id: u64, realm_id: u64, value: Value) -> Self {
        Self {
            engine_id,
            realm_id,
            value,
        }
    }

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
    /// A host object, module, or Realm operation failed.
    Host(RuntimeError),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineMismatch => formatter.write_str("handle belongs to another engine"),
            Self::Compile(error) => write!(formatter, "compile failed: {error}"),
            Self::Host(error) => write!(formatter, "host integration failed: {error}"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// Error returned by a host function or module resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostError {
    kind: ExceptionKind,
    message: String,
}

impl HostError {
    /// Creates a host TypeError.
    #[must_use]
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            kind: ExceptionKind::TypeError,
            message: message.into(),
        }
    }

    /// Creates a host ReferenceError.
    #[must_use]
    pub fn reference_error(message: impl Into<String>) -> Self {
        Self {
            kind: ExceptionKind::ReferenceError,
            message: message.into(),
        }
    }

    fn into_runtime(self) -> RuntimeError {
        match self.kind {
            ExceptionKind::TypeError => RuntimeError::type_error(self.message),
            ExceptionKind::ReferenceError => RuntimeError::reference_error(self.message),
            _ => RuntimeError::internal_error(self.message),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for HostError {}

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

    /// Returns the original throw value, or a native error object when it was
    /// materialized while crossing a JavaScript function boundary.
    #[must_use]
    pub fn thrown(&self) -> Option<&ValueHandle> {
        self.thrown.as_ref()
    }

    fn runtime(error: RuntimeError, engine_id: u64, realm_id: u64) -> Self {
        let kind = match error.kind() {
            RuntimeErrorKind::ThrownValue => ExceptionKind::ThrownValue,
            RuntimeErrorKind::TypeError => ExceptionKind::TypeError,
            RuntimeErrorKind::SyntaxError => ExceptionKind::SyntaxError,
            RuntimeErrorKind::ReferenceError => ExceptionKind::ReferenceError,
            RuntimeErrorKind::RangeError => ExceptionKind::RangeError,
            RuntimeErrorKind::InternalError => ExceptionKind::Engine,
        };
        Self {
            kind,
            message: error.message().to_owned(),
            thrown: error
                .thrown_value()
                .map(|value| ValueHandle::new(engine_id, realm_id, value.clone())),
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
