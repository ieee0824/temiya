//! Realm ownership and execution-context stack management.

use super::{
    Runtime,
    environment::StoredEnvironment,
    stored::{StoredObject, StoredOwner, StoredValue},
};
use crate::gc::{Handle, Root, Trace, Tracer};
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    cell::{OnceCell, RefCell},
    ops::Deref,
};

use crate::ast::Program;

use super::{
    Environment, HostHooks, IntrinsicId, IntrinsicRegistry, JobQueue, Object, RuntimeError,
    RuntimeResult, Value, WellKnownSymbol, WellKnownSymbols,
};

/// An ECMAScript Realm with its global object and Global Environment Record.
#[derive(Clone)]
pub struct Realm(pub(super) RealmReference);

#[derive(Clone)]
pub(super) enum RealmReference {
    Legacy(Rc<RealmData>),
    Managed(Rc<RealmRoot>),
}
impl Deref for RealmReference {
    type Target = RealmData;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(data) => data,
            Self::Managed(root) => &root.data,
        }
    }
}
pub(super) struct RealmRoot {
    pub(super) root: Root<ManagedRealm>,
    pub(super) data: Rc<RealmData>,
    pub(super) runtime: Runtime,
}
pub(super) struct ManagedRealm {
    pub(super) data: Rc<RealmData>,
}

#[derive(Clone)]
pub(super) enum StoredRealm {
    Legacy(Realm),
    Managed(Handle<ManagedRealm>),
}
impl StoredRealm {
    pub(super) fn initial(runtime: Option<&Runtime>, realm: &Realm) -> RuntimeResult<Self> {
        match (runtime, realm.runtime()) {
            (None, None) => Ok(Self::Legacy(realm.clone())),
            (Some(left), Some(right)) if left.same_heap(right) => {
                Ok(Self::Managed(realm.handle().expect("managed Realm")))
            }
            _ => Err(RuntimeError::type_error(
                "Realm allocation context mismatch",
            )),
        }
    }
    pub(super) fn record_owner(&self, owner: &Object) -> RuntimeResult<()> {
        match (self, owner.0.runtime()) {
            (Self::Managed(handle), Some(runtime)) => {
                runtime.record_object_edge(owner.0.handle().expect("managed owner"), *handle)
            }
            (Self::Legacy(_), None) => Ok(()),
            _ => Err(RuntimeError::type_error(
                "Realm allocation context mismatch",
            )),
        }
    }
    pub(super) fn store(owner: &impl StoredOwner, realm: &Realm) -> RuntimeResult<Self> {
        match (owner.runtime(), realm.runtime()) {
            (None, None) => Ok(Self::Legacy(realm.clone())),
            (Some(left), Some(right)) if left.same_heap(right) => {
                let target = realm.handle().expect("managed Realm");
                left.record_object_edge(owner.handle().expect("managed owner"), target)?;
                Ok(Self::Managed(target))
            }
            _ => Err(RuntimeError::type_error(
                "Realm belongs to a different allocation context",
            )),
        }
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<Realm> {
        match (self, owner.runtime()) {
            (Self::Legacy(realm), None) => Ok(realm.clone()),
            (Self::Managed(handle), Some(runtime)) => runtime
                .root_realm(*handle)
                .ok_or_else(|| RuntimeError::internal_error("stale managed Realm edge")),
            _ => Err(RuntimeError::internal_error(
                "Realm allocation context mismatch",
            )),
        }
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Self::Managed(handle) = self {
            tracer.mark(*handle);
        }
    }
}
impl Trace for ManagedRealm {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.data.trace(tracer);
    }
}
impl StoredOwner for Realm {
    type Allocation = ManagedRealm;
    fn runtime(&self) -> Option<&Runtime> {
        match &self.0 {
            RealmReference::Legacy(_) => None,
            RealmReference::Managed(root) => Some(&root.runtime),
        }
    }
    fn handle(&self) -> Option<Handle<ManagedRealm>> {
        match &self.0 {
            RealmReference::Legacy(_) => None,
            RealmReference::Managed(root) => Some(root.root.handle()),
        }
    }
}

pub(super) struct RealmData {
    pub(super) facade: RefCell<Weak<RealmRoot>>,
    id: u64,
    global_object: StoredObject,
    global_environment: StoredEnvironment,
    // The Global Environment's chosen this value is immutable. Keep the same
    // traced value here so sloppy calls need not revive an Environment facade.
    global_this_value: StoredValue,
    job_queue: super::job_queue::StoredJobQueue,
    intrinsics: OnceCell<super::intrinsics::StoredIntrinsicRegistry>,
    well_known_symbols: WellKnownSymbols,
    host_hooks: HostHooks,
}

impl Trace for RealmData {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.global_object.trace(tracer);
        self.global_environment.trace(tracer);
        self.global_this_value.trace(tracer);
        self.job_queue.trace(tracer);
        if let Some(intrinsics) = self.intrinsics.get() {
            intrinsics.trace(tracer);
        }
    }
}

impl Realm {
    /// Materializes a catchable exception without invoking a potentially
    /// replaced global constructor. Internal failures remain Rust errors.
    pub(crate) fn exception_value(&self, error: &RuntimeError) -> RuntimeResult<Value> {
        use super::{PropertyDescriptor, RuntimeErrorKind};
        if let Some(value) = error.thrown_value() {
            return Ok(value.clone());
        }
        let prototype = match error.kind() {
            RuntimeErrorKind::ThrownValue => unreachable!("throw payload"),
            RuntimeErrorKind::TypeError => IntrinsicId::TypeErrorPrototype,
            RuntimeErrorKind::SyntaxError => IntrinsicId::SyntaxErrorPrototype,
            RuntimeErrorKind::ReferenceError => IntrinsicId::ReferenceErrorPrototype,
            RuntimeErrorKind::RangeError => IntrinsicId::RangeErrorPrototype,
            RuntimeErrorKind::InternalError => return Err(error.clone()),
        };
        let object = Object::new_for(&self.global_object());
        object.set_prototype(Some(self.intrinsic(prototype)));
        object.define_own_property(
            "message".into(),
            PropertyDescriptor::data(Value::String(error.message().into()), true, false, true),
        )?;
        Ok(Value::Object(object))
    }

    /// Retains the originating Realm before a function crosses a caller boundary.
    /// Success paths and already-materialized exceptions allocate nothing here.
    pub(crate) fn bind_exception(&self, mut error: RuntimeError) -> RuntimeError {
        if error.thrown.is_none() && error.kind() != super::RuntimeErrorKind::InternalError {
            match self.exception_value(&error) {
                Ok(value) => error.thrown = Some(Box::new(value)),
                Err(failure) => return failure,
            }
        }
        error
    }

    /// Creates a Realm around a global object and its chosen global-this value.
    #[must_use]
    pub fn new(global_object: Object, global_this_value: Value) -> Self {
        Self::try_new(global_object, global_this_value)
            .expect("a Realm requires an extensible global object")
    }

    /// Tries to create and initialize a Realm around a host-supplied global object.
    pub fn try_new(global_object: Object, global_this_value: Value) -> RuntimeResult<Self> {
        Self::try_new_with_host(global_object, global_this_value, HostHooks::default())
    }

    /// Tries to create a Realm with explicitly supplied host-dependent services.
    pub fn try_new_with_host(
        global_object: Object,
        global_this_value: Value,
        host_hooks: HostHooks,
    ) -> RuntimeResult<Self> {
        if let Value::Object(value) = &global_this_value {
            StoredObject::check_context(&global_object, value)?;
        }
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let well_known_symbols = WellKnownSymbols::initialize();
        let runtime = global_object.0.runtime().cloned();
        let stored_global_this = StoredValue::initial(runtime.as_ref(), &global_this_value)?;
        let global_environment = Environment::global(global_object.clone(), global_this_value);
        let queue = runtime
            .as_ref()
            .map_or_else(JobQueue::new, JobQueue::new_in);
        let data = RealmData {
            facade: RefCell::new(Weak::new()),
            id: next_id(&NEXT_ID, "Realm"),
            global_object: StoredObject::initial(runtime.as_ref(), &global_object)?,
            global_environment: StoredEnvironment::initial(runtime.as_ref(), &global_environment)?,
            global_this_value: stored_global_this,
            job_queue: super::job_queue::StoredJobQueue::initial(runtime.as_ref(), &queue)?,
            intrinsics: OnceCell::new(),
            well_known_symbols,
            host_hooks,
        };
        let realm = match runtime {
            Some(runtime) => runtime.allocate_realm(data),
            None => Self(RealmReference::Legacy(Rc::new(data))),
        };
        let intrinsics = IntrinsicRegistry::initialize(
            &global_object,
            realm.0.host_hooks.clone(),
            &realm.0.well_known_symbols,
            &realm,
        )?;
        // Publish a complete table before a host global Proxy can reenter from
        // DefineProperty traps while the global names are installed.
        realm
            .0
            .intrinsics
            .set(super::intrinsics::StoredIntrinsicRegistry::store(
                &realm,
                &intrinsics,
            )?)
            .map_err(|_| RuntimeError::internal_error("Realm intrinsics already initialized"))?;
        intrinsics.install_globals(&global_object)?;
        Ok(realm)
    }

    /// Creates a Realm whose global-this value is its fresh global object.
    #[must_use]
    pub fn ordinary() -> Self {
        let global_object = Object::new();
        Self::new(global_object.clone(), Value::Object(global_object))
    }

    /// Creates a Realm in an explicit shared allocation context.
    pub fn ordinary_in(runtime: &super::AllocationContext) -> Self {
        let global_object = Object::new_in(runtime);
        Self::new(global_object.clone(), Value::Object(global_object))
    }

    /// Returns an engine-local identity for this Realm.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.0.id
    }

    /// Returns the Realm's global object.
    #[must_use]
    pub fn global_object(&self) -> Object {
        self.0
            .global_object
            .load(self)
            .expect("live Realm global object")
    }

    /// Returns the Realm's Global Environment Record.
    #[must_use]
    pub fn global_environment(&self) -> Environment {
        self.0
            .global_environment
            .load(self)
            .expect("live Realm global environment")
    }

    pub(crate) fn global_this_value(&self) -> RuntimeResult<Value> {
        self.0.global_this_value.load(self)
    }

    /// Returns the Realm's Promise Job Queue.
    #[must_use]
    pub fn job_queue(&self) -> JobQueue {
        self.0.job_queue.load(self).expect("live Realm Job Queue")
    }

    /// Returns an owning snapshot of this Realm's intrinsic registry.
    /// The snapshot and its clones may outlive this Realm.
    #[must_use]
    pub fn intrinsics(&self) -> IntrinsicRegistry {
        self.0
            .intrinsics
            .get()
            .expect("initialized Realm")
            .load(self)
    }

    /// Creates a non-constructable native function with this Realm's Function.prototype.
    /// The function retains its source Realm through a traced internal edge.
    /// Low-level Object::native_function remains Realm-independent.
    pub fn create_native_function(
        &self,
        name: &str,
        length: u32,
        callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
    ) -> RuntimeResult<Object> {
        super::native_function::create(
            &self.intrinsic(IntrinsicId::FunctionPrototype),
            name,
            length,
            callback,
        )
    }

    /// Returns one intrinsic object owned by this Realm.
    #[must_use]
    pub fn intrinsic(&self, id: IntrinsicId) -> Object {
        self.0
            .intrinsics
            .get()
            .expect("initialized Realm")
            .get(self, id)
    }

    /// Returns one of this Realm's well-known symbols.
    #[must_use]
    pub fn well_known_symbol(&self, id: WellKnownSymbol) -> super::Symbol {
        self.0.well_known_symbols.get(id)
    }

    /// Returns this Realm's host-service boundary.
    #[must_use]
    pub fn host_hooks(&self) -> &HostHooks {
        &self.0.host_hooks
    }
}

impl PartialEq for Realm {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(&*self.0, &*other.0)
    }
}

impl Eq for Realm {}

impl fmt::Debug for Realm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Realm")
            .field("identity", &self.identity())
            .finish_non_exhaustive()
    }
}

/// A parsed Script Record associated with the Realm in which it will execute.
#[derive(Clone, Debug)]
pub struct ScriptRecord(Rc<ScriptData>);

#[derive(Debug)]
struct ScriptData {
    id: u64,
    realm: Realm,
    program: Program,
}

impl ScriptRecord {
    /// Creates a Script Record from an owned parsed program.
    #[must_use]
    pub fn new(realm: Realm, program: Program) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(Rc::new(ScriptData {
            id: next_id(&NEXT_ID, "Script Record"),
            realm,
            program,
        }))
    }

    /// Returns an engine-local identity for this Script Record.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.0.id
    }

    /// Returns the Script Record's Realm.
    #[must_use]
    pub fn realm(&self) -> Realm {
        self.0.realm.clone()
    }

    /// Returns the parsed ECMAScript code.
    #[must_use]
    pub fn program(&self) -> &Program {
        &self.0.program
    }
}

impl PartialEq for ScriptRecord {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ScriptRecord {}

/// An execution context's lifecycle while owned by an [`Agent`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextState {
    /// The context is the currently executing context.
    Running,
    /// Execution is paused until the context is resumed.
    Suspended,
}

/// The Realm, code, and environment ownership for one execution context.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    id: u64,
    realm: Realm,
    script: Option<ScriptRecord>,
    function: Option<Object>,
    lexical_environment: Environment,
    variable_environment: Environment,
}

impl ExecutionContext {
    /// Creates a host or function execution context.
    #[must_use]
    pub fn new(
        realm: Realm,
        function: Option<Object>,
        lexical_environment: Environment,
        variable_environment: Environment,
    ) -> Self {
        Self::create(
            realm,
            None,
            function,
            lexical_environment,
            variable_environment,
        )
    }

    /// Creates a Script execution context using the Realm's global environment.
    #[must_use]
    pub fn for_script(script: ScriptRecord) -> Self {
        let realm = script.realm();
        let environment = realm.global_environment();
        Self::create(realm, Some(script), None, environment.clone(), environment)
    }

    fn create(
        realm: Realm,
        script: Option<ScriptRecord>,
        function: Option<Object>,
        lexical_environment: Environment,
        variable_environment: Environment,
    ) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: next_id(&NEXT_ID, "Execution Context"),
            realm,
            script,
            function,
            lexical_environment,
            variable_environment,
        }
    }

    /// Returns an engine-local identity for this context.
    #[must_use]
    pub fn identity(&self) -> u64 {
        self.id
    }

    /// Returns the context's Realm.
    #[must_use]
    pub fn realm(&self) -> &Realm {
        &self.realm
    }

    /// Returns the active Script Record, if this is a script context.
    #[must_use]
    pub fn script(&self) -> Option<&ScriptRecord> {
        self.script.as_ref()
    }

    /// Returns the active function object, if this is a function context.
    #[must_use]
    pub fn function(&self) -> Option<&Object> {
        self.function.as_ref()
    }

    /// Returns the context's current lexical environment.
    #[must_use]
    pub fn lexical_environment(&self) -> &Environment {
        &self.lexical_environment
    }

    /// Replaces the context's current lexical environment.
    pub fn set_lexical_environment(&mut self, environment: Environment) {
        self.lexical_environment = environment;
    }

    /// Returns the context's variable environment.
    #[must_use]
    pub fn variable_environment(&self) -> &Environment {
        &self.variable_environment
    }

    /// Replaces the context's variable environment.
    pub fn set_variable_environment(&mut self, environment: Environment) {
        self.variable_environment = environment;
    }
}

struct ContextEntry {
    context: ExecutionContext,
    state: ContextState,
}

/// The single-thread ECMAScript agent state and execution-context stack.
#[derive(Default)]
pub struct Agent {
    stack: Vec<ContextEntry>,
    protected_depth: usize,
}

impl Agent {
    /// Creates an agent with an empty execution-context stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of contexts currently retained by the agent.
    #[must_use]
    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }

    /// Returns the currently executing or suspended top context.
    #[must_use]
    pub fn current_context(&self) -> Option<&ExecutionContext> {
        self.stack.last().map(|entry| &entry.context)
    }

    /// Returns a mutable reference to the top execution context.
    pub fn current_context_mut(&mut self) -> Option<&mut ExecutionContext> {
        self.stack.last_mut().map(|entry| &mut entry.context)
    }

    /// Returns the lifecycle state of the top execution context.
    #[must_use]
    pub fn current_state(&self) -> Option<ContextState> {
        self.stack.last().map(|entry| entry.state)
    }

    /// Enters a context, suspending a running caller before pushing it.
    pub fn enter(&mut self, context: ExecutionContext) -> RuntimeResult<()> {
        if let Some(current) = self.stack.last_mut() {
            if current.state == ContextState::Running {
                current.state = ContextState::Suspended;
            }
        }
        self.stack.push(ContextEntry {
            context,
            state: ContextState::Running,
        });
        Ok(())
    }

    /// Suspends the currently running context.
    pub fn suspend(&mut self) -> RuntimeResult<()> {
        let current = self
            .stack
            .last_mut()
            .ok_or_else(|| RuntimeError::internal_error("no context to suspend"))?;
        if current.state != ContextState::Running {
            return Err(RuntimeError::internal_error(
                "current context is already suspended",
            ));
        }
        current.state = ContextState::Suspended;
        Ok(())
    }

    /// Resumes the currently suspended context.
    pub fn resume(&mut self) -> RuntimeResult<()> {
        let current = self
            .stack
            .last_mut()
            .ok_or_else(|| RuntimeError::internal_error("no context to resume"))?;
        if current.state != ContextState::Suspended {
            return Err(RuntimeError::internal_error(
                "current context is already running",
            ));
        }
        current.state = ContextState::Running;
        Ok(())
    }

    /// Leaves the top context and resumes its suspended caller, if any.
    pub fn leave(&mut self) -> RuntimeResult<ExecutionContext> {
        if self.stack.len() <= self.protected_depth {
            return Err(RuntimeError::internal_error(
                "cannot leave a protected caller execution context",
            ));
        }
        let entry = self
            .stack
            .pop()
            .ok_or_else(|| RuntimeError::internal_error("no context to leave"))?;
        if let Some(caller) = self.stack.last_mut() {
            if caller.state != ContextState::Suspended {
                self.stack.push(entry);
                return Err(RuntimeError::internal_error(
                    "caller context was not suspended",
                ));
            }
            caller.state = ContextState::Running;
        }
        Ok(entry.context)
    }

    /// Runs an operation in a context and restores the stack after normal or abrupt completion.
    pub fn run_in_context<T>(
        &mut self,
        context: ExecutionContext,
        operation: impl FnOnce(&mut Self) -> RuntimeResult<T>,
    ) -> RuntimeResult<T> {
        let original_depth = self.stack.len();
        let original_state = self.current_state();
        self.enter(context)?;
        let previous_protected_depth = self.protected_depth;
        self.protected_depth = self.protected_depth.max(original_depth);
        let result = operation(self);
        let cleanup_result = self.restore_depth(original_depth, original_state);
        self.protected_depth = previous_protected_depth;
        match (result, cleanup_result) {
            (Err(error), _) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn restore_depth(
        &mut self,
        depth: usize,
        original_state: Option<ContextState>,
    ) -> RuntimeResult<()> {
        if self.stack.len() < depth {
            return Err(RuntimeError::internal_error(
                "operation removed a caller execution context",
            ));
        }
        while self.stack.len() > depth {
            self.leave()?;
        }
        if let (Some(current), Some(state)) = (self.stack.last_mut(), original_state) {
            current.state = state;
        }
        Ok(())
    }
}

impl fmt::Debug for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Agent")
            .field("stack_depth", &self.stack_depth())
            .field("current_state", &self.current_state())
            .finish()
    }
}

fn next_id(counter: &AtomicU64, kind: &str) -> u64 {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| panic!("exhausted {kind} identities"));
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return current,
            Err(observed) => current = observed,
        }
    }
}
