use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};

fn execute(
    realm: &Realm,
    source: &str,
    vm: bool,
) -> Result<Completion, temiya::runtime::RuntimeError> {
    if vm {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), realm)
    } else {
        StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script(source).unwrap())
    }
}

fn result(realm: &Realm, source: &str, vm: bool) -> Value {
    match execute(realm, source, vm).unwrap() {
        Completion::Normal(Some(value)) => value,
        other => panic!("{other:?}"),
    }
}

#[test]
fn updates_return_numeric_prefix_postfix_values_and_preserve_bigint() {
    for (source, expected) in [
        (
            "var n='41';var old=n++;old+':'+n;",
            Value::String("41:42".into()),
        ),
        ("var n='41';++n;", Value::Number(42.0)),
        ("var n='41';n++;", Value::Number(41.0)),
        ("var n=-0;n++;", Value::Number(-0.0)),
        ("var n=43;--n;", Value::Number(42.0)),
        ("var n=43n;--n;", Value::BigInt(42.into())),
        ("var n=42n;var old=n--;old;", Value::BigInt(42.into())),
        ("var n=0;var old=n++;old;", Value::Number(0.0)),
        ("var n=0/0;n++;", Value::Number(f64::NAN)),
        ("var n=1/0;n++;", Value::Number(f64::INFINITY)),
    ] {
        for vm in [false, true] {
            let value = result(&Realm::ordinary(), source, vm);
            assert!(value.same_value(&expected), "{source}: {value:?}");
        }
    }
}

#[test]
fn computed_update_converts_key_once_and_obeys_get_numeric_set_order() {
    let source = "var log='';var calls=0;var stored;var key=new Object();key[Symbol.toPrimitive]=function(hint){calls++;log=log+hint;return calls===1?'first':'other';};var number=new Object();number[Symbol.toPrimitive]=function(hint){log=log+hint;return 41;};var obj=new Object();var desc=new Object();desc.get=function(){if(!(this===obj)){throw 99;}log=log+'G';return number;};desc.set=function(v){if(!(this===obj)){throw 99;}log=log+'S';stored=v;};desc.configurable=true;Object.defineProperty(obj,'first',desc);var old=(obj[key])++;old+':'+stored+':'+log;";
    for vm in [false, true] {
        let realm = Realm::ordinary();
        assert!(
            result(&realm, source, vm).same_value(&Value::String("41:42:stringGnumberS".into()))
        );
        assert!(result(&realm, "calls;", vm).same_value(&Value::Number(1.0)));
        assert!(
            realm
                .global_object()
                .get(&"obj".into())
                .unwrap()
                .to_boolean()
        );
    }
}

#[test]
fn update_errors_skip_writes_and_preserve_tdz_const_and_symbols() {
    for (source, expected) in [
        ("missing++;", RuntimeErrorKind::ReferenceError),
        ("x++;let x=1;", RuntimeErrorKind::ReferenceError),
        ("const x=1;x++;", RuntimeErrorKind::TypeError),
        ("var x=Symbol.iterator;x++;", RuntimeErrorKind::TypeError),
        (
            "'use strict';var text='1';text[0]++;",
            RuntimeErrorKind::TypeError,
        ),
        (
            "var bad=new Object();bad[Symbol.toPrimitive]=function(){throw 17;};var x=bad;x++;",
            RuntimeErrorKind::ThrownValue,
        ),
    ] {
        for vm in [false, true] {
            let realm = Realm::ordinary();
            let error = execute(&realm, source, vm).unwrap_err();
            assert_eq!(error.kind(), expected, "{source}");
            if expected == RuntimeErrorKind::ThrownValue {
                assert!(
                    error
                        .thrown_value()
                        .unwrap()
                        .same_value(&Value::Number(17.0))
                );
            }
        }
    }
}

#[test]
fn typeof_unresolved_names_tdz_callables_and_other_values_match() {
    for (source, expected) in [
        ("typeof (missing);", "undefined"),
        ("typeof null;", "object"),
        ("typeof 42n;", "bigint"),
        ("typeof Symbol.iterator;", "symbol"),
        ("typeof (()=>42);", "function"),
        ("typeof 42;", "number"),
        ("typeof 'x';", "string"),
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary(), source, vm).same_value(&Value::String(expected.into()))
            );
        }
    }
    for vm in [false, true] {
        assert_eq!(
            execute(&Realm::ordinary(), "typeof x;let x=1;", vm)
                .unwrap_err()
                .kind(),
            RuntimeErrorKind::ReferenceError
        );
    }
}

#[test]
fn public_references_retain_coerced_object_keys_between_get_and_put() {
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::{JsString, Object, Reference, ReferencedName};
    let object = Object::new();
    object
        .create_data_property("first".into(), Value::Number(1.0))
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let key = Object::new();
    key.set_to_primitive_hook(move |_| {
        let count = captured.get() + 1;
        captured.set(count);
        Ok(Value::String(JsString::from(if count == 1 {
            "first"
        } else {
            "second"
        })))
    });
    let reference = Reference::property(
        Value::Object(object.clone()),
        ReferencedName::Value(Value::Object(key)),
        false,
        None,
    );
    assert!(
        reference
            .get_value()
            .unwrap()
            .same_value(&Value::Number(1.0))
    );
    reference.put_value(Value::Number(42.0)).unwrap();
    assert_eq!(calls.get(), 1);
    assert!(
        object
            .get(&"first".into())
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
    assert!(
        object
            .try_get_own_property(&"second".into())
            .unwrap()
            .is_none()
    );
}

#[test]
fn resolved_reference_releases_the_original_object_key_before_getter() {
    use std::rc::Rc;
    use temiya::runtime::{Object, PropertyDescriptor, Reference, ReferencedName};
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let key = Object::new();
    key.set_to_primitive_hook(move |_| {
        let _retain_until_conversion = &marker;
        Ok(Value::String("answer".into()))
    });
    let object = Object::new();
    let getter = Object::native_function(move |_, _| {
        assert!(
            weak.upgrade().is_none(),
            "computed key retained after conversion"
        );
        Ok(Value::Number(42.0))
    });
    object
        .define_own_property(
            "answer".into(),
            PropertyDescriptor::accessor(Some(getter), None, true, true),
        )
        .unwrap();
    let reference = Reference::property(
        Value::Object(object),
        ReferencedName::Value(Value::Object(key)),
        false,
        None,
    );
    assert!(
        reference
            .get_value()
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
}

#[test]
fn unary_numeric_operations_use_number_hint_and_bigint_rules() {
    for (source, expected) in [
        ("-0;", Value::Number(-0.0)),
        ("~4294967296;", Value::Number(-1.0)),
        ("~-1;", Value::Number(0.0)),
        ("-42n;", Value::BigInt((-42).into())),
        ("~41n;", Value::BigInt((-42).into())),
        ("var n=0;void n++;n;", Value::Number(1.0)),
        ("void 42;", Value::Undefined),
        (
            "var o=new Object();o[Symbol.toPrimitive]=function(hint){if(hint==='number'){return '42';}throw 17;};+o;",
            Value::Number(42.0),
        ),
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary(), source, vm).same_value(&expected),
                "{source}"
            );
        }
    }
    for source in [
        "+42n;",
        "+Symbol.iterator;",
        "var o=new Object();o[Symbol.toPrimitive]=function(){return 42n;};+o;",
    ] {
        for vm in [false, true] {
            assert_eq!(
                execute(&Realm::ordinary(), source, vm).unwrap_err().kind(),
                RuntimeErrorKind::TypeError,
                "{source}"
            );
        }
    }
}

#[test]
fn computed_key_conversion_can_reenter_the_same_public_reference() {
    use std::{
        cell::{Cell, OnceCell},
        rc::Rc,
    };
    use temiya::runtime::{Object, Reference, ReferencedName};
    let object = Object::new();
    object
        .create_data_property("answer".into(), Value::Number(42.0))
        .unwrap();
    let slot: Rc<OnceCell<Reference>> = Rc::new(OnceCell::new());
    let weak = Rc::downgrade(&slot);
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let key = Object::new();
    key.set_to_primitive_hook(move |_| {
        let previous = counter.get();
        counter.set(previous + 1);
        if previous == 0 {
            assert!(
                weak.upgrade()
                    .unwrap()
                    .get()
                    .unwrap()
                    .get_value()?
                    .same_value(&Value::Number(42.0))
            );
        }
        Ok(Value::String("answer".into()))
    });
    slot.set(Reference::property(
        Value::Object(object),
        ReferencedName::Value(Value::Object(key)),
        false,
        None,
    ))
    .unwrap();
    assert!(
        slot.get()
            .unwrap()
            .get_value()
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
    assert_eq!(calls.get(), 2);
    assert!(
        slot.get()
            .unwrap()
            .get_value()
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
    assert_eq!(calls.get(), 2);
}
