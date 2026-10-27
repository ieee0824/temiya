//! ECMAScript function objects, calls, constructors, and parameter binding.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::{
    ast::{Expr, ExprKind, Function, FunctionBody, MethodKind, Stmt, StmtKind},
    bytecode::{BytecodeCompiler, VerifiedBytecode, VirtualMachine},
};

thread_local! { static AUTO_LOOP_BYTECODE: Cell<bool> = const { Cell::new(false) }; }
#[cfg(test)]
thread_local! { static LOOP_VM_ENTRIES: Cell<usize> = const { Cell::new(0) }; }

pub(crate) fn with_auto_loop_bytecode<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            AUTO_LOOP_BYTECODE.with(|value| value.set(self.0));
        }
    }
    AUTO_LOOP_BYTECODE.with(|value| {
        let previous = value.replace(enabled);
        let _restore = Restore(previous);
        f()
    })
}

fn contains_own_loop(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::While { .. } | StmtKind::DoWhile { .. } | StmtKind::For { .. } => true,
        StmtKind::Block(items) => items.iter().any(contains_own_loop),
        StmtKind::If {
            consequent,
            alternate,
            ..
        } => contains_own_loop(consequent) || alternate.as_deref().is_some_and(contains_own_loop),
        StmtKind::Labeled { body, .. } => contains_own_loop(body),
        _ => false,
    }
}

use super::{
    Completion, Environment, ExpressionEvaluator, JsString, Object, PropertyKey, Realm,
    RuntimeError, RuntimeResult, StatementEvaluator, ThisBindingStatus, Value,
    generator::create_generator_object, promise::settled_promise_object,
};

/// Runtime behavior selected for a parsed function definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionKind {
    /// An ordinary function with dynamic `this` and `[[Construct]]`.
    Normal,
    /// An arrow function with lexical `this` and no `[[Construct]]`.
    Arrow,
    /// An object or class method.
    Method(MethodKind),
}

#[derive(Clone)]
pub(super) struct FunctionClosure {
    pub(super) code: Function,
    pub(super) kind: FunctionKind,
    pub(super) realm: Realm,
    pub(super) environment: Environment,
    pub(super) lexical_this: Value,
    loop_bytecode: RefCell<Option<Option<Rc<VerifiedBytecode>>>>,
    #[allow(dead_code)]
    home_object: Option<Object>,
}

pub(super) fn create_function(
    code: &Function,
    kind: FunctionKind,
    evaluator: &ExpressionEvaluator,
    home_object: Option<Object>,
) -> RuntimeResult<Object> {
    if code.asynchronous && code.generator {
        return Err(RuntimeError::internal_error(
            "async generators are not yet implemented",
        ));
    }
    let name_environment =
        if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
            let environment = Environment::declarative(Some(evaluator.environment.clone()));
            environment.create_immutable_binding(JsString::from(name.as_str()), false)?;
            Some(environment)
        } else {
            None
        };
    let closure = Rc::new(FunctionClosure {
        code: code.clone(),
        kind,
        realm: evaluator.realm.clone(),
        environment: name_environment
            .clone()
            .unwrap_or_else(|| evaluator.environment.clone()),
        lexical_this: evaluator.this_value.clone(),
        loop_bytecode: RefCell::new(None),
        home_object,
    });
    let slot = Rc::new(RefCell::new(None::<super::WeakObject>));
    let call_closure = closure.clone();
    let call_slot = slot.clone();
    let call = move |this: Value, arguments: &[Value]| {
        let function = call_slot
            .borrow()
            .as_ref()
            .and_then(super::WeakObject::upgrade)
            .ok_or_else(|| RuntimeError::internal_error("function object is not initialized"))?;
        if call_closure.code.generator {
            create_generator_object(&call_closure, function, this, arguments).map(Value::Object)
        } else if call_closure.code.asynchronous {
            settled_promise_object(
                &call_closure.realm,
                call_function(&call_closure, function, this, arguments, None)
                    .map_err(|error| call_closure.realm.bind_exception(error)),
            )
            .map(Value::Object)
        } else {
            call_function(&call_closure, function, this, arguments, None)
                .map_err(|error| call_closure.realm.bind_exception(error))
        }
    };
    let constructable = !code.generator
        && !code.asynchronous
        && matches!(
            kind,
            FunctionKind::Normal | FunctionKind::Method(MethodKind::Constructor)
        );
    let function = if constructable {
        let construct_closure = closure.clone();
        let construct_slot = slot.clone();
        Object::native_callable_constructor(call, move |arguments, new_target| {
            let function = construct_slot
                .borrow()
                .as_ref()
                .and_then(super::WeakObject::upgrade)
                .ok_or_else(|| {
                    RuntimeError::internal_error("function object is not initialized")
                })?;
            construct_function(&construct_closure, function, arguments, new_target)
                .map_err(|error| construct_closure.realm.bind_exception(error))
        })
    } else {
        Object::native_function(call)
    };
    *slot.borrow_mut() = Some(function.downgrade());
    function.initialize_prototype(Some(
        evaluator
            .realm
            .intrinsic(super::IntrinsicId::FunctionPrototype),
    ));
    if let (Some(environment), Some(ExprKind::Identifier(name))) =
        (name_environment, code.name.as_ref().map(|name| &name.kind))
    {
        environment.initialize_binding(
            &JsString::from(name.as_str()),
            Value::Object(function.clone()),
        )?;
    }

    let length = code
        .parameters
        .iter()
        .take_while(|parameter| {
            !matches!(
                parameter.kind,
                ExprKind::Assignment { .. } | ExprKind::Spread(_)
            )
        })
        .count();
    function.define_own_property(
        "length".into(),
        super::PropertyDescriptor::data(Value::Number(length as f64), false, false, true),
    )?;
    if let Some(ExprKind::Identifier(name)) = code.name.as_ref().map(|name| &name.kind) {
        function
            .create_data_property("name".into(), Value::String(JsString::from(name.as_str())))?;
    }
    if constructable {
        function.install_lazy_function_prototype(
            evaluator
                .realm
                .intrinsic(super::IntrinsicId::ObjectPrototype),
        )?;
    }
    Ok(function)
}

fn call_function(
    closure: &FunctionClosure,
    function: Object,
    this_argument: Value,
    arguments: &[Value],
    new_target: Option<Object>,
) -> RuntimeResult<Value> {
    let lexical = closure.kind == FunctionKind::Arrow;
    let this_value = if lexical {
        closure.lexical_this.clone()
    } else if closure.code.strict {
        this_argument
    } else {
        ordinary_this(&closure.realm, this_argument)?
    };
    let environment = Environment::function(
        Some(closure.environment.clone()),
        function,
        new_target.clone(),
        if lexical {
            ThisBindingStatus::Lexical
        } else {
            ThisBindingStatus::Uninitialized
        },
    );
    if !lexical {
        environment.bind_this_value(this_value.clone())?;
    }
    if !lexical {
        bind_arguments(&environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        this_value.clone(),
        closure.code.strict,
    )
    .with_new_target(new_target);
    bind_parameters(
        &evaluator,
        &environment,
        &closure.code.parameters,
        arguments,
    )?;

    if closure.kind == FunctionKind::Normal && AUTO_LOOP_BYTECODE.with(Cell::get) {
        let mut cache = closure.loop_bytecode.borrow_mut();
        if cache.is_none() {
            let has_loop =
                matches!(&closure.code.body, FunctionBody::Block(body) if contains_own_loop(body));
            *cache = Some(if has_loop {
                BytecodeCompiler::compile_prepared_function(&closure.code)
                    .ok()
                    .map(Rc::new)
            } else {
                None
            });
        }
        let bytecode = cache.as_ref().and_then(Clone::clone);
        drop(cache);
        if let Some(bytecode) = bytecode {
            #[cfg(test)]
            LOOP_VM_ENTRIES.with(|count| count.set(count.get() + 1));
            return match VirtualMachine::execute_in_function(
                &bytecode,
                &closure.realm,
                &environment,
            )? {
                Completion::Return(value) => Ok(value),
                Completion::Normal(_) => Ok(Value::Undefined),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                _ => Err(RuntimeError::internal_error(
                    "invalid completion escaped a bytecode function",
                )),
            };
        }
    }

    match &closure.code.body {
        FunctionBody::Expression(expression) => evaluator.evaluate(expression),
        FunctionBody::Block(body) => {
            let statements = StatementEvaluator::with_environments(
                closure.realm.clone(),
                environment.clone(),
                environment,
                this_value,
                closure.code.strict,
            );
            match statements.evaluate_statement(body)? {
                Completion::Return(value) => Ok(value),
                Completion::Normal(_) => Ok(Value::Undefined),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                Completion::Break { .. } | Completion::Continue { .. } => Err(
                    RuntimeError::internal_error("invalid completion escaped a function body"),
                ),
            }
        }
    }
}

fn construct_function(
    closure: &FunctionClosure,
    function: Object,
    arguments: &[Value],
    new_target: Object,
) -> RuntimeResult<Object> {
    let prototype = match new_target.get(&PropertyKey::from("prototype"))? {
        Value::Object(prototype) => Some(prototype),
        _ => Some(closure.realm.intrinsic(super::IntrinsicId::ObjectPrototype)),
    };
    let instance = Object::new();
    if let Some(prototype) = prototype {
        instance.initialize_prototype(Some(prototype));
    }
    let result = call_function(
        closure,
        function,
        Value::Object(instance.clone()),
        arguments,
        Some(new_target),
    )?;
    match result {
        Value::Object(object) => Ok(object),
        _ => Ok(instance),
    }
}

pub(super) fn ordinary_this(realm: &Realm, value: Value) -> RuntimeResult<Value> {
    match value {
        Value::Undefined | Value::Null => Ok(Value::Object(realm.global_object())),
        Value::Object(_) => Ok(value),
        primitive => {
            let object = Object::new();
            object.create_data_property("[[PrimitiveValue]]".into(), primitive)?;
            Ok(Value::Object(object))
        }
    }
}

pub(super) fn bind_arguments(environment: &Environment, arguments: &[Value]) -> RuntimeResult<()> {
    let object = Object::new();
    for (index, value) in arguments.iter().enumerate() {
        object.create_data_property(
            PropertyKey::String(JsString::from(index.to_string())),
            value.clone(),
        )?;
    }
    object.create_data_property("length".into(), Value::Number(arguments.len() as f64))?;
    let name = JsString::from("arguments");
    environment.create_mutable_binding(name.clone(), false)?;
    environment.initialize_binding(&name, Value::Object(object))
}

pub(super) fn bind_parameters(
    evaluator: &ExpressionEvaluator,
    environment: &Environment,
    parameters: &[Expr],
    arguments: &[Value],
) -> RuntimeResult<()> {
    let mut index = 0;
    for parameter in parameters {
        match &parameter.kind {
            ExprKind::Spread(binding) => {
                let rest = Object::new();
                for (rest_index, value) in arguments[index..].iter().enumerate() {
                    rest.create_data_property(
                        PropertyKey::String(JsString::from(rest_index.to_string())),
                        value.clone(),
                    )?;
                }
                rest.create_data_property(
                    "length".into(),
                    Value::Number(arguments.len().saturating_sub(index) as f64),
                )?;
                bind_parameter(environment, binding, Value::Object(rest))?;
                index = arguments.len();
            }
            ExprKind::Assignment { target, value, .. } => {
                let argument = arguments.get(index).cloned().unwrap_or(Value::Undefined);
                let argument = if matches!(argument, Value::Undefined) {
                    evaluator.evaluate(value)?
                } else {
                    argument
                };
                bind_parameter(environment, target, argument)?;
                index += 1;
            }
            _ => {
                bind_parameter(
                    environment,
                    parameter,
                    arguments.get(index).cloned().unwrap_or(Value::Undefined),
                )?;
                index += 1;
            }
        }
    }
    Ok(())
}

fn bind_parameter(environment: &Environment, binding: &Expr, value: Value) -> RuntimeResult<()> {
    let ExprKind::Identifier(name) = &binding.kind else {
        return Err(RuntimeError::internal_error(
            "destructuring parameter binding is not implemented",
        ));
    };
    let name = JsString::from(name.as_str());
    if environment.has_binding(&name)? {
        environment.set_mutable_binding(&name, value, false)
    } else {
        environment.create_mutable_binding(name.clone(), false)?;
        environment.initialize_binding(&name, value)
    }
}

#[cfg(test)]
mod auto_loop_tests {
    use super::LOOP_VM_ENTRIES;
    use crate::{Engine, embedding::ScriptBackend, runtime::Value};

    #[test]
    fn auto_executes_supported_loop_body_and_falls_back_before_effects() {
        LOOP_VM_ENTRIES.with(|count| count.set(0));
        let engine = Engine::builder()
            .script_backend(ScriptBackend::Auto)
            .build();
        let realm = engine.create_realm();
        // Object literal forces setup through the AST; function is created there.
        let setup = engine.compile(&realm, "var o = {}; function sum(n) { var x = 0; for (var i = 0; i < n; i++) x += i; return x; } function fallback() { var x = 0; while (x < 2) { x++; } try { return x; } finally {} }").unwrap();
        assert_eq!(setup.backend(), ScriptBackend::Ast);
        engine.evaluate(&setup).unwrap();
        let script = engine.compile(&realm, "sum(10);").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Number(45.0)
        ));
        assert_eq!(LOOP_VM_ENTRIES.with(|count| count.get()), 1);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Number(45.0)
        ));
        assert_eq!(LOOP_VM_ENTRIES.with(|count| count.get()), 2);
        let script = engine.compile(&realm, "fallback();").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Number(2.0)
        ));
        assert_eq!(LOOP_VM_ENTRIES.with(|count| count.get()), 2);
    }

    #[test]
    fn execution_policy_is_restored_after_nested_calls_and_panics() {
        use super::{AUTO_LOOP_BYTECODE, with_auto_loop_bytecode};
        with_auto_loop_bytecode(true, || {
            assert!(AUTO_LOOP_BYTECODE.with(std::cell::Cell::get));
            let _ = std::panic::catch_unwind(|| with_auto_loop_bytecode(false, || panic!("test")));
            assert!(AUTO_LOOP_BYTECODE.with(std::cell::Cell::get));
        });
        assert!(!AUTO_LOOP_BYTECODE.with(std::cell::Cell::get));
    }

    #[test]
    #[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
    fn pinned_test262_original_function_loops() {
        use crate::{
            runtime::{Completion, Realm, ScriptRecord},
            test262::{TEST262_REVISION, TestCase},
        };
        use std::process::Command;
        let root = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
        let git = |args: &[&str]| {
            let output = Command::new("git")
                .args(["-c", &format!("safe.directory={root}"), "-C", &root])
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        };
        assert_eq!(git(&["rev-parse", "HEAD"]).trim(), TEST262_REVISION);
        let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
        let cases = [
            "test/language/block-scope/leave/for-loop-block-let-declaration-only-shadows-outer-parameter-value-1.js",
            "test/language/block-scope/leave/verify-context-in-for-loop-block.js",
            "test/language/statements/return/12.9-1.js",
        ];
        let mut failures = Vec::new();
        for path in cases {
            let case = TestCase::parse(path, read(path)).unwrap();
            let blob = git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")]);
            for variant in case.variants() {
                for auto in [false, true] {
                    let realm = Realm::ordinary();
                    for harness in ["sta.js", "assert.js"]
                        .into_iter()
                        .chain(case.metadata.includes.iter().map(String::as_str))
                    {
                        let source = read(&format!("harness/{harness}"));
                        let result = ScriptRecord::parse(realm.clone(), &source)
                            .unwrap()
                            .evaluate()
                            .unwrap();
                        assert!(
                            matches!(result, Completion::Normal(_)),
                            "harness {harness}: {result:?}"
                        );
                    }
                    let script = ScriptRecord::parse(realm, &case.source_for(variant)).unwrap();
                    LOOP_VM_ENTRIES.with(|count| count.set(0));
                    let result = super::with_auto_loop_bytecode(auto, || script.evaluate());
                    let entries = LOOP_VM_ENTRIES.with(|count| count.get());
                    println!(
                        "{path} blob={} {variant:?} auto={auto} vm_entries={entries} result={result:?}",
                        blob.trim()
                    );
                    if !matches!(result, Ok(Completion::Normal(_)))
                        || (auto && entries == 0)
                        || (!auto && entries != 0)
                    {
                        failures.push(format!("{path} {variant:?} auto={auto}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:?}");
    }
}
