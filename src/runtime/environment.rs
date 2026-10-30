//! Lexical environments and ECMAScript environment records.

use super::{
    Runtime,
    stored::{StoredObject, StoredOwner, StoredValue},
};
use crate::gc::{Handle, Root, Trace, Tracer};
use std::cell::RefCell;
use std::fmt;
use std::ops::Deref;
use std::rc::{Rc, Weak};

use super::{
    JsString, Object, PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult, Value,
};

/// The concrete environment-record category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnvironmentKind {
    /// A declarative environment record.
    Declarative,
    /// An object environment record, including `with` environments.
    Object,
    /// A global environment record combining object and declarative records.
    Global,
    /// A function environment record.
    Function,
    /// A module environment record.
    Module,
}

/// Configuration for an object environment record.
#[derive(Clone, Debug, Default)]
pub struct ObjectEnvironmentOptions {
    /// Whether the record was created for a `with` statement.
    pub with_environment: bool,
    /// The Realm's `Symbol.unscopables` key when `with_environment` is true.
    pub unscopables_key: Option<PropertyKey>,
}

/// The state of a function environment's `this` binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThisBindingStatus {
    /// An arrow function has lexical `this` and no local binding.
    Lexical,
    /// A derived constructor has not initialized `this` yet.
    Uninitialized,
    /// The local `this` binding has been initialized.
    Initialized,
}

pub(crate) enum FunctionThis {
    Lexical,
    Uninitialized,
    Initialized(Value),
}

/// An identity-bearing lexical environment with an optional outer environment.
#[derive(Clone)]
pub struct Environment(pub(super) EnvironmentReference);

#[derive(Clone)]
pub(super) struct EnvironmentReference(pub(super) Rc<EnvironmentRoot>);
impl Deref for EnvironmentReference {
    type Target = EnvironmentData;
    fn deref(&self) -> &Self::Target {
        &self.0.data
    }
}
pub(super) struct EnvironmentRoot {
    pub(super) root: Root<ManagedEnvironment>,
    pub(super) data: Rc<EnvironmentData>,
    pub(super) runtime: Runtime,
}
pub(super) type ManagedEnvironment = EnvironmentData;

#[derive(Clone)]
pub(super) struct StoredEnvironment(pub(super) Handle<ManagedEnvironment>);
impl StoredEnvironment {
    pub(super) fn initial(runtime: Option<&Runtime>, value: &Environment) -> RuntimeResult<Self> {
        match (runtime, value.runtime()) {
            (Some(left), Some(right)) if left.same_heap(right) => {
                Ok(Self(value.handle().expect("managed environment")))
            }
            _ => Err(RuntimeError::type_error(
                "Environment allocation context mismatch",
            )),
        }
    }
    fn check_context(runtime: Option<&Runtime>, value: &Environment) -> RuntimeResult<()> {
        Self::initial(runtime, value).map(|_| ())
    }
    pub(super) fn store(owner: &impl StoredOwner, value: &Environment) -> RuntimeResult<Self> {
        let stored = Self::initial(owner.runtime(), value)?;
        owner
            .runtime()
            .expect("validated allocation context")
            .record_object_edge(owner.handle().expect("managed owner"), stored.0)?;
        Ok(stored)
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<Environment> {
        owner
            .runtime()
            .ok_or_else(|| RuntimeError::internal_error("Environment allocation context mismatch"))?
            .root_environment(self.0)
            .ok_or_else(|| RuntimeError::internal_error("stale environment edge"))
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        tracer.mark(self.0);
    }
}
impl StoredOwner for Environment {
    type Allocation = ManagedEnvironment;
    fn runtime(&self) -> Option<&Runtime> {
        Some(&self.0.0.runtime)
    }
    fn handle(&self) -> Option<Handle<ManagedEnvironment>> {
        Some(self.0.0.root.handle())
    }
}

pub(super) struct EnvironmentData {
    pub(super) facade: RefCell<Weak<EnvironmentRoot>>,
    outer: Option<StoredEnvironment>,
    record: RefCell<EnvironmentRecord>,
}

enum EnvironmentRecord {
    Declarative(DeclarativeRecord),
    Object(ObjectRecord),
    Global(GlobalRecord),
    Function(FunctionRecord),
    Module(ModuleRecord),
}

impl Trace for EnvironmentData {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        fn bindings(record: &DeclarativeRecord, tracer: &mut Tracer<'_>) {
            for binding in &record.bindings {
                if let Some(v) = &binding.value {
                    v.trace(tracer);
                }
            }
        }
        if let Some(outer) = &self.outer {
            outer.trace(tracer);
        }
        match &*self.record.borrow() {
            EnvironmentRecord::Declarative(record) => bindings(record, tracer),
            EnvironmentRecord::Object(record) => record.binding_object.trace(tracer),
            EnvironmentRecord::Global(record) => {
                record.object_record.binding_object.trace(tracer);
                bindings(&record.declarative_record, tracer);
                record.global_this_value.trace(tracer);
            }
            EnvironmentRecord::Function(record) => {
                bindings(&record.declarative_record, tracer);
                record.function_object.trace(tracer);
                if let Some(v) = &record.this_value {
                    v.trace(tracer);
                }
                if let Some(v) = &record.new_target {
                    v.trace(tracer);
                }
            }
            EnvironmentRecord::Module(record) => {
                bindings(&record.declarative_record, tracer);
                for import in &record.imports {
                    import.target_environment.trace(tracer);
                }
            }
        }
    }
}

#[derive(Default)]
struct DeclarativeRecord {
    bindings: Vec<Binding>,
}

struct Binding {
    name: JsString,
    value: Option<StoredValue>,
    mutable: bool,
    strict: bool,
    deletable: bool,
}

#[derive(Clone)]
struct ObjectRecord {
    binding_object: StoredObject,
    options: ObjectEnvironmentOptions,
}

struct GlobalRecord {
    object_record: ObjectRecord,
    declarative_record: DeclarativeRecord,
    global_this_value: StoredValue,
    var_declared_names: Vec<JsString>,
}

struct FunctionRecord {
    declarative_record: DeclarativeRecord,
    this_status: ThisBindingStatus,
    this_value: Option<StoredValue>,
    function_object: StoredObject,
    new_target: Option<StoredObject>,
}

#[derive(Default)]
struct ModuleRecord {
    declarative_record: DeclarativeRecord,
    imports: Vec<ImportBinding>,
}

#[derive(Clone)]
struct ImportBinding {
    local_name: JsString,
    target_environment: StoredEnvironment,
    target_name: JsString,
}

impl Environment {
    /// Guard-only read: never consult Object/global/module records or invoke JS.
    pub(crate) fn try_pure_number_binding(&self, name: &JsString) -> Option<f64> {
        let mut current = self.clone();
        loop {
            {
                let record = current.0.record.borrow();
                let bindings = match &*record {
                    EnvironmentRecord::Function(record) => &record.declarative_record.bindings,
                    EnvironmentRecord::Declarative(record) => &record.bindings,
                    _ => return None,
                };
                if let Some(binding) = bindings.iter().find(|binding| binding.name == *name) {
                    return match &binding.value {
                        Some(StoredValue::Number(number)) => Some(*number),
                        _ => None,
                    };
                }
            }
            current = current.outer()?;
        }
    }
    /// Borrows the shared context that owns this environment.
    pub fn allocation_context(&self) -> &super::AllocationContext {
        self.runtime().expect("environment allocation context")
    }
    pub(crate) fn update_number_binding(
        &self,
        name: &JsString,
        hint: Option<usize>,
        increment: bool,
        strict: bool,
    ) -> RuntimeResult<Option<(f64, f64)>> {
        let mut record = self.0.record.borrow_mut();
        let record = match &mut *record {
            EnvironmentRecord::Function(record) => &mut record.declarative_record,
            EnvironmentRecord::Declarative(record) => record,
            _ => return Ok(None),
        };
        let Some(index) = hint
            .filter(|index| {
                record
                    .bindings
                    .get(*index)
                    .is_some_and(|binding| binding.name == *name)
            })
            .or_else(|| {
                record
                    .bindings
                    .iter()
                    .position(|binding| binding.name == *name)
            })
        else {
            return Ok(None);
        };
        let binding = &mut record.bindings[index];
        let Some(StoredValue::Number(old)) = &binding.value else {
            return Ok(None);
        };
        let old = *old;
        let new = if increment { old + 1.0 } else { old - 1.0 };
        binding.write(self, Value::Number(new), strict)?;
        Ok(Some((old, new)))
    }

    pub(crate) fn append_string_binding(
        &self,
        name: &JsString,
        hint: Option<usize>,
        suffix: &JsString,
    ) -> Option<Value> {
        let mut record = self.0.record.borrow_mut();
        let record = match &mut *record {
            EnvironmentRecord::Function(record) => &mut record.declarative_record,
            EnvironmentRecord::Declarative(record) => record,
            _ => return None,
        };
        let index = hint
            .filter(|index| {
                record
                    .bindings
                    .get(*index)
                    .is_some_and(|binding| binding.name == *name)
            })
            .or_else(|| {
                record
                    .bindings
                    .iter()
                    .position(|binding| binding.name == *name)
            })?;
        let binding = &mut record.bindings[index];
        if !binding.mutable {
            return None;
        }
        let Some(StoredValue::String(string)) = &mut binding.value else {
            return None;
        };
        string.append_owned(suffix);
        Some(Value::String(string.clone()))
    }

    pub(crate) fn function_binding_index(&self, name: &JsString) -> Option<usize> {
        let record = self.0.record.borrow();
        let EnvironmentRecord::Function(record) = &*record else {
            return None;
        };
        record
            .declarative_record
            .bindings
            .iter()
            .position(|binding| binding.name == *name)
    }

    #[inline]
    pub(crate) fn get_function_binding_at(
        &self,
        name: &JsString,
        index: Option<usize>,
        strict: bool,
    ) -> RuntimeResult<Value> {
        {
            let record = self.0.record.borrow();
            if let (EnvironmentRecord::Function(record), Some(index)) = (&*record, index) {
                if let Some(binding) = record
                    .declarative_record
                    .bindings
                    .get(index)
                    .filter(|binding| binding.name == *name)
                {
                    return binding.read(self);
                }
            }
        }
        self.get_binding_value(name, strict)
    }

    pub(crate) fn set_function_binding_at(
        &self,
        name: &JsString,
        index: Option<usize>,
        value: Value,
        strict: bool,
    ) -> RuntimeResult<()> {
        {
            let mut record = self.0.record.borrow_mut();
            if let (EnvironmentRecord::Function(record), Some(index)) = (&mut *record, index) {
                if let Some(binding) = record
                    .declarative_record
                    .bindings
                    .get_mut(index)
                    .filter(|binding| binding.name == *name)
                {
                    return binding.write(self, value, strict);
                }
            }
        }
        self.set_mutable_binding(name, value, strict)
    }
    /// Creates a declarative environment record in its required parent's context.
    #[must_use]
    pub fn declarative(outer: Self) -> Self {
        Self::new(
            Some(outer),
            EnvironmentRecord::Declarative(DeclarativeRecord::default()),
        )
    }

    /// Creates a parentless declarative record in an explicit context.
    #[must_use]
    pub fn declarative_in(context: &super::AllocationContext) -> Self {
        Self::new_with_runtime(
            None,
            EnvironmentRecord::Declarative(DeclarativeRecord::default()),
            Some(context.clone()),
        )
    }

    /// Creates an object environment record.
    #[must_use]
    pub fn object(
        binding_object: Object,
        outer: Option<Self>,
        options: ObjectEnvironmentOptions,
    ) -> Self {
        let runtime = binding_object.0.runtime().cloned();
        let stored = StoredObject::initial(runtime.as_ref(), &binding_object)
            .expect("binding object context");
        Self::new_with_runtime(
            outer,
            EnvironmentRecord::Object(ObjectRecord {
                binding_object: stored,
                options,
            }),
            runtime,
        )
    }

    /// Creates a global environment record.
    #[must_use]
    pub fn global(global_object: Object, global_this_value: Value) -> Self {
        let runtime = global_object.0.runtime().cloned();
        let binding_object =
            StoredObject::initial(runtime.as_ref(), &global_object).expect("global object context");
        let stored_this = StoredValue::initial(runtime.as_ref(), &global_this_value)
            .expect("global this context");
        Self::new_with_runtime(
            None,
            EnvironmentRecord::Global(GlobalRecord {
                object_record: ObjectRecord {
                    binding_object,
                    options: ObjectEnvironmentOptions::default(),
                },
                declarative_record: DeclarativeRecord::default(),
                global_this_value: stored_this,
                var_declared_names: Vec::new(),
            }),
            runtime,
        )
    }

    /// Creates a function environment record.
    #[must_use]
    pub fn function(
        outer: Option<Self>,
        function_object: Object,
        new_target: Option<Object>,
        this_status: ThisBindingStatus,
    ) -> Self {
        let this = match this_status {
            ThisBindingStatus::Lexical => FunctionThis::Lexical,
            ThisBindingStatus::Uninitialized => FunctionThis::Uninitialized,
            ThisBindingStatus::Initialized => FunctionThis::Initialized(Value::Undefined),
        };
        Self::function_with_parameters(outer, function_object, new_target, this, &[], &[], 0)
    }

    pub(crate) fn function_with_parameters(
        outer: Option<Self>,
        function_object: Object,
        new_target: Option<Object>,
        this: FunctionThis,
        parameters: &[JsString],
        arguments: &[Value],
        capacity: usize,
    ) -> Self {
        let runtime = function_object.0.runtime().cloned();
        let stored_new_target = new_target
            .as_ref()
            .map(|value| StoredObject::initial(runtime.as_ref(), value))
            .transpose()
            .expect("new.target allocation context");
        let stored_function = StoredObject::initial(runtime.as_ref(), &function_object)
            .expect("function allocation context");
        let (this_status, this_value) = match this {
            FunctionThis::Lexical => (ThisBindingStatus::Lexical, None),
            FunctionThis::Uninitialized => (ThisBindingStatus::Uninitialized, None),
            FunctionThis::Initialized(value) => (
                ThisBindingStatus::Initialized,
                Some(
                    StoredValue::initial(runtime.as_ref(), &value)
                        .expect("this allocation context"),
                ),
            ),
        };
        let mut bindings: Vec<Binding> = Vec::with_capacity(capacity.max(parameters.len()));
        for (index, name) in parameters.iter().enumerate() {
            let value = StoredValue::initial(
                runtime.as_ref(),
                arguments.get(index).unwrap_or(&Value::Undefined),
            )
            .expect("parameter allocation context");
            if let Some(binding) = bindings.iter_mut().find(|binding| binding.name == *name) {
                // Sloppy duplicate parameters retain the last supplied value,
                // including undefined when that occurrence has no argument.
                binding.value = Some(value);
            } else {
                bindings.push(Binding {
                    name: name.clone(),
                    value: Some(value),
                    mutable: true,
                    strict: false,
                    deletable: false,
                });
            }
        }
        // All initial edges participate in the incoming-record trace before
        // allocation. They need no post-publication write barriers.
        Self::new_with_runtime(
            outer,
            EnvironmentRecord::Function(FunctionRecord {
                declarative_record: DeclarativeRecord { bindings },
                this_status,
                this_value,
                function_object: stored_function,
                new_target: stored_new_target,
            }),
            runtime,
        )
    }

    /// Creates a module environment record in its required parent's context.
    #[must_use]
    pub fn module(outer: Self) -> Self {
        Self::new(
            Some(outer),
            EnvironmentRecord::Module(ModuleRecord::default()),
        )
    }

    /// Creates a parentless module record in an explicit context.
    #[must_use]
    pub fn module_in(context: &super::AllocationContext) -> Self {
        Self::new_with_runtime(
            None,
            EnvironmentRecord::Module(ModuleRecord::default()),
            Some(context.clone()),
        )
    }

    fn new(outer: Option<Self>, record: EnvironmentRecord) -> Self {
        let runtime = outer.as_ref().and_then(StoredOwner::runtime).cloned();
        Self::new_with_runtime(outer, record, runtime)
    }

    fn new_with_runtime(
        outer: Option<Self>,
        record: EnvironmentRecord,
        runtime: Option<Runtime>,
    ) -> Self {
        let runtime = runtime.expect("environment allocation context");
        let data = EnvironmentData {
            facade: RefCell::new(Weak::new()),
            outer: outer
                .as_ref()
                .map(|outer| StoredEnvironment::initial(Some(&runtime), outer))
                .transpose()
                .expect("outer environment allocation context"),
            record: RefCell::new(record),
        };
        runtime.allocate_environment(data)
    }

    /// Returns this environment record's category.
    #[must_use]
    pub fn kind(&self) -> EnvironmentKind {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Declarative(_) => EnvironmentKind::Declarative,
            EnvironmentRecord::Object(_) => EnvironmentKind::Object,
            EnvironmentRecord::Global(_) => EnvironmentKind::Global,
            EnvironmentRecord::Function(_) => EnvironmentKind::Function,
            EnvironmentRecord::Module(_) => EnvironmentKind::Module,
        }
    }

    /// Returns the outer lexical environment.
    #[must_use]
    pub fn outer(&self) -> Option<Self> {
        self.0
            .outer
            .as_ref()
            .map(|outer| outer.load(self))
            .transpose()
            .expect("live outer environment")
    }

    /// Tests whether this record exposes a binding with the supplied name.
    pub fn has_binding(&self, name: &JsString) -> RuntimeResult<bool> {
        let object_record = {
            let record = self.0.record.borrow();
            match &*record {
                EnvironmentRecord::Declarative(record) => return Ok(record.has_binding(name)),
                EnvironmentRecord::Object(record) => record.clone(),
                EnvironmentRecord::Global(record) => {
                    if record.declarative_record.has_binding(name) {
                        return Ok(true);
                    }
                    record.object_record.clone()
                }
                EnvironmentRecord::Function(record) => {
                    return Ok(record.declarative_record.has_binding(name));
                }
                EnvironmentRecord::Module(record) => {
                    return Ok(record.declarative_record.has_binding(name)
                        || record
                            .imports
                            .iter()
                            .any(|binding| binding.local_name == *name));
                }
            }
        };
        object_record.has_binding(self, name)
    }

    /// Creates an uninitialized mutable binding.
    pub fn create_mutable_binding(&self, name: JsString, deletable: bool) -> RuntimeResult<()> {
        let object_record = {
            let record = self.0.record.borrow();
            match &*record {
                EnvironmentRecord::Object(record) => Some(record.clone()),
                _ => None,
            }
        };
        if let Some(record) = object_record {
            return record.create_mutable(self, name, deletable);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.create_mutable(name, deletable),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                record.declarative_record.create_mutable(name, deletable)
            }
            EnvironmentRecord::Function(record) => {
                record.declarative_record.create_mutable(name, deletable)
            }
            EnvironmentRecord::Module(record) => {
                record.declarative_record.create_mutable(name, deletable)
            }
        }
    }

    /// Creates an uninitialized immutable binding.
    pub fn create_immutable_binding(&self, name: JsString, strict: bool) -> RuntimeResult<()> {
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.create_immutable(name, strict),
            EnvironmentRecord::Global(record) => {
                record.declarative_record.create_immutable(name, strict)
            }
            EnvironmentRecord::Function(record) => {
                record.declarative_record.create_immutable(name, strict)
            }
            EnvironmentRecord::Module(record) => {
                record.declarative_record.create_immutable(name, strict)
            }
            EnvironmentRecord::Object(_) => Err(RuntimeError::type_error(
                "object environments cannot create immutable bindings",
            )),
        }
    }

    /// Initializes an existing binding, ending its temporal dead zone.
    pub fn initialize_binding(&self, name: &JsString, value: Value) -> RuntimeResult<()> {
        if let Some(record) = self.object_record_for_binding(name) {
            return record.initialize(self, name, value);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.initialize(self, name, value),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                record.declarative_record.initialize(self, name, value)
            }
            EnvironmentRecord::Function(record) => {
                record.declarative_record.initialize(self, name, value)
            }
            EnvironmentRecord::Module(record) => {
                if record
                    .imports
                    .iter()
                    .any(|binding| binding.local_name == *name)
                {
                    Err(RuntimeError::type_error(
                        "cannot initialize an import binding",
                    ))
                } else {
                    record.declarative_record.initialize(self, name, value)
                }
            }
        }
    }

    /// Assigns an existing mutable binding.
    pub fn set_mutable_binding(
        &self,
        name: &JsString,
        value: Value,
        strict: bool,
    ) -> RuntimeResult<()> {
        if let Some(record) = self.object_record_for_binding(name) {
            return record.set(self, name, value, strict);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.set(self, name, value, strict),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    record.declarative_record.set(self, name, value, strict)
                } else {
                    unreachable!()
                }
            }
            EnvironmentRecord::Function(record) => {
                record.declarative_record.set(self, name, value, strict)
            }
            EnvironmentRecord::Module(record) => {
                if record
                    .imports
                    .iter()
                    .any(|binding| binding.local_name == *name)
                {
                    Err(RuntimeError::type_error(
                        "cannot assign to an import binding",
                    ))
                } else {
                    record.declarative_record.set(self, name, value, strict)
                }
            }
        }
    }

    /// Reads a binding, raising ReferenceError for absent or uninitialized names.
    pub fn get_binding_value(&self, name: &JsString, strict: bool) -> RuntimeResult<Value> {
        if let Some(record) = self.object_record_for_binding(name) {
            return record.get(self, name, strict);
        }
        let imported = {
            let record = self.0.record.borrow();
            match &*record {
                EnvironmentRecord::Module(record) => record
                    .imports
                    .iter()
                    .find(|binding| binding.local_name == *name)
                    .cloned(),
                _ => None,
            }
        };
        if let Some(binding) = imported {
            return binding
                .target_environment
                .load(self)?
                .get_binding_value(&binding.target_name, true);
        }
        let record = self.0.record.borrow();
        match &*record {
            EnvironmentRecord::Declarative(record) => record.get(self, name),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    record.declarative_record.get(self, name)
                } else {
                    unreachable!()
                }
            }
            EnvironmentRecord::Function(record) => record.declarative_record.get(self, name),
            EnvironmentRecord::Module(record) => record.declarative_record.get(self, name),
        }
    }

    /// Deletes a binding when the record and its attributes allow deletion.
    pub fn delete_binding(&self, name: &JsString) -> RuntimeResult<bool> {
        if let Some(object_record) = self.object_record_for_binding(name) {
            let deleted = object_record.delete(self, name)?;
            if deleted {
                let mut record = self.0.record.borrow_mut();
                if let EnvironmentRecord::Global(record) = &mut *record {
                    record
                        .var_declared_names
                        .retain(|candidate| candidate != name);
                }
            }
            return Ok(deleted);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => Ok(record.delete(name)),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    Ok(record.declarative_record.delete(name))
                } else {
                    unreachable!()
                }
            }
            EnvironmentRecord::Function(record) => Ok(record.declarative_record.delete(name)),
            EnvironmentRecord::Module(record) => {
                if record
                    .imports
                    .iter()
                    .any(|binding| binding.local_name == *name)
                {
                    Ok(false)
                } else {
                    Ok(record.declarative_record.delete(name))
                }
            }
        }
    }

    /// Resolves a name through the lexical environment chain.
    pub fn resolve_binding(&self, name: &JsString) -> RuntimeResult<Option<Self>> {
        let mut environment = Some(self.clone());
        while let Some(current) = environment {
            if current.has_binding(name)? {
                return Ok(Some(current));
            }
            environment = current.outer();
        }
        Ok(None)
    }

    /// Reads a resolved identifier through the lexical environment chain.
    pub fn get_identifier_value(&self, name: &JsString, strict: bool) -> RuntimeResult<Value> {
        self.resolve_binding(name)?
            .ok_or_else(|| reference_error(name, "is not defined"))?
            .get_binding_value(name, strict)
    }

    /// Creates a live immutable import binding in a module environment.
    pub fn create_import_binding(
        &self,
        local_name: JsString,
        target_environment: Self,
        target_name: JsString,
    ) -> RuntimeResult<()> {
        StoredEnvironment::check_context(self.runtime(), &target_environment)?;
        if !target_environment.has_binding(&target_name)? {
            return Err(reference_error(
                &target_name,
                "does not exist in target module",
            ));
        }
        let mut record = self.0.record.borrow_mut();
        let EnvironmentRecord::Module(record) = &mut *record else {
            return Err(RuntimeError::type_error(
                "import bindings require a module environment",
            ));
        };
        if record.declarative_record.has_binding(&local_name)
            || record
                .imports
                .iter()
                .any(|binding| binding.local_name == local_name)
        {
            return Err(RuntimeError::syntax_error("duplicate module binding"));
        }
        record.imports.push(ImportBinding {
            local_name,
            target_environment: StoredEnvironment::store(self, &target_environment)?,
            target_name,
        });
        Ok(())
    }

    /// Returns the object that supplies bindings for an Object or Global record.
    #[must_use]
    pub fn binding_object(&self) -> Option<Object> {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Object(record) => Some(
                record
                    .binding_object
                    .load(self)
                    .expect("live binding object"),
            ),
            EnvironmentRecord::Global(record) => Some(
                record
                    .object_record
                    .binding_object
                    .load(self)
                    .expect("live global object"),
            ),
            _ => None,
        }
    }

    /// Returns the binding object when this is a `with` environment record.
    #[must_use]
    pub fn with_base_object(&self) -> Option<Object> {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Object(record) if record.options.with_environment => {
                Some(record.binding_object.load(self).expect("live with object"))
            }
            _ => None,
        }
    }

    /// Tests whether a global lexical declaration exists.
    pub fn has_lexical_declaration(&self, name: &JsString) -> RuntimeResult<bool> {
        self.with_global(|record| Ok(record.declarative_record.has_binding(name)))
    }

    /// Returns legacy bookkeeping of recorded global `var` names.
    /// ECMAScript 2026 lexical eligibility uses record/property checks instead.
    pub fn has_var_declaration(&self, name: &JsString) -> RuntimeResult<bool> {
        self.with_global(|record| Ok(record.var_declared_names.contains(name)))
    }

    /// Tests whether a non-configurable own global property restricts declarations.
    pub fn has_restricted_global_property(&self, name: &JsString) -> RuntimeResult<bool> {
        let object = self.global_binding_object()?;
        Ok(object
            .try_get_own_property(&string_key(name))?
            .is_some_and(|descriptor| descriptor.configurable == Some(false)))
    }

    /// Implements `CanDeclareGlobalVar`.
    pub fn can_declare_global_var(&self, name: &JsString) -> RuntimeResult<bool> {
        let object = self.global_binding_object()?;
        if object.try_get_own_property(&string_key(name))?.is_some() {
            Ok(true)
        } else {
            object.try_is_extensible()
        }
    }

    /// Implements `CanDeclareGlobalFunction`.
    pub fn can_declare_global_function(&self, name: &JsString) -> RuntimeResult<bool> {
        let object = self.global_binding_object()?;
        let Some(descriptor) = object.try_get_own_property(&string_key(name))? else {
            return object.try_is_extensible();
        };
        Ok(descriptor.configurable == Some(true)
            || (descriptor.is_data_descriptor()
                && descriptor.writable == Some(true)
                && descriptor.enumerable == Some(true)))
    }

    /// Tests whether a global lexical declaration can be created without conflict.
    pub fn can_declare_global_lexical(&self, name: &JsString) -> RuntimeResult<bool> {
        Ok(!self.has_lexical_declaration(name)? && !self.has_restricted_global_property(name)?)
    }

    /// Creates and initializes a global `var` binding.
    pub fn create_global_var_binding(&self, name: JsString, deletable: bool) -> RuntimeResult<()> {
        if self.has_lexical_declaration(&name)? {
            return Err(RuntimeError::syntax_error(
                "global var declaration conflicts with a lexical declaration",
            ));
        }
        let object = self.global_binding_object()?;
        let has_own = object.try_get_own_property(&string_key(&name))?.is_some();
        let extensible = object.try_is_extensible()?;
        if !has_own && extensible {
            define_or_error(
                &object,
                string_key(&name),
                PropertyDescriptor::data(Value::Undefined, true, true, deletable),
            )?;
        }
        if !has_own && !extensible {
            return Err(RuntimeError::type_error(
                "global object cannot accept a new var binding",
            ));
        }
        self.with_global_mut(|record| {
            if extensible && !record.var_declared_names.contains(&name) {
                record.var_declared_names.push(name);
            }
            Ok(())
        })
    }

    /// Creates and initializes a global function binding.
    pub fn create_global_function_binding(
        &self,
        name: JsString,
        value: Value,
        deletable: bool,
    ) -> RuntimeResult<()> {
        if self.has_lexical_declaration(&name)? {
            return Err(RuntimeError::syntax_error(
                "global function declaration conflicts with a lexical declaration",
            ));
        }
        if !self.can_declare_global_function(&name)? {
            return Err(RuntimeError::type_error(
                "global function declaration is not permitted",
            ));
        }
        let object = self.global_binding_object()?;
        let key = string_key(&name);
        let descriptor = match object.try_get_own_property(&key)? {
            None => PropertyDescriptor::data(value.clone(), true, true, deletable),
            Some(existing) if existing.configurable == Some(true) => {
                PropertyDescriptor::data(value.clone(), true, true, deletable)
            }
            Some(_) => PropertyDescriptor {
                value: Some(value.clone()),
                writable: Some(true),
                enumerable: Some(true),
                ..PropertyDescriptor::default()
            },
        };
        define_or_error(&object, key.clone(), descriptor)?;
        if !object.set(&key, value)? {
            return Err(RuntimeError::type_error(
                "global function binding could not be assigned",
            ));
        }
        self.with_global_mut(|record| {
            if !record.var_declared_names.contains(&name) {
                record.var_declared_names.push(name);
            }
            Ok(())
        })
    }

    /// Creates an uninitialized global lexical binding after conflict checks.
    pub fn create_global_lexical_binding(
        &self,
        name: JsString,
        mutable: bool,
        strict: bool,
    ) -> RuntimeResult<()> {
        if !self.can_declare_global_lexical(&name)? {
            return Err(RuntimeError::syntax_error(
                "global lexical declaration conflicts with an existing declaration",
            ));
        }
        self.with_global_mut(|record| {
            if mutable {
                record.declarative_record.create_mutable(name, false)
            } else {
                record.declarative_record.create_immutable(name, strict)
            }
        })
    }

    /// Returns the Global Environment Record's `[[GlobalThisValue]]`.
    pub fn global_this_value(&self) -> RuntimeResult<Value> {
        self.with_global(|record| record.global_this_value.load(self))
    }

    /// Returns whether this function record provides a local `this` binding.
    pub fn has_this_binding(&self) -> RuntimeResult<bool> {
        self.with_function(|record| Ok(record.this_status != ThisBindingStatus::Lexical))
    }

    /// Initializes and returns a function environment's `this` binding.
    pub fn bind_this_value(&self, value: Value) -> RuntimeResult<Value> {
        self.with_function_mut(|record| {
            match record.this_status {
                ThisBindingStatus::Lexical => {
                    return Err(RuntimeError::type_error("lexical this cannot be bound"));
                }
                ThisBindingStatus::Initialized => {
                    return Err(reference_error(
                        &JsString::from("this"),
                        "is already initialized",
                    ));
                }
                ThisBindingStatus::Uninitialized => {}
            }
            record.this_value = Some(StoredValue::store(self, &value)?);
            record.this_status = ThisBindingStatus::Initialized;
            Ok(value)
        })
    }

    /// Returns a function environment's initialized `this` binding.
    pub fn get_this_binding(&self) -> RuntimeResult<Value> {
        self.with_function(|record| match record.this_status {
            ThisBindingStatus::Initialized => record.this_value.as_ref().unwrap().load(self),
            ThisBindingStatus::Uninitialized => Err(reference_error(
                &JsString::from("this"),
                "has not been initialized",
            )),
            ThisBindingStatus::Lexical => {
                Err(RuntimeError::type_error("function has lexical this"))
            }
        })
    }

    // Resolve a non-lexical super receiver and its immutable function edge in
    // one record borrow. No key expression or prototype operation runs here.
    pub(super) fn super_reference_binding(&self) -> RuntimeResult<Option<(Value, Object)>> {
        let record = self.0.record.borrow();
        let EnvironmentRecord::Function(record) = &*record else {
            return Ok(None);
        };
        let receiver = match record.this_status {
            ThisBindingStatus::Lexical => return Ok(None),
            ThisBindingStatus::Initialized => record.this_value.as_ref().unwrap().load(self)?,
            ThisBindingStatus::Uninitialized => {
                return Err(reference_error(
                    &JsString::from("this"),
                    "has not been initialized",
                ));
            }
        };
        Ok(Some((receiver, record.function_object.load(self)?)))
    }

    /// Returns the function object associated with a function environment.
    pub fn function_object(&self) -> RuntimeResult<Object> {
        self.with_function(|record| record.function_object.load(self))
    }

    /// Returns the function environment's `new.target`, if present.
    pub fn new_target(&self) -> RuntimeResult<Option<Object>> {
        self.with_function(|record| {
            record
                .new_target
                .as_ref()
                .map(|target| target.load(self))
                .transpose()
        })
    }

    /// Returns the current function-environment `this` binding status.
    pub fn this_binding_status(&self) -> RuntimeResult<ThisBindingStatus> {
        self.with_function(|record| Ok(record.this_status))
    }

    fn object_record_for_binding(&self, name: &JsString) -> Option<ObjectRecord> {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Object(record) => Some(record.clone()),
            EnvironmentRecord::Global(record) if !record.declarative_record.has_binding(name) => {
                Some(record.object_record.clone())
            }
            _ => None,
        }
    }

    fn global_binding_object(&self) -> RuntimeResult<Object> {
        self.with_global(|record| record.object_record.binding_object.load(self))
    }

    fn with_global<T>(
        &self,
        operation: impl FnOnce(&GlobalRecord) -> RuntimeResult<T>,
    ) -> RuntimeResult<T> {
        let record = self.0.record.borrow();
        let EnvironmentRecord::Global(record) = &*record else {
            return Err(RuntimeError::type_error(
                "operation requires a global environment",
            ));
        };
        operation(record)
    }

    fn with_global_mut<T>(
        &self,
        operation: impl FnOnce(&mut GlobalRecord) -> RuntimeResult<T>,
    ) -> RuntimeResult<T> {
        let mut record = self.0.record.borrow_mut();
        let EnvironmentRecord::Global(record) = &mut *record else {
            return Err(RuntimeError::type_error(
                "operation requires a global environment",
            ));
        };
        operation(record)
    }

    fn with_function<T>(
        &self,
        operation: impl FnOnce(&FunctionRecord) -> RuntimeResult<T>,
    ) -> RuntimeResult<T> {
        let record = self.0.record.borrow();
        let EnvironmentRecord::Function(record) = &*record else {
            return Err(RuntimeError::type_error(
                "operation requires a function environment",
            ));
        };
        operation(record)
    }

    fn with_function_mut<T>(
        &self,
        operation: impl FnOnce(&mut FunctionRecord) -> RuntimeResult<T>,
    ) -> RuntimeResult<T> {
        let mut record = self.0.record.borrow_mut();
        let EnvironmentRecord::Function(record) = &mut *record else {
            return Err(RuntimeError::type_error(
                "operation requires a function environment",
            ));
        };
        operation(record)
    }
}

impl PartialEq for Environment {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(&*self.0, &*other.0)
    }
}

impl Eq for Environment {}

impl fmt::Debug for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Environment")
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}

impl DeclarativeRecord {
    fn has_binding(&self, name: &JsString) -> bool {
        self.bindings.iter().any(|binding| binding.name == *name)
    }

    fn create_mutable(&mut self, name: JsString, deletable: bool) -> RuntimeResult<()> {
        self.create(name, true, false, deletable)
    }

    fn create_immutable(&mut self, name: JsString, strict: bool) -> RuntimeResult<()> {
        self.create(name, false, strict, false)
    }

    fn create(
        &mut self,
        name: JsString,
        mutable: bool,
        strict: bool,
        deletable: bool,
    ) -> RuntimeResult<()> {
        if self.has_binding(&name) {
            return Err(RuntimeError::syntax_error("duplicate environment binding"));
        }
        self.bindings.push(Binding {
            name,
            value: None,
            mutable,
            strict,
            deletable,
        });
        Ok(())
    }

    fn initialize(
        &mut self,
        owner: &Environment,
        name: &JsString,
        value: Value,
    ) -> RuntimeResult<()> {
        let binding = self.binding_mut(name)?;
        if binding.value.is_some() {
            return Err(RuntimeError::type_error("binding is already initialized"));
        }
        binding.value = Some(StoredValue::store_owned(owner, value)?);
        Ok(())
    }

    fn set(
        &mut self,
        owner: &Environment,
        name: &JsString,
        value: Value,
        strict: bool,
    ) -> RuntimeResult<()> {
        self.binding_mut(name)?.write(owner, value, strict)
    }

    fn get(&self, owner: &Environment, name: &JsString) -> RuntimeResult<Value> {
        self.bindings
            .iter()
            .find(|binding| binding.name == *name)
            .ok_or_else(|| reference_error(name, "is not defined"))?
            .read(owner)
    }

    fn delete(&mut self, name: &JsString) -> bool {
        let Some(position) = self
            .bindings
            .iter()
            .position(|binding| binding.name == *name)
        else {
            return true;
        };
        if !self.bindings[position].deletable {
            return false;
        }
        self.bindings.remove(position);
        true
    }

    fn binding_mut(&mut self, name: &JsString) -> RuntimeResult<&mut Binding> {
        self.bindings
            .iter_mut()
            .find(|binding| binding.name == *name)
            .ok_or_else(|| reference_error(name, "is not defined"))
    }
}

impl Binding {
    fn read(&self, owner: &Environment) -> RuntimeResult<Value> {
        // Copy immutable primitive bits without dispatching the full Value
        // clone. Name checks and record borrowing still occur in the caller;
        // an uninitialized binding continues through the TDZ error below.
        if let Some(StoredValue::Number(value)) = &self.value {
            return Ok(Value::Number(*value));
        }
        self.value
            .as_ref()
            .ok_or_else(|| reference_error(&self.name, "is in the temporal dead zone"))?
            .load(owner)
    }

    fn write(&mut self, owner: &Environment, value: Value, strict: bool) -> RuntimeResult<()> {
        if self.value.is_none() {
            return Err(reference_error(&self.name, "is in the temporal dead zone"));
        }
        if self.mutable {
            match (&mut self.value, value) {
                // Same-type scalar writes have neither managed edges nor a
                // payload to drop. Keep the TDZ/mutability checks above and
                // avoid general Value conversion/copy/drop work on this path.
                (Some(StoredValue::Number(current)), Value::Number(next)) => *current = next,
                (_, value) => self.value = Some(StoredValue::store_owned(owner, value)?),
            }
            Ok(())
        } else if strict || self.strict {
            Err(RuntimeError::type_error(
                "assignment to an immutable binding",
            ))
        } else {
            Ok(())
        }
    }
}

impl ObjectRecord {
    fn has_binding(&self, owner: &Environment, name: &JsString) -> RuntimeResult<bool> {
        let object = self.binding_object.load(owner)?;
        let found = object.has_property(&string_key(name))?;
        if !found || !self.options.with_environment {
            return Ok(found);
        }
        let Some(unscopables_key) = &self.options.unscopables_key else {
            return Ok(true);
        };
        let unscopables = object.get(unscopables_key)?;
        if let Value::Object(unscopables) = unscopables {
            return Ok(!unscopables.get(&string_key(name))?.to_boolean());
        }
        Ok(true)
    }

    fn create_mutable(
        &self,
        owner: &Environment,
        name: JsString,
        deletable: bool,
    ) -> RuntimeResult<()> {
        let object = self.binding_object.load(owner)?;
        define_or_error(
            &object,
            string_key(&name),
            PropertyDescriptor::data(Value::Undefined, true, true, deletable),
        )
    }

    fn initialize(&self, owner: &Environment, name: &JsString, value: Value) -> RuntimeResult<()> {
        if self
            .binding_object
            .load(owner)?
            .set(&string_key(name), value)?
        {
            Ok(())
        } else {
            Err(RuntimeError::type_error(
                "object binding could not be initialized",
            ))
        }
    }

    fn set(
        &self,
        owner: &Environment,
        name: &JsString,
        value: Value,
        strict: bool,
    ) -> RuntimeResult<()> {
        if self
            .binding_object
            .load(owner)?
            .set(&string_key(name), value)?
            || !strict
        {
            Ok(())
        } else {
            Err(RuntimeError::type_error("object binding is not writable"))
        }
    }

    fn get(&self, owner: &Environment, name: &JsString, strict: bool) -> RuntimeResult<Value> {
        let object = self.binding_object.load(owner)?;
        if !object.has_property(&string_key(name))? {
            return if strict {
                Err(reference_error(name, "is not defined"))
            } else {
                Ok(Value::Undefined)
            };
        }
        object.get(&string_key(name))
    }

    fn delete(&self, owner: &Environment, name: &JsString) -> RuntimeResult<bool> {
        self.binding_object
            .load(owner)?
            .try_delete(&string_key(name))
    }
}

fn string_key(name: &JsString) -> PropertyKey {
    PropertyKey::String(name.clone())
}

fn define_or_error(
    object: &Object,
    key: PropertyKey,
    descriptor: PropertyDescriptor,
) -> RuntimeResult<()> {
    if object.define_own_property(key, descriptor)? {
        Ok(())
    } else {
        Err(RuntimeError::type_error("property definition was rejected"))
    }
}

fn reference_error(name: &JsString, detail: &str) -> RuntimeError {
    RuntimeError::reference_error(format!("{} {detail}", name.to_string_lossy()))
}

#[cfg(test)]
mod slot_hint_tests {
    use super::*;
    #[test]
    fn managed_object_and_global_records_do_not_root_their_stored_objects() {
        for global in [false, true] {
            let runtime = Runtime::new();
            let object = Object::new_in(&runtime);
            object
                .create_data_property("answer".into(), Value::Number(42.0))
                .unwrap();
            object
                .create_data_property("self".into(), Value::Object(object.clone()))
                .unwrap();
            let weak = object.downgrade();
            let global_this = Object::new_in(&runtime);
            let weak_this = global_this.downgrade();
            let environment = if global {
                Environment::global(object.clone(), Value::Object(global_this.clone()))
            } else {
                Environment::object(
                    object.clone(),
                    None,
                    ObjectEnvironmentOptions {
                        with_environment: true,
                        ..ObjectEnvironmentOptions::default()
                    },
                )
            };
            drop(object);
            drop(global_this);
            assert_eq!(runtime.diagnostics().rooted, 1);
            assert_eq!(
                runtime.collect_migration_graph().unwrap().retained,
                if global { 3 } else { 2 }
            );
            assert_eq!(environment.binding_object(), weak.upgrade());
            assert!(
                environment
                    .get_binding_value(&"answer".into(), true)
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
            if global {
                assert!(
                    environment
                        .global_this_value()
                        .unwrap()
                        .same_value(&Value::Object(weak_this.upgrade().unwrap()))
                );
            } else {
                assert_eq!(environment.with_base_object(), weak.upgrade());
                assert!(weak_this.upgrade().is_none());
            }
            drop(environment);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
            assert!(weak.upgrade().is_none());
            assert!(weak_this.upgrade().is_none());
        }
    }
    #[test]
    fn managed_global_declarations_release_borrows_before_proxy_reentry_and_gc() {
        let runtime = Runtime::new();
        let target = Object::new_in(&runtime);
        let handler = Object::new_in(&runtime);
        let trap = Object::new_in(&runtime);
        let slot = Rc::new(RefCell::new(None::<Environment>));
        let callback_slot = slot.clone();
        let callback_runtime = runtime.clone();
        let calls = Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        trap.set_native_call(move |_, _| {
            callback_runtime.collect_migration_graph().unwrap();
            let environment = callback_slot.borrow().clone().unwrap();
            let count = callback_calls.get() + 1;
            callback_calls.set(count);
            environment.create_mutable_binding(format!("nested{count}").into(), false)?;
            Ok(Value::Undefined)
        });
        handler
            .create_data_property(
                "getOwnPropertyDescriptor".into(),
                Value::Object(trap.clone()),
            )
            .unwrap();
        let proxy = crate::runtime::ProxyObject::try_new(target.clone(), handler.clone()).unwrap();
        let environment = Environment::global(proxy.object(), Value::Object(proxy.object()));
        *slot.borrow_mut() = Some(environment.clone());
        environment
            .create_global_var_binding("variable".into(), true)
            .unwrap();
        environment
            .create_global_function_binding("function".into(), Value::Number(42.0), true)
            .unwrap();
        assert!(environment.has_var_declaration(&"variable".into()).unwrap());
        assert!(environment.has_var_declaration(&"function".into()).unwrap());
        assert!(
            target
                .get(&"function".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert!(calls.get() >= 2);
        let delete_trap = Object::new_in(&runtime);
        let delete_runtime = runtime.clone();
        let delete_slot = slot.clone();
        delete_trap.set_native_call(move |_, arguments| {
            delete_runtime.collect_migration_graph().unwrap();
            let environment = delete_slot.borrow().clone().unwrap();
            environment.create_mutable_binding("deleteNested".into(), false)?;
            let Value::Object(target) = &arguments[0] else {
                panic!("delete target");
            };
            let Value::String(key) = &arguments[1] else {
                panic!("delete key");
            };
            target.try_delete(&key.clone().into()).map(Value::Boolean)
        });
        handler
            .create_data_property("deleteProperty".into(), Value::Object(delete_trap.clone()))
            .unwrap();
        assert!(environment.delete_binding(&"function".into()).unwrap());
        assert!(!environment.has_var_declaration(&"function".into()).unwrap());
        assert!(target.get_own_property(&"function".into()).is_none());
        let marker = Object::new_in(&runtime);
        let thrown = marker.clone();
        trap.set_native_call(move |_, _| Err(RuntimeError::thrown(Value::Object(thrown.clone()))));
        let thrown = marker.clone();
        delete_trap
            .set_native_call(move |_, _| Err(RuntimeError::thrown(Value::Object(thrown.clone()))));
        for result in [
            environment
                .has_restricted_global_property(&"abrupt".into())
                .map(|_| ()),
            environment
                .can_declare_global_var(&"abrupt".into())
                .map(|_| ()),
            environment
                .can_declare_global_function(&"abrupt".into())
                .map(|_| ()),
            environment.create_global_var_binding("abrupt".into(), true),
            environment.create_global_function_binding("abrupt".into(), Value::Number(0.0), true),
            environment.delete_binding(&"abrupt".into()).map(|_| ()),
        ] {
            assert!(
                result
                    .unwrap_err()
                    .thrown_value()
                    .unwrap()
                    .same_value(&Value::Object(marker.clone()))
            );
        }
        assert!(!environment.has_var_declaration(&"abrupt".into()).unwrap());
        assert!(target.get_own_property(&"abrupt".into()).is_none());
        // Ordinary host captures explicitly release their external roots.
        trap.clear_native_call();
        delete_trap.clear_native_call();
        slot.borrow_mut().take();
    }
    #[test]
    fn managed_function_records_trace_this_function_and_new_target() {
        let runtime = Runtime::new();
        let function = Object::new_in(&runtime);
        let new_target = Object::new_in(&runtime);
        let environment = Environment::function(
            None,
            function.clone(),
            Some(new_target.clone()),
            ThisBindingStatus::Uninitialized,
        );
        let weak_function = function.downgrade();
        let weak_target = new_target.downgrade();
        drop(function);
        drop(new_target);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert!(environment.get_this_binding().is_err());
        let foreign = Object::new_in(&Runtime::new());
        assert!(environment.bind_this_value(Value::Object(foreign)).is_err());
        assert_eq!(
            environment.this_binding_status().unwrap(),
            ThisBindingStatus::Uninitialized
        );
        runtime.collect_migration_graph().unwrap();
        let this = Object::new_in(&runtime);
        this.create_data_property("self".into(), Value::Object(this.clone()))
            .unwrap();
        let weak_this = this.downgrade();
        environment
            .bind_this_value(Value::Object(this.clone()))
            .unwrap();
        drop(this);
        runtime.verify_migration_barriers().unwrap();
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
        assert_eq!(
            environment.function_object().unwrap(),
            weak_function.upgrade().unwrap()
        );
        assert_eq!(environment.new_target().unwrap(), weak_target.upgrade());
        let this = weak_this.upgrade().unwrap();
        assert!(
            environment
                .get_this_binding()
                .unwrap()
                .same_value(&Value::Object(this.clone()))
        );
        assert!(environment.bind_this_value(Value::Null).is_err());
        drop(this);
        drop(environment);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 4);
        assert!(weak_function.upgrade().is_none());
        assert!(weak_target.upgrade().is_none());
        assert!(weak_this.upgrade().is_none());
    }
    #[test]
    fn managed_module_import_cycles_keep_live_bindings_without_internal_roots() {
        let runtime = Runtime::new();
        let parent = runtime.allocate_environment(EnvironmentData {
            facade: RefCell::new(Weak::new()),
            outer: None,
            record: RefCell::new(EnvironmentRecord::Declarative(DeclarativeRecord::default())),
        });
        let first = Environment::module(parent.clone());
        first.create_mutable_binding("x".into(), false).unwrap();
        first
            .initialize_binding(&"x".into(), Value::Number(40.0))
            .unwrap();
        runtime.collect_migration_graph().unwrap();
        let second = Environment::module(parent.clone());
        second.create_mutable_binding("y".into(), false).unwrap();
        second
            .initialize_binding(&"y".into(), Value::Number(41.0))
            .unwrap();
        let second_handle = second.handle().unwrap();
        first
            .create_import_binding("alias".into(), second.clone(), "y".into())
            .unwrap();
        second
            .create_import_binding("back".into(), first.clone(), "x".into())
            .unwrap();
        drop(parent);
        drop(second);
        assert_eq!(runtime.diagnostics().rooted, 1);
        runtime.verify_migration_barriers().unwrap();
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        let second = runtime.root_environment(second_handle).unwrap();
        second
            .set_mutable_binding(&"y".into(), Value::Number(42.0), true)
            .unwrap();
        assert!(
            first
                .get_binding_value(&"alias".into(), true)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert!(
            second
                .get_binding_value(&"back".into(), true)
                .unwrap()
                .same_value(&Value::Number(40.0))
        );
        let foreign_context = Runtime::new();
        let foreign = Environment::module_in(&foreign_context);
        foreign.create_mutable_binding("z".into(), false).unwrap();
        foreign
            .initialize_binding(&"z".into(), Value::Number(0.0))
            .unwrap();
        assert!(
            first
                .create_import_binding("invalid".into(), foreign, "z".into())
                .is_err()
        );
        assert!(!first.has_binding(&"invalid".into()).unwrap());
        drop(second);
        drop(first);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 3);
    }
    #[test]
    fn managed_bindings_trace_objects_and_release_replaced_cycles() {
        let runtime = Runtime::new();
        let environment = runtime.allocate_environment(EnvironmentData {
            facade: RefCell::new(Weak::new()),
            outer: None,
            record: RefCell::new(EnvironmentRecord::Declarative(DeclarativeRecord::default())),
        });
        let key = JsString::from("object");
        environment
            .create_mutable_binding(key.clone(), false)
            .unwrap();
        assert!(environment.get_binding_value(&key, true).is_err());
        environment
            .initialize_binding(&key, Value::Undefined)
            .unwrap();
        runtime.collect_migration_graph().unwrap();
        let object = Object::new_in(&runtime);
        object
            .create_data_property("self".into(), Value::Object(object.clone()))
            .unwrap();
        let weak = object.downgrade();
        environment
            .set_mutable_binding(&key, Value::Object(object.clone()), true)
            .unwrap();
        drop(object);
        assert_eq!(runtime.diagnostics().rooted, 1);
        runtime.verify_migration_barriers().unwrap();
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 2);
        let Value::Object(loaded) = environment.get_binding_value(&key, true).unwrap() else {
            panic!("object binding");
        };
        assert!(
            loaded
                .get(&"self".into())
                .unwrap()
                .same_value(&Value::Object(loaded.clone()))
        );
        drop(loaded);
        let foreign = Object::new_in(&Runtime::new());
        assert!(
            environment
                .set_mutable_binding(&key, Value::Object(foreign), true)
                .is_err()
        );
        assert!(matches!(
            environment.get_binding_value(&key, true).unwrap(),
            Value::Object(_)
        ));
        environment
            .set_mutable_binding(&key, Value::Number(-0.0), true)
            .unwrap();
        assert!(
            environment
                .get_binding_value(&key, true)
                .unwrap()
                .same_value(&Value::Number(-0.0))
        );
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 1);
        assert!(weak.upgrade().is_none());
        drop(environment);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 1);
    }
    #[test]
    fn managed_outer_environment_is_traced_without_an_internal_root() {
        let runtime = Runtime::new();
        let outer = runtime.allocate_environment(EnvironmentData {
            facade: RefCell::new(Weak::new()),
            outer: None,
            record: RefCell::new(EnvironmentRecord::Declarative(DeclarativeRecord::default())),
        });
        outer
            .create_mutable_binding("answer".into(), false)
            .unwrap();
        outer
            .initialize_binding(&"answer".into(), Value::Number(42.0))
            .unwrap();
        let handle = outer.handle().unwrap();
        let child = Environment::declarative(outer.clone());
        let clone = child.clone();
        assert_eq!(runtime.diagnostics().rooted, 2);
        assert_eq!(child.outer(), Some(outer.clone()));
        drop(outer);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 2);
        let reloaded = clone.outer().unwrap();
        assert_eq!(reloaded.handle(), Some(handle));
        assert!(
            reloaded
                .get_binding_value(&"answer".into(), true)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(reloaded);
        drop(child);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 2);
        drop(clone);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 2);
    }
    #[test]
    fn prepared_parameter_records_initialize_missing_and_duplicate_names() {
        let context = Runtime::new();
        let names = [
            JsString::from("a"),
            JsString::from("a"),
            JsString::from("b"),
        ];
        let env = Environment::function_with_parameters(
            None,
            Object::new_in(&context),
            None,
            FunctionThis::Uninitialized,
            &names,
            &[Value::Number(1.0)],
            3,
        );
        for name in [&names[0], &names[2]] {
            assert!(
                env.get_binding_value(name, false)
                    .unwrap()
                    .same_value(&Value::Undefined)
            );
            assert!(!env.delete_binding(name).unwrap());
        }
        env.set_mutable_binding(&names[0], Value::Number(42.0), true)
            .unwrap();
        assert!(
            env.get_binding_value(&names[0], true)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
    }

    #[test]
    fn prepared_function_edges_survive_collection_before_frame_publication() {
        let context = Runtime::new();
        let function = Object::new_in(&context);
        let weak_function = function.downgrade();
        let receiver = Object::new_in(&context);
        receiver
            .create_data_property("self".into(), Value::Object(receiver.clone()))
            .unwrap();
        let weak_receiver = receiver.downgrade();
        let argument = Object::new_in(&context);
        argument
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let target = Object::new_in(&context);
        let weak_target = target.downgrade();
        let arguments = [Value::Object(argument)];
        context.set_allocation_collection_interval(1);
        let environment = Environment::function_with_parameters(
            None,
            function,
            Some(target),
            FunctionThis::Initialized(Value::Object(receiver)),
            &["value".into()],
            &arguments,
            1,
        );
        drop(arguments);
        context.collect_garbage().unwrap();
        assert_eq!(
            environment.function_object().unwrap(),
            weak_function.upgrade().unwrap()
        );
        assert_eq!(
            environment.new_target().unwrap().unwrap(),
            weak_target.upgrade().unwrap()
        );
        {
            let Value::Object(receiver) = environment.get_this_binding().unwrap() else {
                panic!("receiver")
            };
            assert_eq!(receiver, weak_receiver.upgrade().unwrap());
            assert!(
                receiver
                    .get(&"self".into())
                    .unwrap()
                    .same_value(&Value::Object(receiver.clone()))
            );
            let Value::Object(argument) = environment
                .get_binding_value(&"value".into(), true)
                .unwrap()
            else {
                panic!("argument")
            };
            assert!(
                argument
                    .get(&"answer".into())
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
        }
        context.verify_migration_barriers().unwrap();
        drop(environment);
        assert_eq!(context.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn number_update_respects_ignored_and_throwing_immutable_writes() {
        let context = Runtime::new();
        let env = Environment::function(
            None,
            Object::new_in(&context),
            None,
            ThisBindingStatus::Uninitialized,
        );
        let name: JsString = "fixed".into();
        env.create_immutable_binding(name.clone(), false).unwrap();
        env.initialize_binding(&name, Value::Number(41.0)).unwrap();
        let hint = env.function_binding_index(&name);
        assert_eq!(
            env.update_number_binding(&name, hint, true, false).unwrap(),
            Some((41.0, 42.0))
        );
        assert!(
            env.get_binding_value(&name, false)
                .unwrap()
                .same_value(&Value::Number(41.0))
        );
        assert_eq!(
            env.update_number_binding(&name, hint, true, true)
                .unwrap_err()
                .kind(),
            super::super::RuntimeErrorKind::TypeError
        );
        assert!(
            env.get_binding_value(&name, false)
                .unwrap()
                .same_value(&Value::Number(41.0))
        );
    }

    #[test]
    fn string_append_rechecks_shifted_slots_and_preserves_readonly_bindings() {
        let context = Runtime::new();
        let env = Environment::function(
            None,
            Object::new_in(&context),
            None,
            ThisBindingStatus::Uninitialized,
        );
        let prefix: JsString = "prefix".into();
        let name: JsString = "value".into();
        env.create_mutable_binding(prefix.clone(), true).unwrap();
        env.initialize_binding(&prefix, Value::Null).unwrap();
        env.create_mutable_binding(name.clone(), false).unwrap();
        env.initialize_binding(&name, Value::String("left_".into()))
            .unwrap();
        let hint = env.function_binding_index(&name);
        assert!(env.delete_binding(&prefix).unwrap());
        let other: JsString = "other".into();
        env.create_mutable_binding(other.clone(), false).unwrap();
        env.initialize_binding(&other, Value::String("other".into()))
            .unwrap();
        assert!(
            env.append_string_binding(&name, hint, &"right".into())
                .unwrap()
                .same_value(&Value::String("left_right".into()))
        );
        assert!(
            env.get_binding_value(&other, false)
                .unwrap()
                .same_value(&Value::String("other".into()))
        );
        let fixed: JsString = "fixed".into();
        env.create_immutable_binding(fixed.clone(), true).unwrap();
        env.initialize_binding(&fixed, Value::String("fixed".into()))
            .unwrap();
        assert!(
            env.append_string_binding(&fixed, None, &"suffix".into())
                .is_none()
        );
        assert!(
            env.get_binding_value(&fixed, false)
                .unwrap()
                .same_value(&Value::String("fixed".into()))
        );
    }

    #[test]
    fn stale_function_slot_hints_preserve_binding_identity_and_checks() {
        let context = Runtime::new();
        let env = Environment::function(
            None,
            Object::new_in(&context),
            None,
            ThisBindingStatus::Uninitialized,
        );
        let discarded: JsString = "discarded".into();
        let name: JsString = "value".into();
        for (key, value, deletable) in [(&discarded, 0.0, true), (&name, 41.0, false)] {
            env.create_mutable_binding(key.clone(), deletable).unwrap();
            env.initialize_binding(key, Value::Number(value)).unwrap();
        }
        let hint = env.function_binding_index(&name);
        assert_eq!(hint, Some(1));
        assert!(env.delete_binding(&discarded).unwrap());
        let other: JsString = "other".into();
        env.create_mutable_binding(other.clone(), false).unwrap();
        env.initialize_binding(&other, Value::Number(99.0)).unwrap();
        assert!(
            env.get_function_binding_at(&name, hint, false)
                .unwrap()
                .same_value(&Value::Number(41.0))
        );
        env.set_function_binding_at(&name, hint, Value::Number(42.0), false)
            .unwrap();
        assert!(
            env.get_binding_value(&name, false)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert!(
            env.get_binding_value(&other, false)
                .unwrap()
                .same_value(&Value::Number(99.0))
        );
        let tdz: JsString = "tdz".into();
        env.create_mutable_binding(tdz.clone(), false).unwrap();
        let hint = env.function_binding_index(&tdz);
        assert_eq!(
            env.get_function_binding_at(&tdz, hint, false)
                .unwrap_err()
                .kind(),
            super::super::RuntimeErrorKind::ReferenceError
        );
        assert_eq!(
            env.set_function_binding_at(&tdz, hint, Value::Null, false)
                .unwrap_err()
                .kind(),
            super::super::RuntimeErrorKind::ReferenceError
        );
        for strict_binding in [false, true] {
            let key: JsString = if strict_binding { "constant" } else { "named" }.into();
            env.create_immutable_binding(key.clone(), strict_binding)
                .unwrap();
            env.initialize_binding(&key, Value::Number(1.0)).unwrap();
            let hint = env.function_binding_index(&key);
            let result = env.set_function_binding_at(&key, hint, Value::Number(2.0), false);
            if strict_binding {
                assert_eq!(
                    result.unwrap_err().kind(),
                    super::super::RuntimeErrorKind::TypeError
                );
            } else {
                result.unwrap();
            }
            assert!(
                env.get_binding_value(&key, false)
                    .unwrap()
                    .same_value(&Value::Number(1.0))
            );
        }
    }
}
