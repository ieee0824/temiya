//! Stable, thread-confined Rust embedding boundary.

use std::{
    any::Any,
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::bytecode::{BytecodeCompiler, CompileError, VerifiedBytecode, VirtualMachine};
use crate::runtime::{
    Completion, JsString, ModuleGraph, Object, Realm, RuntimeError, RuntimeErrorKind, ScriptRecord,
    SourceTextModule, Value,
};

/// Script execution backend selected when an engine is built.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScriptBackend {
    /// Execute Scripts with the AST interpreter.
    Ast,
    /// Compile the whole Script to verified bytecode or return a diagnostic.
    Bytecode,
    /// Use bytecode when the whole Script is supported, otherwise use the AST.
    #[default]
    Auto,
}

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
    backend: ScriptBackend,
}

impl EngineBuilder {
    /// Creates a builder with stable default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: EngineOptions::default(),
            host_state: None,
            backend: ScriptBackend::Auto,
        }
    }

    /// Selects how Scripts compiled by this engine are executed.
    #[must_use]
    pub fn script_backend(mut self, backend: ScriptBackend) -> Self {
        self.backend = backend;
        self
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
        Engine::from_parts(self.options, self.host_state, self.backend)
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
    backend: ScriptBackend,
    runtime: crate::runtime::Runtime,
    // Transitional allocation selection. Removed when all traced payload and
    // active-root families are migrated; not an alternate execution backend.
    managed_allocation: bool,
    thread_confined: PhantomData<Rc<()>>,
}

impl Engine {
    /// Creates an engine with the supplied configuration.
    #[must_use]
    pub fn new(options: EngineOptions) -> Self {
        Self::from_parts(options, None, ScriptBackend::Auto)
    }

    /// Starts configuring an engine.
    #[must_use]
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    fn from_parts(
        options: EngineOptions,
        host_state: Option<Box<dyn Any>>,
        backend: ScriptBackend,
    ) -> Self {
        static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_ENGINE.fetch_add(1, Ordering::Relaxed),
            options,
            host_state,
            backend,
            runtime: crate::runtime::Runtime::new(),
            managed_allocation: false,
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
        let realm = if self.managed_allocation {
            Realm::ordinary_in(&self.runtime)
        } else {
            Realm::ordinary()
        };
        RealmHandle {
            engine_id: self.id,
            realm_id: realm.identity(),
            realm,
        }
    }

    /// Roots a runtime value in an owned handle for a Realm.
    pub fn value(&self, realm: &RealmHandle, value: Value) -> Result<ValueHandle, EmbedError> {
        self.require_realm(realm)?;
        if let Value::Object(object) = &value {
            realm
                .realm
                .global_object()
                .check_allocation_context(object)
                .map_err(EmbedError::Host)?;
        }
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
        let function = realm
            .realm
            .create_native_function(name, 0, move |this, arguments| {
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
            })
            .map_err(EmbedError::Host)?;
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
        let object = Object::new_for(&realm.realm.global_object());
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

    /// Loads a module graph with referrer-aware canonical module identities.
    ///
    /// `resolve` receives `None` for the root and the canonical importing
    /// module's name for dependencies. It returns the canonical name used for
    /// caching and for `source`. Attributes remain part of module identity;
    /// hosts can reject unsupported attributes in `resolve`. Both callbacks are
    /// panic-isolated. Existing [`Self::load_module`] resolution is unchanged.
    pub fn load_module_with_resolver<R, S>(
        &self,
        realm: &RealmHandle,
        specifier: &str,
        resolve: R,
        source: S,
    ) -> Result<ModuleHandle, EmbedError>
    where
        R: Fn(Option<&str>, &crate::ast::ModuleRequest) -> Result<String, HostError> + 'static,
        S: Fn(&str) -> Result<String, HostError> + 'static,
    {
        self.require_realm(realm)?;
        let graph = ModuleGraph::with_resolver(
            realm.realm.clone(),
            move |referrer, request| {
                let referrer = referrer.map(JsString::to_string_lossy);
                catch_unwind(AssertUnwindSafe(|| resolve(referrer.as_deref(), request)))
                    .map_err(|_| RuntimeError::internal_error("module resolver panicked"))?
                    .map(JsString::from)
                    .map_err(HostError::into_runtime)
            },
            move |request| {
                let specifier = String::from_utf16_lossy(&request.specifier);
                catch_unwind(AssertUnwindSafe(|| source(&specifier)))
                    .map_err(|_| RuntimeError::internal_error("module source loader panicked"))?
                    .map_err(HostError::into_runtime)
            },
        );
        let module = graph.load(specifier).map_err(EmbedError::Host)?;
        Ok(ModuleHandle {
            engine_id: self.id,
            realm_id: realm.realm_id,
            module,
        })
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
        let script =
            ScriptRecord::parse(realm.realm.clone(), source).map_err(EmbedError::Compile)?;
        let bytecode = match self.backend {
            ScriptBackend::Ast => None,
            ScriptBackend::Bytecode => Some(
                BytecodeCompiler::compile_program(script.program())
                    .map_err(EmbedError::Bytecode)?,
            ),
            ScriptBackend::Auto => BytecodeCompiler::compile_program(script.program()).ok(),
        };
        Ok(CompiledScript {
            engine_id: self.id,
            realm_id: realm.realm_id,
            script,
            bytecode,
        })
    }

    /// Evaluates a compiled Script and returns an owned value or exception.
    pub fn evaluate(&self, script: &CompiledScript) -> Result<ValueHandle, Exception> {
        if script.engine_id != self.id {
            return Err(Exception::engine(
                "compiled script belongs to another engine",
            ));
        }
        match match &script.bytecode {
            Some(bytecode) => VirtualMachine::execute_in_realm(bytecode, &script.script.realm()),
            None => script.script.evaluate(),
        }
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
            if let Value::Object(object) = &value.value {
                realm
                    .realm
                    .global_object()
                    .check_allocation_context(object)
                    .map_err(EmbedError::Host)?;
            }
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
    bytecode: Option<VerifiedBytecode>,
}

impl CompiledScript {
    /// Returns the execution backend chosen for this Script.
    #[must_use]
    pub fn backend(&self) -> ScriptBackend {
        if self.bytecode.is_some() {
            ScriptBackend::Bytecode
        } else {
            ScriptBackend::Ast
        }
    }
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
    /// The bytecode compiler rejected unsupported or invalid Script syntax.
    Bytecode(CompileError),
    /// A host object, module, or Realm operation failed.
    Host(RuntimeError),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineMismatch => formatter.write_str("handle belongs to another engine"),
            Self::Compile(error) => write!(formatter, "compile failed: {error}"),
            Self::Bytecode(error) => write!(formatter, "bytecode compile failed: {error}"),
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

#[cfg(test)]
mod managed_allocation_tests {
    use super::*;
    #[test]
    fn public_engine_evaluation_uses_the_owned_heap_in_the_migration_context() {
        let engine = Engine {
            managed_allocation: true,
            ..Engine::default()
        };
        let realm = engine.create_realm();
        let baseline = engine.runtime.diagnostics().live;
        assert!(
            baseline > 0,
            "managed Realm intrinsics must be allocated in its Runtime"
        );
        let script = engine
            .compile(&realm, "var value={answer:42};value.self=value;value;")
            .unwrap();
        let value = engine.evaluate(&script).unwrap();
        assert!(engine.runtime.diagnostics().live > baseline);
        let Value::Object(object) = value.value() else {
            panic!("object result")
        };
        assert!(
            object
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert!(
            object
                .get(&"self".into())
                .unwrap()
                .same_value(value.value())
        );
    }
    #[test]
    fn managed_host_boundaries_reject_foreign_objects_without_repeating_effects() {
        use std::cell::Cell;
        let first = Engine {
            managed_allocation: true,
            ..Engine::default()
        };
        let second = Engine {
            managed_allocation: true,
            ..Engine::default()
        };
        let realm = first.create_realm();
        let other_realm = second.create_realm();
        let foreign = second
            .create_object(&other_realm, std::iter::empty::<(&str, ValueHandle)>())
            .unwrap();
        assert!(first.value(&realm, foreign.value().clone()).is_err());
        let calls = Rc::new(Cell::new(0));
        let called = calls.clone();
        let raw = foreign.value().clone();
        first
            .register_global_function(&realm, "bad", move |context, _, _| {
                called.set(called.get() + 1);
                Ok(context.value(raw.clone()))
            })
            .unwrap();
        let callback = realm.realm.global_object().get(&"bad".into()).unwrap();
        let Value::Object(callback) = callback else {
            panic!("callable")
        };
        assert!(
            callback
                .call(Value::Undefined, &[foreign.value().clone()])
                .is_err()
        );
        assert_eq!(
            calls.get(),
            0,
            "foreign arguments must be rejected before the callback"
        );
        let script = first.compile(&realm, "bad();").unwrap();
        assert_eq!(
            first.evaluate(&script).unwrap_err().kind(),
            ExceptionKind::TypeError
        );
        assert_eq!(
            calls.get(),
            1,
            "a failing native result must not cause a retry"
        );
        let caught = first
            .compile(&realm, "try {bad();} catch(error){error.name;}")
            .unwrap();
        assert!(
            first
                .evaluate(&caught)
                .unwrap()
                .value()
                .same_value(&Value::String("TypeError".into()))
        );
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn retiring_a_native_capture_releases_the_heap_borrow_before_host_drop_reentry() {
        use std::cell::Cell;
        struct Reenter {
            engine: Rc<Engine>,
            realm: RealmHandle,
            observed: Rc<Cell<bool>>,
        }
        impl Reenter {
            fn keep_alive(&self) {}
        }
        impl Drop for Reenter {
            fn drop(&mut self) {
                assert!(
                    self.engine.runtime.collect_migration_graph().is_err(),
                    "retirement must reject recursive collection without holding the Heap borrow"
                );
                let script = self
                    .engine
                    .compile(&self.realm, "({answer:42}).answer;")
                    .unwrap();
                assert!(
                    self.engine
                        .evaluate(&script)
                        .unwrap()
                        .value()
                        .same_value(&Value::Number(42.0))
                );
                self.observed.set(true);
            }
        }
        let base = Engine {
            managed_allocation: true,
            ..Engine::default()
        };
        let engine = Rc::new(base);
        let realm = engine.create_realm();
        let observed = Rc::new(Cell::new(false));
        let state = Reenter {
            engine: engine.clone(),
            realm: realm.clone(),
            observed: observed.clone(),
        };
        engine
            .register_global_function(&realm, "retire", move |context, _, _| {
                state.keep_alive();
                Ok(context.value(Value::Undefined))
            })
            .unwrap();
        let Value::Object(function) = realm.realm.global_object().get(&"retire".into()).unwrap()
        else {
            panic!("function")
        };
        let weak = function.downgrade();
        drop(function);
        let clear = engine.compile(&realm, "delete retire;").unwrap();
        engine.evaluate(&clear).unwrap();
        drop(clear);
        engine.runtime.collect_migration_graph().unwrap();
        assert!(
            observed.get(),
            "host Drop must run after retirement without a Heap borrow"
        );
        assert!(
            weak.upgrade().is_none(),
            "a reused slot must not resurrect its old function handle"
        );
    }

    #[test]
    fn a_partial_array_literal_keeps_its_first_object_alive_during_host_reentry_and_collection() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
            let mut base = Engine::builder().script_backend(backend).build();
            base.managed_allocation = true;
            let engine = Rc::new(base);
            let realm = engine.create_realm();
            let token = Rc::new(());
            let weak = Rc::downgrade(&token);
            engine
                .register_global_function(&realm, "marker", move |context, _, _| {
                    let _ = Rc::strong_count(&token);
                    Ok(context.value(Value::Number(42.0)))
                })
                .unwrap();
            let reentrant = engine.clone();
            let callback_realm = realm.clone();
            engine
                .register_global_function(&realm, "trigger", move |context, _, _| {
                    let nested = reentrant
                        .compile(&callback_realm, "delete marker;40+2;")
                        .unwrap();
                    assert!(
                        reentrant
                            .evaluate(&nested)
                            .unwrap()
                            .value()
                            .same_value(&Value::Number(42.0))
                    );
                    drop(nested);
                    reentrant.runtime.collect_migration_graph().unwrap();
                    Ok(context.value(Value::Number(0.0)))
                })
                .unwrap();
            let script = engine
                .compile(&realm, "var result=[{probe:marker},trigger()][0];result;")
                .unwrap();
            assert_eq!(script.backend(), backend);
            let value = engine.evaluate(&script).unwrap();
            drop(script);
            let Value::Object(object) = value.value() else {
                panic!("object")
            };
            let Value::Object(callback) = object.get(&"probe".into()).unwrap() else {
                panic!("marker retained by partial literal")
            };
            assert!(
                callback
                    .call(Value::Undefined, &[])
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
            drop(callback);
            assert!(weak.upgrade().is_some());
            let clear = engine
                .compile(&realm, "result=undefined;delete trigger;")
                .unwrap();
            engine.evaluate(&clear).unwrap();
            drop(clear);
            drop(value);
            engine.runtime.collect_migration_graph().unwrap();
            assert!(
                weak.upgrade().is_none(),
                "partial-literal roots must end with evaluation/returned-handle lifetime"
            );
        }
    }

    #[test]
    fn migrated_function_allocations_execute_through_the_public_ast_and_vm_apis() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
            let mut engine = Engine::builder().script_backend(backend).build();
            engine.managed_allocation = true;
            let realm = engine.create_realm();
            for source in [
                "function sum(a,b){return a+b;}sum(20,22);",
                "var constructedArray=new Array({answer:42});constructedArray[0].answer;",
                "var calledArray=Array({answer:42});calledArray[0].answer;",
                "var arrayIter=[{answer:42}].values();arrayIter.next().value.answer;",
                "var proxyValue=new Proxy({answer:42},{});proxyValue.answer;",
                "var revocable=Proxy.revocable({answer:42},{});var beforeRevoke=revocable.proxy.answer;revocable.revoke();revocable.revoke();beforeRevoke;",
                "var proxyCall=new Proxy(function(x){return x+2;},{});proxyCall(40);",
                "function ProxyBox(x){this.answer=x;}var proxyCtor=new Proxy(ProxyBox,{});(new proxyCtor(42)).answer;",
                "var proxyTrap=new Proxy(function(){return 0;},{apply:function(target,receiver,args){return args[0]+2;}});proxyTrap(40);",
                "var stringIter='x'[Symbol.iterator]();if(stringIter.next().value==='x' && stringIter.next().done){42;}else{0;}",
                "var names=Object.getOwnPropertyNames({answer:42});if(names[0]==='answer'){42;}else{0;}",
                "var twice=(x)=>x*2;twice(21);",
                "function Box(x){this.value=x;}var box=new Box(42);box.value;",
                "function outer(x){return function(y){return x+y;};}var add=outer(20);add(22);",
                "function boundSum(a,b){return this.base+a+b;}var sumBound=boundSum.bind({base:10},20);sumBound(12);",
                "function BoundBox(x){this.value=x;}var Bound=BoundBox.bind(null,42);var boundBox=new Bound();if(boundBox instanceof Bound && boundBox instanceof BoundBox){boundBox.value;}else{0;}",
            ] {
                let script = engine.compile(&realm, source).unwrap();
                assert_eq!(script.backend(), backend);
                assert!(
                    engine
                        .evaluate(&script)
                        .unwrap()
                        .value()
                        .same_value(&Value::Number(42.0)),
                    "{backend:?} {source}"
                );
            }
            let revoked_access = engine.compile(&realm, "revocable.proxy.answer;").unwrap();
            assert_eq!(
                engine.evaluate(&revoked_access).unwrap_err().kind(),
                ExceptionKind::TypeError
            );
            let construct_revoke = engine.compile(&realm, "new revocable.revoke();").unwrap();
            assert_eq!(
                engine.evaluate(&construct_revoke).unwrap_err().kind(),
                ExceptionKind::TypeError
            );
        }
    }

    #[test]
    fn managed_ast_function_captures_preserve_then_release_environment_cycles() {
        for source in [
            "var escaped=(function make(){var payload={probe:marker};function read(){return payload.probe();}payload.back=read;return read;})();delete marker;escaped;",
            "var escaped=(()=>{let payload={probe:marker};let read=()=>payload.probe();payload.back=read;return read;})();delete marker;escaped;",
        ] {
            let mut engine = Engine::builder().script_backend(ScriptBackend::Ast).build();
            engine.managed_allocation = true;
            let realm = engine.create_realm();
            let token = Rc::new(());
            let weak = Rc::downgrade(&token);
            engine
                .register_global_function(&realm, "marker", move |context, _, _| {
                    let _ = Rc::strong_count(&token);
                    Ok(context.value(Value::Number(42.0)))
                })
                .unwrap();
            let script = engine.compile(&realm, source).unwrap();
            let value = engine.evaluate(&script).unwrap();
            let clone = value.clone();
            drop(script);
            let clear = engine.compile(&realm, "escaped=undefined;").unwrap();
            engine.evaluate(&clear).unwrap();
            drop(clear);
            drop(value);
            engine.runtime.collect_migration_graph().unwrap();
            assert!(weak.upgrade().is_some());
            {
                let Value::Object(function) = clone.value() else {
                    panic!("escaped function");
                };
                assert!(
                    function
                        .call(Value::Undefined, &[])
                        .unwrap()
                        .same_value(&Value::Number(42.0))
                );
            }
            drop(clone);
            engine.runtime.collect_migration_graph().unwrap();
            assert!(
                weak.upgrade().is_none(),
                "unrooted closure/environment cycle: {source}"
            );
        }
    }

    #[test]
    fn public_handle_clones_preserve_a_js_cycle_then_allow_reclamation_at_a_test_safepoint() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
            for source in [
                "var value={probe:marker};value.self=value;delete marker;value;",
                "var value={probe:marker};value.other={owner:value};delete marker;value;",
                "var value=[marker];value.probe=marker;value[1]=value;delete marker;value;",
                "var value=[marker,,null];value.probe=marker;value[3]=value;delete marker;value;",
                "var value=[];value.probe=marker;value[1000000]=value;delete marker;value;",
            ] {
                let mut engine = Engine::builder().script_backend(backend).build();
                engine.managed_allocation = true;
                let realm = engine.create_realm();
                let token = Rc::new(());
                let weak = Rc::downgrade(&token);
                engine
                    .register_global_function(&realm, "marker", move |context, _, _| {
                        let _ = Rc::strong_count(&token);
                        Ok(context.value(Value::Number(42.0)))
                    })
                    .unwrap();
                let script = engine.compile(&realm, source).unwrap();
                assert_eq!(script.backend(), backend);
                let value = engine.evaluate(&script).unwrap();
                let cloned = value.clone();
                drop(script);
                let clear = engine.compile(&realm, "value=undefined;").unwrap();
                engine.evaluate(&clear).unwrap();
                drop(clear);
                engine.runtime.collect_migration_graph().unwrap();
                assert!(weak.upgrade().is_some(), "{backend:?} {source}");
                drop(value);
                engine.runtime.collect_migration_graph().unwrap();
                assert!(weak.upgrade().is_some());
                let Value::Object(object) = cloned.value() else {
                    panic!("object result")
                };
                let Value::Object(callback) = object.get(&"probe".into()).unwrap() else {
                    panic!("callback")
                };
                assert!(
                    callback
                        .call(Value::Undefined, &[])
                        .unwrap()
                        .same_value(&Value::Number(42.0))
                );
                drop(callback);
                drop(cloned);
                engine.runtime.collect_migration_graph().unwrap();
                assert!(
                    weak.upgrade().is_none(),
                    "unrooted JS graph should release its marker: {backend:?} {source}"
                );
            }
        }
    }
}
