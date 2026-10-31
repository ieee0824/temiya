//! Bytecode-backed functions and shared class/object-definition bridges.
use super::{
    environment::{FunctionThis, StoredEnvironment},
    execution::StoredRealm,
};
use std::{cell::OnceCell, fmt, rc::Rc};

use crate::bytecode::{Constant, FunctionTemplate, Instruction, VirtualMachine};

use super::{
    Completion, Environment, IntrinsicId, Object, Realm, RuntimeError, RuntimeResult, Value,
};

struct Closure {
    template: Rc<FunctionTemplate>,
    realm: Realm,
    environment: Environment,
}

// Only immutable syntax/code is retained. Nested definition caches form the
// finite syntax tree; FunctionCode's link to its parent cache remains weak.
#[derive(Clone, Default)]
pub(crate) struct DefinitionCodeCache(Rc<OnceCell<super::function::FunctionCache>>);

impl fmt::Debug for DefinitionCodeCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DefinitionCodeCache(..)")
    }
}

impl PartialEq for DefinitionCodeCache {
    fn eq(&self, _: &Self) -> bool {
        // Cache population does not change verified code equality.
        true
    }
}

impl DefinitionCodeCache {
    pub(crate) fn context(&self) -> DefinitionEvaluationContext {
        DefinitionEvaluationContext {
            cache: Some(self.0.get_or_init(Default::default).clone()),
        }
    }
}

#[derive(Default)]
pub(crate) struct DefinitionEvaluationContext {
    cache: Option<super::function::FunctionCache>,
}

impl DefinitionEvaluationContext {
    pub(crate) fn evaluate(
        &mut self,
        class: &crate::ast::Class,
        inferred_name: Option<super::JsString>,
        realm: &Realm,
        environment: &Environment,
    ) -> RuntimeResult<Object> {
        let evaluator = self.evaluator(realm, environment, true)?;
        super::class::evaluate(class, &evaluator, inferred_name)
    }

    pub(crate) fn evaluate_object_literal(
        &mut self,
        expression: &crate::ast::Expr,
        strict: bool,
        realm: &Realm,
        environment: &Environment,
    ) -> RuntimeResult<Value> {
        self.evaluator(realm, environment, strict)?
            .evaluate(expression)
    }

    // Called only by the verified short-function executor: its empty-frame
    // proof excludes lexical this/new.target/super, arrows and direct eval
    // throughout the metadata expression. Ordinary methods establish their
    // own call frame when invoked and still capture this environment.
    pub(crate) fn evaluate_object_literal_without_frame_observers(
        &mut self,
        expression: &crate::ast::Expr,
        strict: bool,
        realm: &Realm,
        environment: &Environment,
    ) -> RuntimeResult<Value> {
        super::ExpressionEvaluator::with_environment(
            realm.clone(),
            environment.clone(),
            Value::Undefined,
            strict,
        )
        .with_function_cache(Some(
            self.cache.get_or_insert_with(Default::default).clone(),
        ))
        .evaluate(expression)
    }

    fn evaluator(
        &mut self,
        realm: &Realm,
        environment: &Environment,
        strict: bool,
    ) -> RuntimeResult<super::ExpressionEvaluator> {
        let mut current = Some(environment.clone());
        let mut this_environment = None;
        let mut this_value = Value::Undefined;
        let mut new_target = None;
        while let Some(scope) = current {
            if scope.kind() == super::EnvironmentKind::Function && scope.has_this_binding()? {
                // Do not read this eagerly: a prepared derived frame may still
                // have an uninitialized binding that the definition never touches.
                new_target = scope.new_target()?;
                this_environment = Some(scope);
                break;
            }
            if scope.kind() == super::EnvironmentKind::Global {
                this_value = scope.global_this_value()?;
                break;
            }
            current = scope.outer();
        }
        Ok(super::ExpressionEvaluator::with_environment(
            realm.clone(),
            environment.clone(),
            this_value,
            strict,
        )
        .with_this_environment(this_environment)
        .with_new_target(new_target)
        .with_function_cache(Some(
            self.cache.get_or_insert_with(Default::default).clone(),
        )))
    }
}

pub(super) fn execute_template(
    template: Rc<FunctionTemplate>,
    realm: Realm,
    environment: Environment,
    function: Object,
    this: Value,
    arguments: &[Value],
    new_target: Option<Object>,
) -> RuntimeResult<Value> {
    #[cfg(test)]
    BRIDGED_CALLS.with(|count| count.set(count.get() + 1));
    Closure {
        template,
        realm,
        environment,
    }
    .call(function, this, arguments, new_target)
}

#[cfg(test)]
thread_local! {
    static BRIDGED_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static EMPTY_FRAME_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(super) fn bridged_calls() -> usize {
    BRIDGED_CALLS.with(std::cell::Cell::get)
}

// Handles are already traced from owner. Reusing the stored-call path avoids
// reviving an unused outer Environment for AST-created compiled methods.
pub(super) fn execute_stored_template(
    template: &Rc<FunctionTemplate>,
    realm: &StoredRealm,
    environment: &StoredEnvironment,
    owner: &Object,
    this: Value,
    arguments: &[Value],
) -> RuntimeResult<Value> {
    #[cfg(test)]
    BRIDGED_CALLS.with(|count| count.set(count.get() + 1));
    StoredClosure {
        template: template.clone(),
        realm: realm.clone(),
        environment: environment.clone(),
    }
    .call(owner, this, arguments)
}

pub(super) struct StoredClosure {
    template: Rc<FunctionTemplate>,
    realm: StoredRealm,
    environment: StoredEnvironment,
}
impl StoredClosure {
    pub(super) fn has_legacy_properties(&self) -> bool {
        !self.template.strict && !self.template.arrow
    }
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
        let realm = self.realm.load(owner)?;
        if !self.template.arrow
            && !self.template.arguments_object
            && self.template.body.has_leaf_function()
        {
            let receiver = if self.template.strict {
                this.clone()
            } else {
                super::function::ordinary_this(&realm, this.clone())
                    .map_err(|error| realm.bind_exception(error))?
            };
            if self.template.body.leaf_needs_environment() && environment.is_none() {
                environment = Some(self.environment.load(owner)?);
            }
            if let Some(result) = self.template.body.try_leaf_function(
                &self.template.parameters,
                arguments,
                &receiver,
                &realm,
                owner,
                environment.as_ref(),
            ) {
                return result.map_err(|error| realm.bind_exception(error));
            }
        }
        let closure = Closure {
            template: self.template.clone(),
            realm,
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
        let prototype = new_target
            .constructor_prototype(super::IntrinsicId::ObjectPrototype, || {
                Ok(closure.realm.intrinsic(IntrinsicId::ObjectPrototype))
            })?;
        let instance = Object::new_for(&new_target);
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
    let function = Object::new_with_prototype(realm.intrinsic(IntrinsicId::FunctionPrototype));
    let closure = StoredClosure {
        template: template.clone(),
        realm: StoredRealm::store(&function, realm)?,
        environment: StoredEnvironment::store(&function, named.as_ref().unwrap_or(&environment))?,
    };
    function.set_managed_callable(super::object::ManagedCallable::Bytecode(closure));
    function.initialize_function_properties(
        template.parameters.len(),
        template.name.clone().unwrap_or_default(),
        (!template.arrow).then(|| realm.intrinsic(IntrinsicId::ObjectPrototype)),
    )?;
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
        self.call_leaf_or_fallback(function, this, arguments, new_target)
    }

    fn call_leaf_or_fallback(
        &self,
        function: Object,
        this: Value,
        arguments: &[Value],
        new_target: Option<Object>,
    ) -> RuntimeResult<Value> {
        if !self.template.arrow
            && !self.template.arguments_object
            && self.template.body.has_leaf_function()
        {
            // Normalize only an eligible leaf call. Preserve the original
            // receiver for fallback, including host-supplied global this values.
            let receiver = if self.template.strict {
                this.clone()
            } else {
                super::function::ordinary_this(&self.realm, this.clone())?
            };
            if let Some(result) = self.template.body.try_leaf_function(
                &self.template.parameters,
                arguments,
                &receiver,
                &self.realm,
                &function,
                Some(&self.environment),
            ) {
                // Function, newTarget, receiver and outer-environment roots
                // remain live during property/coercion callbacks. Never retry.
                return result;
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
        if new_target.is_none()
            && !self.template.arrow
            && !self.template.arguments_object
            && self.template.parameters.is_empty()
            && self.template.body.can_elide_empty_function_frame()
        {
            #[cfg(test)]
            EMPTY_FRAME_CALLS.with(|count| count.set(count.get() + 1));
            let result = VirtualMachine::execute_without_own_frame(
                &self.template.body,
                &self.realm,
                &self.environment,
            )?;
            // Preserve receiver boxing and the live function root across every
            // callback, even though their empty environment is unobservable.
            drop(this);
            drop(function);
            return match result {
                Completion::Return(value) => Ok(value),
                Completion::Normal(_) => Ok(Value::Undefined),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                _ => Err(RuntimeError::internal_error(
                    "invalid bytecode function completion",
                )),
            };
        }
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
        let scalar_bindings_unobserved = !self.template.arguments_object
            && arguments
                .iter()
                .all(|value| matches!(value, Value::Number(_)));
        let result = VirtualMachine::execute_with_private_frame(
            &self.template.body,
            &self.realm,
            &frame,
            scalar_bindings_unobserved,
        )?;
        match result {
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

    #[cfg(not(miri))]
    #[test]
    fn property_only_ast_methods_do_not_revive_unused_outer_environments() {
        let runtime = Runtime::new();
        let realm = Realm::ordinary_in(&runtime);
        let code = BytecodeCompiler::compile(
            "function make(){return {read(){return this.value;}}.read;}make();",
        )
        .unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("method")
        };
        let receiver = Object::new_in(&runtime);
        receiver
            .create_data_property("value".into(), Value::Number(42.0))
            .unwrap();
        runtime.collect_garbage().unwrap();
        let (_, before, _) = runtime.profile_snapshot();
        for _ in 0..100 {
            assert!(
                function
                    .call(Value::Object(receiver.clone()), &[])
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
        }
        let (_, after, _) = runtime.profile_snapshot();
        assert_eq!(
            after, before,
            "unused stored environment must not create transient roots"
        );
        drop((function, receiver, realm));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn unobserved_object_literal_loop_headers_do_not_allocate_per_iteration() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let realm = Realm::ordinary_in(&runtime);
        let code=BytecodeCompiler::compile("(function(n){let object={value:1,read(){return this.value;}};let total=0;for(let i=0;i<n;i++)total+=object.read();return total;});").unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        let before = runtime.diagnostics().live;
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(3.0)])
                .unwrap()
                .same_value(&Value::Number(3.0))
        );
        let short = runtime.diagnostics().live - before;
        let count = if cfg!(miri) { 5.0 } else { 30.0 };
        let before = runtime.diagnostics().live;
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(count)])
                .unwrap()
                .same_value(&Value::Number(count))
        );
        let long = runtime.diagnostics().live - before;
        assert_eq!(
            long, short,
            "unobserved header must not allocate on every iteration"
        );
        drop((function, realm));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn unobserved_field_class_loop_headers_do_not_allocate_per_iteration() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let realm = Realm::ordinary_in(&runtime);
        let code=BytecodeCompiler::compile("(function(n){let C=class{value=1;};let object=new C();let total=0;for(let i=0;i<n;i++)total+=object.value;return total;});").unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        let before = runtime.diagnostics().live;
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(3.0)])
                .unwrap()
                .same_value(&Value::Number(3.0))
        );
        let short = runtime.diagnostics().live - before;
        let count = if cfg!(miri) { 5.0 } else { 30.0 };
        let before = runtime.diagnostics().live;
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(count)])
                .unwrap()
                .same_value(&Value::Number(count))
        );
        let long = runtime.diagnostics().live - before;
        assert_eq!(
            long, short,
            "unobserved field metadata must not allocate loop headers per iteration"
        );
        drop((function, realm));
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

#[cfg(test)]
mod empty_frame_tests {
    use super::EMPTY_FRAME_CALLS;
    use crate::{
        Engine,
        embedding::ScriptBackend,
        runtime::{Object, Value},
    };

    #[test]
    fn fresh_object_factories_really_elide_empty_frames() {
        let engine = Engine::default();
        {
            let realm = engine.create_realm();
            let before = EMPTY_FRAME_CALLS.with(|count| count.get());
            let script=engine.compile(&realm,"function make(){return {next(){return 0;},return(){throw 99;}};}let record=make();record.next()===0;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
            assert!(EMPTY_FRAME_CALLS.with(|count| count.get()) > before);
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn captured_mutations_and_activation_observers_survive_reentrant_gc() {
        for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
            let engine = Engine::builder().script_backend(backend).build();
            {
                let realm = engine.create_realm();
                let gc = Object::native_function_with_context(
                    engine.allocation_context(),
                    |context, _, _| {
                        context.collect_garbage()?;
                        Ok(Value::Undefined)
                    },
                );
                engine
                    .set_global(
                        &realm,
                        "gcNow",
                        engine.value(&realm, Value::Object(gc)).unwrap(),
                    )
                    .unwrap();
                let source = "let state=0;let reads=0;let marker={};function make(v){return {get value(){gcNow();if(state!==0)throw marker;state=1;reads++;return v;}};}let a=make(20);let b=make(22);let first=a.value;state=0;let second=b.value;state=0;function own(){return {read:()=>this.answer};}let receiver={answer:42};function args(){return {read:()=>arguments[0]};}function construct(){return {answer:new.target};}function local(){let value=41;return {get next(){gcNow();return ++value;}};}let x=local();let y=local();let caught=false;state=1;try{a.value;}catch(e){caught=e===marker;}first+second===42&&reads===2&&own.call(receiver).read()===42&&args(42).read()===42&&new construct().answer===construct&&x.next===42&&y.next===42&&x.next===43&&caught;";
                let script = engine.compile(&realm, source).unwrap();
                assert!(
                    matches!(
                        engine.evaluate(&script).unwrap().value(),
                        Value::Boolean(true)
                    ),
                    "{backend:?}"
                );
            }
            assert_eq!(engine.collect_garbage().unwrap().retained, 0);
        }
    }
}
