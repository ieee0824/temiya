use std::cell::RefCell;
use std::rc::Rc;

use temiya::runtime::{
    Environment, EnvironmentKind, JsString, Object, ObjectEnvironmentOptions, PropertyDescriptor,
    PropertyKey, RuntimeErrorKind, Symbol, ThisBindingStatus, Value,
};

fn name(value: &str) -> JsString {
    JsString::from(value)
}

fn assert_number(value: Value, expected: f64) {
    assert!(value.same_value(&Value::Number(expected)), "{value:?}");
}

#[test]
fn declarative_bindings_enforce_tdz_mutability_and_deletion() {
    let context = temiya::runtime::AllocationContext::new();
    let environment = Environment::declarative_in(&context);
    assert_eq!(environment.kind(), EnvironmentKind::Declarative);
    environment
        .create_mutable_binding(name("letValue"), true)
        .unwrap();
    assert_eq!(
        environment
            .get_binding_value(&name("letValue"), true)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
    assert_eq!(
        environment
            .set_mutable_binding(&name("letValue"), Value::Number(1.0), false)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
    environment
        .initialize_binding(&name("letValue"), Value::Number(1.0))
        .unwrap();
    environment
        .set_mutable_binding(&name("letValue"), Value::Number(2.0), true)
        .unwrap();
    assert_number(
        environment
            .get_binding_value(&name("letValue"), true)
            .unwrap(),
        2.0,
    );
    assert!(environment.delete_binding(&name("letValue")).unwrap());

    environment
        .create_immutable_binding(name("constant"), true)
        .unwrap();
    environment
        .initialize_binding(&name("constant"), Value::Number(3.0))
        .unwrap();
    assert_eq!(
        environment
            .set_mutable_binding(&name("constant"), Value::Number(4.0), false)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(!environment.delete_binding(&name("constant")).unwrap());
    assert!(
        environment
            .create_mutable_binding(name("constant"), false)
            .is_err()
    );

    environment
        .create_immutable_binding(name("sloppyConstant"), false)
        .unwrap();
    environment
        .initialize_binding(&name("sloppyConstant"), Value::Number(5.0))
        .unwrap();
    environment
        .set_mutable_binding(&name("sloppyConstant"), Value::Number(6.0), false)
        .unwrap();
    assert_number(
        environment
            .get_binding_value(&name("sloppyConstant"), false)
            .unwrap(),
        5.0,
    );
    assert!(
        environment
            .set_mutable_binding(&name("sloppyConstant"), Value::Number(6.0), true)
            .is_err()
    );
}

#[test]
fn environment_chains_resolve_nearest_binding() {
    let context = temiya::runtime::AllocationContext::new();
    let outer = Environment::declarative_in(&context);
    outer.create_mutable_binding(name("x"), false).unwrap();
    outer
        .initialize_binding(&name("x"), Value::Number(1.0))
        .unwrap();
    let inner = Environment::declarative(outer.clone());
    inner.create_mutable_binding(name("x"), false).unwrap();
    inner
        .initialize_binding(&name("x"), Value::Number(2.0))
        .unwrap();

    assert_eq!(inner.outer(), Some(outer));
    assert_eq!(
        inner.resolve_binding(&name("x")).unwrap(),
        Some(inner.clone())
    );
    assert_number(inner.get_identifier_value(&name("x"), true).unwrap(), 2.0);
    assert_eq!(
        inner
            .get_identifier_value(&name("missing"), true)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
}

#[test]
fn object_environment_maps_bindings_to_properties() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    let environment =
        Environment::object(object.clone(), None, ObjectEnvironmentOptions::default());
    environment
        .create_mutable_binding(name("property"), true)
        .unwrap();
    environment
        .initialize_binding(&name("property"), Value::Number(1.0))
        .unwrap();
    environment
        .set_mutable_binding(&name("property"), Value::Number(2.0), true)
        .unwrap();
    assert_number(object.get(&PropertyKey::from("property")).unwrap(), 2.0);
    assert!(environment.delete_binding(&name("property")).unwrap());
    assert!(!environment.has_binding(&name("property")).unwrap());
    assert!(
        environment
            .get_binding_value(&name("property"), false)
            .unwrap()
            .same_value(&Value::Undefined)
    );

    object
        .define_own_property(
            "locked".into(),
            PropertyDescriptor::data(Value::Undefined, false, true, false),
        )
        .unwrap();
    assert_eq!(
        environment
            .set_mutable_binding(&name("locked"), Value::Number(1.0), true)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn with_environment_honors_unscopables() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let unscopables_symbol = Symbol::new(Some(name("Symbol.unscopables")));
    let object = Object::new_in(&allocation_context);
    object
        .create_data_property("visible".into(), Value::Number(1.0))
        .unwrap();
    object
        .create_data_property("hidden".into(), Value::Number(2.0))
        .unwrap();
    let unscopables = Object::new_in(&allocation_context);
    unscopables
        .create_data_property("hidden".into(), Value::Boolean(true))
        .unwrap();
    object
        .create_data_property(
            PropertyKey::from(unscopables_symbol.clone()),
            Value::Object(unscopables),
        )
        .unwrap();
    let environment = Environment::object(
        object,
        None,
        ObjectEnvironmentOptions {
            with_environment: true,
            unscopables_key: Some(PropertyKey::from(unscopables_symbol)),
        },
    );
    assert!(environment.has_binding(&name("visible")).unwrap());
    assert!(!environment.has_binding(&name("hidden")).unwrap());
}

#[test]
fn with_unscopables_getters_can_reenter_environment_lookup() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let unscopables_symbol = Symbol::new(None);
    let object = Object::new_in(&allocation_context);
    object
        .create_data_property("visible".into(), Value::Number(1.0))
        .unwrap();
    object
        .create_data_property("checked".into(), Value::Number(2.0))
        .unwrap();
    let holder = Rc::new(RefCell::new(None::<Environment>));
    let observed = Rc::clone(&holder);
    let getter = Object::native_function_in(&allocation_context, move |_, _| {
        assert!(
            !observed
                .borrow()
                .as_ref()
                .unwrap()
                .has_binding(&name("absent"))?
        );
        Ok(Value::Undefined)
    });
    object
        .define_own_property(
            PropertyKey::from(unscopables_symbol.clone()),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        )
        .unwrap();
    let environment = Environment::object(
        object,
        None,
        ObjectEnvironmentOptions {
            with_environment: true,
            unscopables_key: Some(PropertyKey::from(unscopables_symbol)),
        },
    );
    *holder.borrow_mut() = Some(environment.clone());
    assert!(environment.with_base_object().is_some());
    assert!(environment.has_binding(&name("checked")).unwrap());
}

#[test]
fn global_environment_detects_lexical_and_var_conflicts() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let global_object = Object::new_in(&allocation_context);
    global_object
        .define_own_property(
            "restricted".into(),
            PropertyDescriptor::data(Value::Number(1.0), true, true, false),
        )
        .unwrap();
    let environment =
        Environment::global(global_object.clone(), Value::Object(global_object.clone()));
    assert!(
        environment
            .has_restricted_global_property(&name("restricted"))
            .unwrap()
    );
    assert!(
        !environment
            .can_declare_global_lexical(&name("restricted"))
            .unwrap()
    );
    assert!(
        environment
            .create_global_lexical_binding(name("restricted"), true, false)
            .is_err()
    );

    environment
        .create_global_var_binding(name("variable"), true)
        .unwrap();
    assert!(environment.has_var_declaration(&name("variable")).unwrap());
    // ECMA-262 2026 checks restricted properties, not a historical var-name list.
    // This explicitly deletable var has a configurable property.
    assert!(
        environment
            .can_declare_global_lexical(&name("variable"))
            .unwrap()
    );
    environment
        .create_global_lexical_binding(name("lexical"), true, false)
        .unwrap();
    assert!(
        !environment
            .can_declare_global_lexical(&name("lexical"))
            .unwrap()
    );
    assert!(
        environment
            .create_global_var_binding(name("lexical"), false)
            .is_err()
    );
    environment
        .create_global_function_binding(name("function"), Value::Number(7.0), true)
        .unwrap();
    assert_number(
        environment
            .get_binding_value(&name("function"), true)
            .unwrap(),
        7.0,
    );
    assert!(environment.delete_binding(&name("variable")).unwrap());
    assert!(!environment.has_var_declaration(&name("variable")).unwrap());
}

#[test]
fn non_extensible_globals_reject_new_var_and_function_declarations() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    let environment = Environment::global(object.clone(), Value::Object(object.clone()));
    object.prevent_extensions();
    assert!(!environment.can_declare_global_var(&name("newVar")).unwrap());
    assert!(
        !environment
            .can_declare_global_function(&name("newFunction"))
            .unwrap()
    );
    assert!(
        environment
            .create_global_var_binding(name("newVar"), false)
            .is_err()
    );
}

#[test]
fn existing_property_on_non_extensible_global_is_not_recorded_as_var() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let object = Object::new_in(&allocation_context);
    object
        .create_data_property("existing".into(), Value::Number(1.0))
        .unwrap();
    let environment = Environment::global(object.clone(), Value::Object(object.clone()));
    object.prevent_extensions();
    assert!(
        environment
            .can_declare_global_var(&name("existing"))
            .unwrap()
    );
    environment
        .create_global_var_binding(name("existing"), false)
        .unwrap();
    assert!(!environment.has_var_declaration(&name("existing")).unwrap());
}

#[test]
fn function_environment_tracks_this_function_and_new_target() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let function = Object::native_function_in(&allocation_context, |_, _| Ok(Value::Undefined));
    let new_target = Object::new_in(&allocation_context);
    let environment = Environment::function(
        None,
        function.clone(),
        Some(new_target.clone()),
        ThisBindingStatus::Uninitialized,
    );
    assert_eq!(environment.kind(), EnvironmentKind::Function);
    assert!(environment.has_this_binding().unwrap());
    assert_eq!(
        environment.get_this_binding().unwrap_err().kind(),
        RuntimeErrorKind::ReferenceError
    );
    let this_value = Value::Object(Object::new_in(&allocation_context));
    assert!(
        environment
            .bind_this_value(this_value.clone())
            .unwrap()
            .same_value(&this_value)
    );
    assert!(
        environment
            .get_this_binding()
            .unwrap()
            .same_value(&this_value)
    );
    assert_eq!(
        environment.this_binding_status().unwrap(),
        ThisBindingStatus::Initialized
    );
    assert!(environment.bind_this_value(Value::Null).is_err());
    assert_eq!(environment.function_object().unwrap(), function);
    assert_eq!(environment.new_target().unwrap(), Some(new_target));

    let lexical = Environment::function(
        None,
        Object::new_in(&allocation_context),
        None,
        ThisBindingStatus::Lexical,
    );
    assert!(!lexical.has_this_binding().unwrap());
    assert!(lexical.bind_this_value(Value::Undefined).is_err());
}

#[test]
fn module_import_bindings_are_live_and_immutable() {
    let context = temiya::runtime::AllocationContext::new();
    let exporting = Environment::module_in(&context);
    exporting
        .create_mutable_binding(name("exported"), false)
        .unwrap();
    exporting
        .initialize_binding(&name("exported"), Value::Number(1.0))
        .unwrap();
    let importing = Environment::module_in(&context);
    importing
        .create_import_binding(name("local"), exporting.clone(), name("exported"))
        .unwrap();
    assert_number(
        importing.get_binding_value(&name("local"), true).unwrap(),
        1.0,
    );
    exporting
        .set_mutable_binding(&name("exported"), Value::Number(2.0), true)
        .unwrap();
    assert_number(
        importing.get_binding_value(&name("local"), true).unwrap(),
        2.0,
    );
    assert_eq!(
        importing
            .set_mutable_binding(&name("local"), Value::Number(3.0), true)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(!importing.delete_binding(&name("local")).unwrap());
    assert!(
        importing
            .create_import_binding(name("missing"), exporting, name("absent"))
            .is_err()
    );
}

#[test]
fn record_specific_operations_reject_wrong_environment_kinds() {
    let context = temiya::runtime::AllocationContext::new();
    let declarative = Environment::declarative_in(&context);
    assert_eq!(
        declarative.global_this_value().unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert_eq!(
        declarative.get_this_binding().unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
    assert!(
        declarative
            .create_import_binding(name("x"), declarative.clone(), name("x"))
            .is_err()
    );
}

#[test]
fn ancestor_resolution_survives_gc_during_unscopables_then_continues_outward() {
    let context = temiya::runtime::AllocationContext::new();
    let outer = Environment::declarative_in(&context);
    outer
        .create_mutable_binding(name("visible"), false)
        .unwrap();
    outer
        .initialize_binding(&name("visible"), Value::Number(42.0))
        .unwrap();
    let symbol = Symbol::new(None);
    let object = Object::new_in(&context);
    object
        .create_data_property("visible".into(), Value::Number(1.0))
        .unwrap();
    let calls = Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    let getter = Object::native_function_with_context(&context, move |context, _, _| {
        observed.set(observed.get() + 1);
        context.collect_garbage()?;
        let blocked = Object::new_in(context);
        blocked.create_data_property("visible".into(), Value::Boolean(true))?;
        Ok(Value::Object(blocked))
    });
    object
        .define_own_property(
            PropertyKey::from(symbol.clone()),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        )
        .unwrap();
    let with = Environment::object(
        object,
        Some(outer),
        ObjectEnvironmentOptions {
            with_environment: true,
            unscopables_key: Some(PropertyKey::from(symbol)),
        },
    );
    // Only the descendant stays publicly rooted while lookup skips pure records
    // and crosses an observable object record that collects the managed heap.
    let descendant = Environment::declarative(Environment::declarative(with));
    assert_number(
        descendant
            .get_identifier_value(&name("visible"), true)
            .unwrap(),
        42.0,
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn identifier_value_reads_preserve_tdz_with_receivers_and_closure_scope() {
    use temiya::{Engine, embedding::ScriptBackend};
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let source = "let events='';let x=41;let value=0;let scope={get x(){events+='g';return 42;},method(){return this===scope;}};with(scope){value=x;if(!method())throw 1;}let blocked={};with(blocked){value+=x-41;}let tdz=false;try{let a=b;let b=1;}catch(e){tdz=e instanceof ReferenceError;}let missing=false;try{value+=noSuchBinding;}catch(e){missing=e instanceof ReferenceError;}function direct(){let local=42;return (()=>local)();}value===42 && events==='g' && tdz && missing && direct()===42;";
        let script = engine.compile(&realm, source).unwrap();
        let result = engine.evaluate(&script).unwrap();
        if !matches!(result.value(), Value::Boolean(true)) {
            let details = engine
                .compile(&realm, "value+':'+events+':'+tdz+':'+missing+':'+direct();")
                .unwrap();
            panic!(
                "{backend:?}: {:?}",
                engine.evaluate(&details).unwrap().value()
            );
        }
    }
}
