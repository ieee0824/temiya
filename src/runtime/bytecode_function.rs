//! Bytecode-backed function objects. No AST body evaluation occurs here.
use super::{
    environment::{FunctionThis, StoredEnvironment},
    execution::StoredRealm,
};
use std::rc::Rc;

use crate::bytecode::{Constant, FunctionTemplate, Instruction, VirtualMachine};

use super::{
    Completion, Environment, IntrinsicId, Object, PropertyDescriptor, Realm, RuntimeError,
    RuntimeResult, Value,
};

struct Closure {
    template: Rc<FunctionTemplate>,
    realm: Realm,
    environment: Environment,
}

pub(super) struct StoredClosure {
    template: Rc<FunctionTemplate>,
    realm: StoredRealm,
    environment: StoredEnvironment,
}
impl StoredClosure {
    pub(super) fn source_realm(&self) -> &StoredRealm {
        &self.realm
    }
    fn load(&self, owner: &Object) -> RuntimeResult<Closure> {
        Ok(Closure {
            template: self.template.clone(),
            realm: self.realm.load(owner)?,
            environment: self.environment.load(owner)?,
        })
    }
    pub(super) fn is_constructor(&self) -> bool {
        !self.template.arrow
    }
    pub(super) fn call(
        &self,
        owner: &Object,
        this: Value,
        arguments: &[Value],
    ) -> RuntimeResult<Value> {
        let mut environment = None;
        if !self.template.arguments_object {
            if let Some(value) = self.template.body.try_numeric_return(
                &self.template.parameters,
                arguments,
                |name| {
                    if environment.is_none() {
                        environment = Some(self.environment.load(owner).ok()?);
                    }
                    environment.as_ref()?.try_pure_number_binding(name)
                },
            ) {
                return Ok(value);
            }
        }
        let closure = Closure {
            template: self.template.clone(),
            realm: self.realm.load(owner)?,
            environment: match environment {
                Some(environment) => environment,
                None => self.environment.load(owner)?,
            },
        };
        closure
            .call_fallback(owner.clone(), this, arguments, None)
            .map_err(|error| closure.realm.bind_exception(error))
    }
    pub(super) fn construct(
        &self,
        owner: &Object,
        arguments: &[Value],
        new_target: Object,
    ) -> RuntimeResult<Object> {
        let closure = self.load(owner)?;
        let instance = Object::new_for(&new_target);
        let prototype = match new_target.get(&"prototype".into())? {
            Value::Object(prototype) => prototype,
            _ => closure.realm.intrinsic(IntrinsicId::ObjectPrototype),
        };
        instance.initialize_prototype(Some(prototype));
        let value = closure
            .call(
                owner.clone(),
                Value::Object(instance.clone()),
                arguments,
                Some(new_target),
            )
            .map_err(|error| closure.realm.bind_exception(error))?;
        Ok(match value {
            Value::Object(object) => object,
            _ => instance,
        })
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        // FunctionTemplate constants are immutable, pointer-free code literals.
        self.realm.trace(tracer);
        self.environment.trace(tracer);
    }
}

pub(crate) fn create(
    template: Rc<FunctionTemplate>,
    realm: &Realm,
    environment: Environment,
) -> RuntimeResult<Object> {
    let named = template
        .name
        .as_ref()
        .filter(|_| template.self_binding)
        .map(|name| {
            let scope = Environment::declarative(environment.clone());
            scope.create_immutable_binding(name.clone(), false)?;
            Ok::<_, RuntimeError>(scope)
        })
        .transpose()?;
    let function = Object::new_for(&realm.global_object());
    let closure = StoredClosure {
        template: template.clone(),
        realm: StoredRealm::store(&function, realm)?,
        environment: StoredEnvironment::store(&function, named.as_ref().unwrap_or(&environment))?,
    };
    function.set_managed_callable(super::object::ManagedCallable::Bytecode(closure));
    function.initialize_prototype(Some(realm.intrinsic(IntrinsicId::FunctionPrototype)));
    for (name, value) in [
        ("length", Value::Number(template.parameters.len() as f64)),
        (
            "name",
            Value::String(template.name.clone().unwrap_or_default()),
        ),
    ] {
        function.define_own_property(
            name.into(),
            PropertyDescriptor::data(value, false, false, true),
        )?;
    }
    if !template.arrow {
        function.install_lazy_function_prototype(realm.intrinsic(IntrinsicId::ObjectPrototype))?;
    }
    if let Some(named) = named {
        named.initialize_binding(
            template.name.as_ref().expect("named template"),
            Value::Object(function.clone()),
        )?;
    }
    Ok(function)
}

impl Closure {
    fn call(
        &self,
        function: Object,
        this: Value,
        arguments: &[Value],
        new_target: Option<Object>,
    ) -> RuntimeResult<Value> {
        if !self.template.arguments_object {
            if let Some(value) = self.template.body.try_numeric_return(
                &self.template.parameters,
                arguments,
                |name| self.environment.try_pure_number_binding(name),
            ) {
                return Ok(value);
            }
        }
        self.call_fallback(function, this, arguments, new_target)
    }

    fn call_fallback(
        &self,
        function: Object,
        this: Value,
        arguments: &[Value],
        new_target: Option<Object>,
    ) -> RuntimeResult<Value> {
        let this = if self.template.arrow {
            FunctionThis::Lexical
        } else {
            FunctionThis::Initialized(if self.template.strict {
                this
            } else if matches!(this, Value::Undefined | Value::Null) {
                self.realm.global_this_value()?
            } else {
                boxed_this(&self.realm, this)?
            })
        };
        let frame = if !self.template.arguments_object {
            let body = self.template.body.bytecode();
            let capacity = match body.instructions.first() {
                Some(Instruction::EnterScope(index)) => match &body.constants[*index as usize] {
                    Constant::Scope(scope) => scope.bindings.len(),
                    _ => 0,
                },
                _ => 0,
            };
            Environment::function_with_parameters(
                Some(self.environment.clone()),
                function,
                new_target,
                this,
                &self.template.parameters,
                arguments,
                capacity,
            )
        } else {
            Environment::function_with_parameters(
                Some(self.environment.clone()),
                function,
                new_target,
                this,
                &[],
                &[],
                0,
            )
        };
        if !self.template.arrow && self.template.arguments_object {
            super::function::bind_arguments(&self.realm, &frame, arguments)?;
        }
        if self.template.arguments_object {
            for (index, name) in self.template.parameters.iter().enumerate() {
                let argument = arguments.get(index).cloned().unwrap_or(Value::Undefined);
                if frame.has_binding(name)? {
                    frame.set_mutable_binding(name, argument, false)?;
                } else {
                    frame.create_mutable_binding(name.clone(), false)?;
                    frame.initialize_binding(name, argument)?;
                }
            }
        }
        match VirtualMachine::execute_in_function(&self.template.body, &self.realm, &frame)? {
            Completion::Return(value) => Ok(value),
            Completion::Normal(_) => Ok(Value::Undefined),
            Completion::Throw(value) => Err(RuntimeError::thrown(value)),
            _ => Err(RuntimeError::internal_error(
                "invalid bytecode function completion",
            )),
        }
    }
}

fn boxed_this(realm: &Realm, value: Value) -> RuntimeResult<Value> {
    match value {
        Value::Object(_) => Ok(value),
        value => super::function::box_primitive(realm, value).map(Value::Object),
    }
}

#[cfg(test)]
mod numeric_return_tests {
    use super::*;
    use crate::{bytecode::BytecodeCompiler, runtime::Runtime};

    #[cfg(not(miri))]
    #[test]
    fn parameter_only_numeric_calls_do_not_revive_unused_realm_or_environment_roots() {
        let runtime = Runtime::new();
        let realm = Realm::ordinary_in(&runtime);
        let code = BytecodeCompiler::compile("(function(x){return x+2;});").unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        drop(realm);
        runtime.collect_garbage().unwrap();
        let (_, before, _) = runtime.profile_snapshot();
        for _ in 0..100 {
            assert!(
                function
                    .call(Value::Undefined, &[Value::Number(40.0)])
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
        }
        let (_, after, _) = runtime.profile_snapshot();
        assert_eq!(after, before);
        drop(function);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn pure_numeric_calls_allocate_no_managed_frames_and_code_keeps_no_runtime_edges() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let realm = Realm::ordinary_in(&runtime);
        let code =
            BytecodeCompiler::compile("let make=(n)=>function(x){return x+n;};make(2);").unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        runtime.collect_garbage().unwrap();
        let before = runtime.diagnostics();
        for value in 0..100 {
            assert!(
                function
                    .call(Value::Number(-0.0), &[Value::Number(value as f64)])
                    .unwrap()
                    .same_value(&Value::Number(value as f64 + 2.0))
            );
        }
        let after = runtime.diagnostics();
        assert_eq!(after.live, before.live);
        assert_eq!(after.collections, before.collections);
        assert_eq!(after.rooted, before.rooted);
        drop((function, realm));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        // Keep the compiled templates/plans alive through this collection.
        assert!(!code.bytecode().instructions.is_empty());
    }
}
