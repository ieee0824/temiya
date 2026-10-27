//! Lexical environments and ECMAScript environment records.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

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

/// An identity-bearing lexical environment with an optional outer environment.
#[derive(Clone)]
pub struct Environment(Rc<EnvironmentData>);

struct EnvironmentData {
    outer: Option<Environment>,
    record: RefCell<EnvironmentRecord>,
}

enum EnvironmentRecord {
    Declarative(DeclarativeRecord),
    Object(ObjectRecord),
    Global(GlobalRecord),
    Function(FunctionRecord),
    Module(ModuleRecord),
}

#[derive(Default)]
struct DeclarativeRecord {
    bindings: Vec<Binding>,
}

struct Binding {
    name: JsString,
    value: Option<Value>,
    mutable: bool,
    strict: bool,
    deletable: bool,
}

#[derive(Clone)]
struct ObjectRecord {
    binding_object: Object,
    options: ObjectEnvironmentOptions,
}

struct GlobalRecord {
    object_record: ObjectRecord,
    declarative_record: DeclarativeRecord,
    global_this_value: Value,
    var_declared_names: Vec<JsString>,
}

struct FunctionRecord {
    declarative_record: DeclarativeRecord,
    this_status: ThisBindingStatus,
    this_value: Option<Value>,
    function_object: Object,
    new_target: Option<Object>,
}

#[derive(Default)]
struct ModuleRecord {
    declarative_record: DeclarativeRecord,
    imports: Vec<ImportBinding>,
}

#[derive(Clone)]
struct ImportBinding {
    local_name: JsString,
    target_environment: Environment,
    target_name: JsString,
}

impl Environment {
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
                    return binding.read();
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
                    return binding.write(value, strict);
                }
            }
        }
        self.set_mutable_binding(name, value, strict)
    }
    /// Creates a declarative environment record.
    #[must_use]
    pub fn declarative(outer: Option<Self>) -> Self {
        Self::new(
            outer,
            EnvironmentRecord::Declarative(DeclarativeRecord::default()),
        )
    }

    /// Creates an object environment record.
    #[must_use]
    pub fn object(
        binding_object: Object,
        outer: Option<Self>,
        options: ObjectEnvironmentOptions,
    ) -> Self {
        Self::new(
            outer,
            EnvironmentRecord::Object(ObjectRecord {
                binding_object,
                options,
            }),
        )
    }

    /// Creates a global environment record.
    #[must_use]
    pub fn global(global_object: Object, global_this_value: Value) -> Self {
        Self::new(
            None,
            EnvironmentRecord::Global(GlobalRecord {
                object_record: ObjectRecord {
                    binding_object: global_object,
                    options: ObjectEnvironmentOptions::default(),
                },
                declarative_record: DeclarativeRecord::default(),
                global_this_value,
                var_declared_names: Vec::new(),
            }),
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
        Self::new(
            outer,
            EnvironmentRecord::Function(FunctionRecord {
                declarative_record: DeclarativeRecord::default(),
                this_status,
                this_value: (this_status == ThisBindingStatus::Initialized)
                    .then_some(Value::Undefined),
                function_object,
                new_target,
            }),
        )
    }

    /// Creates a module environment record.
    #[must_use]
    pub fn module(outer: Option<Self>) -> Self {
        Self::new(outer, EnvironmentRecord::Module(ModuleRecord::default()))
    }

    fn new(outer: Option<Self>, record: EnvironmentRecord) -> Self {
        Self(Rc::new(EnvironmentData {
            outer,
            record: RefCell::new(record),
        }))
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
        self.0.outer.clone()
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
        object_record.has_binding(name)
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
            return record.create_mutable(name, deletable);
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
            return record.initialize(name, value);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.initialize(name, value),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => record.declarative_record.initialize(name, value),
            EnvironmentRecord::Function(record) => {
                record.declarative_record.initialize(name, value)
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
                    record.declarative_record.initialize(name, value)
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
            return record.set(name, value, strict);
        }
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => record.set(name, value, strict),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    record.declarative_record.set(name, value, strict)
                } else {
                    unreachable!()
                }
            }
            EnvironmentRecord::Function(record) => {
                record.declarative_record.set(name, value, strict)
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
                    record.declarative_record.set(name, value, strict)
                }
            }
        }
    }

    /// Reads a binding, raising ReferenceError for absent or uninitialized names.
    pub fn get_binding_value(&self, name: &JsString, strict: bool) -> RuntimeResult<Value> {
        if let Some(record) = self.object_record_for_binding(name) {
            return record.get(name, strict);
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
                .get_binding_value(&binding.target_name, true);
        }
        let record = self.0.record.borrow();
        match &*record {
            EnvironmentRecord::Declarative(record) => record.get(name),
            EnvironmentRecord::Object(_) => unreachable!(),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    record.declarative_record.get(name)
                } else {
                    unreachable!()
                }
            }
            EnvironmentRecord::Function(record) => record.declarative_record.get(name),
            EnvironmentRecord::Module(record) => record.declarative_record.get(name),
        }
    }

    /// Deletes a binding when the record and its attributes allow deletion.
    pub fn delete_binding(&self, name: &JsString) -> RuntimeResult<bool> {
        let mut record = self.0.record.borrow_mut();
        match &mut *record {
            EnvironmentRecord::Declarative(record) => Ok(record.delete(name)),
            EnvironmentRecord::Object(record) => Ok(record.delete(name)),
            EnvironmentRecord::Global(record) => {
                if record.declarative_record.has_binding(name) {
                    Ok(record.declarative_record.delete(name))
                } else {
                    let deleted = record.object_record.delete(name);
                    if deleted {
                        record
                            .var_declared_names
                            .retain(|candidate| candidate != name);
                    }
                    Ok(deleted)
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
            target_environment,
            target_name,
        });
        Ok(())
    }

    /// Returns the object that supplies bindings for an Object or Global record.
    #[must_use]
    pub fn binding_object(&self) -> Option<Object> {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Object(record) => Some(record.binding_object.clone()),
            EnvironmentRecord::Global(record) => Some(record.object_record.binding_object.clone()),
            _ => None,
        }
    }

    /// Returns the binding object when this is a `with` environment record.
    #[must_use]
    pub fn with_base_object(&self) -> Option<Object> {
        match &*self.0.record.borrow() {
            EnvironmentRecord::Object(record) if record.options.with_environment => {
                Some(record.binding_object.clone())
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
        self.with_global(|record| {
            Ok(record
                .object_record
                .binding_object
                .get_own_property(&string_key(name))
                .is_some_and(|descriptor| descriptor.configurable == Some(false)))
        })
    }

    /// Implements `CanDeclareGlobalVar`.
    pub fn can_declare_global_var(&self, name: &JsString) -> RuntimeResult<bool> {
        self.with_global(|record| {
            if record
                .object_record
                .binding_object
                .get_own_property(&string_key(name))
                .is_some()
            {
                Ok(true)
            } else {
                Ok(record.object_record.binding_object.is_extensible())
            }
        })
    }

    /// Implements `CanDeclareGlobalFunction`.
    pub fn can_declare_global_function(&self, name: &JsString) -> RuntimeResult<bool> {
        self.with_global(|record| {
            let object = &record.object_record.binding_object;
            let Some(descriptor) = object.get_own_property(&string_key(name)) else {
                return Ok(object.is_extensible());
            };
            Ok(descriptor.configurable == Some(true)
                || (descriptor.is_data_descriptor()
                    && descriptor.writable == Some(true)
                    && descriptor.enumerable == Some(true)))
        })
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
        self.with_global_mut(|record| {
            let object = &record.object_record.binding_object;
            let has_own = object.get_own_property(&string_key(&name)).is_some();
            let extensible = object.is_extensible();
            if !has_own && extensible {
                define_or_error(
                    object,
                    string_key(&name),
                    PropertyDescriptor::data(Value::Undefined, true, true, deletable),
                )?;
            }
            if !has_own && !extensible {
                return Err(RuntimeError::type_error(
                    "global object cannot accept a new var binding",
                ));
            }
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
        self.with_global_mut(|record| {
            let object = &record.object_record.binding_object;
            let key = string_key(&name);
            let descriptor = match object.get_own_property(&key) {
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
            define_or_error(object, key.clone(), descriptor)?;
            if !object.set(&key, value)? {
                return Err(RuntimeError::type_error(
                    "global function binding could not be assigned",
                ));
            }
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
        self.with_global(|record| Ok(record.global_this_value.clone()))
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
            record.this_value = Some(value.clone());
            record.this_status = ThisBindingStatus::Initialized;
            Ok(value)
        })
    }

    /// Returns a function environment's initialized `this` binding.
    pub fn get_this_binding(&self) -> RuntimeResult<Value> {
        self.with_function(|record| match record.this_status {
            ThisBindingStatus::Initialized => Ok(record.this_value.clone().unwrap()),
            ThisBindingStatus::Uninitialized => Err(reference_error(
                &JsString::from("this"),
                "has not been initialized",
            )),
            ThisBindingStatus::Lexical => {
                Err(RuntimeError::type_error("function has lexical this"))
            }
        })
    }

    /// Returns the function object associated with a function environment.
    pub fn function_object(&self) -> RuntimeResult<Object> {
        self.with_function(|record| Ok(record.function_object.clone()))
    }

    /// Returns the function environment's `new.target`, if present.
    pub fn new_target(&self) -> RuntimeResult<Option<Object>> {
        self.with_function(|record| Ok(record.new_target.clone()))
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
        Rc::ptr_eq(&self.0, &other.0)
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

    fn initialize(&mut self, name: &JsString, value: Value) -> RuntimeResult<()> {
        let binding = self.binding_mut(name)?;
        if binding.value.is_some() {
            return Err(RuntimeError::type_error("binding is already initialized"));
        }
        binding.value = Some(value);
        Ok(())
    }

    fn set(&mut self, name: &JsString, value: Value, strict: bool) -> RuntimeResult<()> {
        self.binding_mut(name)?.write(value, strict)
    }

    fn get(&self, name: &JsString) -> RuntimeResult<Value> {
        self.bindings
            .iter()
            .find(|binding| binding.name == *name)
            .ok_or_else(|| reference_error(name, "is not defined"))?
            .read()
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
    fn read(&self) -> RuntimeResult<Value> {
        self.value
            .clone()
            .ok_or_else(|| reference_error(&self.name, "is in the temporal dead zone"))
    }

    fn write(&mut self, value: Value, strict: bool) -> RuntimeResult<()> {
        if self.value.is_none() {
            return Err(reference_error(&self.name, "is in the temporal dead zone"));
        }
        if self.mutable {
            self.value = Some(value);
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
    fn has_binding(&self, name: &JsString) -> RuntimeResult<bool> {
        let found = self.binding_object.has_property(&string_key(name))?;
        if !found || !self.options.with_environment {
            return Ok(found);
        }
        let Some(unscopables_key) = &self.options.unscopables_key else {
            return Ok(true);
        };
        let unscopables = self.binding_object.get(unscopables_key)?;
        if let Value::Object(unscopables) = unscopables {
            return Ok(!unscopables.get(&string_key(name))?.to_boolean());
        }
        Ok(true)
    }

    fn create_mutable(&self, name: JsString, deletable: bool) -> RuntimeResult<()> {
        define_or_error(
            &self.binding_object,
            string_key(&name),
            PropertyDescriptor::data(Value::Undefined, true, true, deletable),
        )
    }

    fn initialize(&self, name: &JsString, value: Value) -> RuntimeResult<()> {
        if self.binding_object.set(&string_key(name), value)? {
            Ok(())
        } else {
            Err(RuntimeError::type_error(
                "object binding could not be initialized",
            ))
        }
    }

    fn set(&self, name: &JsString, value: Value, strict: bool) -> RuntimeResult<()> {
        if self.binding_object.set(&string_key(name), value)? || !strict {
            Ok(())
        } else {
            Err(RuntimeError::type_error("object binding is not writable"))
        }
    }

    fn get(&self, name: &JsString, strict: bool) -> RuntimeResult<Value> {
        if !self.binding_object.has_property(&string_key(name))? {
            return if strict {
                Err(reference_error(name, "is not defined"))
            } else {
                Ok(Value::Undefined)
            };
        }
        self.binding_object.get(&string_key(name))
    }

    fn delete(&self, name: &JsString) -> bool {
        self.binding_object.delete(&string_key(name))
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
    fn stale_function_slot_hints_preserve_binding_identity_and_checks() {
        let env =
            Environment::function(None, Object::new(), None, ThisBindingStatus::Uninitialized);
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
