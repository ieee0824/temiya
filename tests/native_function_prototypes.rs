use std::rc::Rc;
use temiya::{
    Engine,
    runtime::{IntrinsicId, Object, Realm, RuntimeError, Value},
};
fn evaluate(source: &str) -> Value {
    let engine = Engine::default();
    let realm = engine.create_realm();
    engine
        .evaluate(&engine.compile(&realm, source).unwrap())
        .unwrap()
        .value()
        .clone()
}
#[test]
fn public_engine_calls_native_js_and_bound_functions_through_function_prototype() {
    for source in [
        "typeof Function.prototype.call.bind==='function' && typeof Math.abs.call==='function';",
        "Math.abs.call(null,-42)===42 && Math.abs.apply(null,[-42])===42;",
        "Math.abs.bind(null,-42).call()===42;",
        "Function.prototype.call.bind(Math.abs)(null,-42)===42;",
        "Object.getPrototypeOf(Math.abs)===Function.prototype && Object.getPrototypeOf(Function.prototype)===Object.prototype;",
        "function f(a,b){return this.x+a+b;} f.call({x:1},20,21)===42 && f.apply({x:1},[20,21])===42 && f.bind({x:1},20).call(null,21)===42;",
        "let count=0; let a=()=>{count++;return 42;}; a.call(null)===42 && count===1;",
        "let record=Proxy.revocable({},{}); Object.getPrototypeOf(record.revoke)===Function.prototype && record.revoke.length===0;",
        "function* g(){yield 42;} let iterator=g(); Object.getPrototypeOf(iterator.next)===Function.prototype && iterator.next.call(iterator).value===42;",
        "async function f(){return 42;} let p=f(); Object.getPrototypeOf(p.then)===Function.prototype && Object.getPrototypeOf(p.then().then)===Function.prototype;",
    ] {
        assert!(matches!(evaluate(source), Value::Boolean(true)), "{source}");
    }
}
#[test]
fn installed_intrinsic_callables_have_their_own_realms_function_prototype() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for realm in [
        Realm::ordinary_in(&allocation_context),
        Realm::ordinary_in(&allocation_context),
    ] {
        let function_prototype = realm.intrinsic(IntrinsicId::FunctionPrototype);
        for (_, holder) in realm.intrinsics().iter() {
            if holder.is_callable() || holder.is_constructor() {
                assert_eq!(
                    holder.prototype(),
                    Some(if holder == function_prototype {
                        realm.intrinsic(IntrinsicId::ObjectPrototype)
                    } else {
                        function_prototype.clone()
                    })
                );
            }
            for key in holder.own_property_keys() {
                let Some(d) = holder.get_own_property(&key) else {
                    continue;
                };
                for value in [
                    d.value,
                    d.get.flatten().map(Value::Object),
                    d.set.flatten().map(Value::Object),
                ]
                .into_iter()
                .flatten()
                {
                    if let Value::Object(function) = value {
                        if function.is_callable() || function.is_constructor() {
                            assert_eq!(
                                function.prototype(),
                                Some(if function == function_prototype {
                                    realm.intrinsic(IntrinsicId::ObjectPrototype)
                                } else {
                                    function_prototype.clone()
                                }),
                                "{key:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn builtin_metadata_and_method_constructability_are_preserved() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&allocation_context);
    for (id, name, length) in [
        (IntrinsicId::Math, "abs", 1.0),
        (IntrinsicId::FunctionPrototype, "call", 1.0),
        (IntrinsicId::FunctionPrototype, "apply", 2.0),
        (IntrinsicId::FunctionPrototype, "bind", 1.0),
        (IntrinsicId::Reflect, "get", 2.0),
        (IntrinsicId::Reflect, "set", 3.0),
        (IntrinsicId::Number, "isSafeInteger", 1.0),
    ] {
        let desc = realm.intrinsic(id).get_own_property(&name.into()).unwrap();
        assert_eq!(
            (desc.writable, desc.enumerable, desc.configurable),
            (Some(true), Some(false), Some(true))
        );
        let Value::Object(function) = desc.value.unwrap() else {
            panic!()
        };
        assert!(!function.is_constructor());
        assert!(function.get_own_property(&"prototype".into()).is_none());
        for (name, value) in [
            ("name", Value::String(name.into())),
            ("length", Value::Number(length)),
        ] {
            let d = function.get_own_property(&name.into()).unwrap();
            assert!(d.value.unwrap().same_value(&value));
            assert_eq!(
                (d.writable, d.enumerable, d.configurable),
                (Some(false), Some(false), Some(true))
            );
        }
    }
}
#[test]
fn registered_host_function_and_low_level_native_allocation_have_distinct_contracts() {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    engine
        .register_global_function(&realm, "answer", move |context, _, arguments| {
            let _ = &marker;
            Ok(context.value(Value::Number(40.0 + arguments.len() as f64)))
        })
        .unwrap();
    for source in [
        "answer.call(null,1,2)===42;",
        "answer.apply(null,[1,2])===42;",
        "answer.bind(null,1).call(null,2)===42;",
        "Object.getPrototypeOf(answer)===Function.prototype && answer.name==='answer' && answer.length===0;",
    ] {
        assert!(matches!(
            engine
                .evaluate(&engine.compile(&realm, source).unwrap())
                .unwrap()
                .value(),
            Value::Boolean(true)
        ));
    }
    assert!(
        Object::native_function_in(engine.allocation_context(), |_, _| Ok(Value::Undefined))
            .prototype()
            .is_none()
    );
    assert!(weak.upgrade().is_some());
    drop(realm);
    engine.collect_garbage().unwrap();
    assert!(weak.upgrade().is_none());
}
#[test]
fn realm_native_callbacks_and_bound_constructors_release_captured_host_state() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let realm = Realm::ordinary_in(&allocation_context);
    let expected = realm.intrinsic(IntrinsicId::FunctionPrototype);
    let native = realm
        .create_native_function("owned", 0, move |_, _| {
            let _ = &marker;
            Ok(Value::Number(42.0))
        })
        .unwrap();
    drop(realm);
    assert_eq!(native.prototype(), Some(expected));
    assert!(matches!(
        native.call(Value::Undefined, &[]).unwrap(),
        Value::Number(42.0)
    ));
    drop(native);
    allocation_context.collect_garbage().unwrap();
    assert!(weak.upgrade().is_none());
    let realm = Realm::ordinary_in(&allocation_context);
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let target = Object::native_callable_constructor(
        &allocation_context,
        |_, _| Ok(Value::Undefined),
        move |_, target| {
            let _ = &marker;
            Ok(Object::new_in(target.allocation_context()))
        },
    );
    let Value::Object(bind) = realm
        .intrinsic(IntrinsicId::FunctionPrototype)
        .get(&"bind".into())
        .unwrap()
    else {
        panic!()
    };
    let bound = bind.call(Value::Object(target), &[]).unwrap();
    drop(bound);
    allocation_context.collect_garbage().unwrap();
    assert!(weak.upgrade().is_none());
}
#[test]
fn call_apply_bind_preserve_getter_order_and_original_exception() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    assert!(matches!(
        evaluate(
            "let log=''; function f(a,b){log+='C';return this.x+a+b;} let args={get length(){log+='L';return 2;},get 0(){log+='A';return 20;},get 1(){log+='B';return 21;}}; let result=f.apply({x:1},args); result===42 && log==='LABC';"
        ),
        Value::Boolean(true)
    ));
    let realm = Realm::ordinary_in(&allocation_context);
    let thrown = Object::new_in(&allocation_context);
    let marker = thrown.clone();
    let function = realm
        .create_native_function("throws", 0, move |_, _| {
            Err(RuntimeError::thrown(Value::Object(marker.clone())))
        })
        .unwrap();
    let Value::Object(call) = realm
        .intrinsic(IntrinsicId::FunctionPrototype)
        .get(&"call".into())
        .unwrap()
    else {
        panic!()
    };
    let error = call.call(Value::Object(function), &[]).unwrap_err();
    assert!(
        error
            .thrown_value()
            .unwrap()
            .same_value(&Value::Object(thrown))
    );
}

#[test]
fn fresh_native_allocation_does_not_traverse_a_mutated_function_prototype_chain() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::cell::Cell;
    let realm = Realm::ordinary_in(&allocation_context);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let target = Object::new_in(&allocation_context);
    let handler = Object::new_in(&allocation_context);
    handler
        .create_data_property(
            "getPrototypeOf".into(),
            Value::Object(Object::native_function_in(
                &allocation_context,
                move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Value::Null)
                },
            )),
        )
        .unwrap();
    let proxy = temiya::runtime::ProxyObject::new(target, handler).object();
    assert!(
        realm
            .intrinsic(IntrinsicId::FunctionPrototype)
            .try_set_prototype(Some(proxy))
            .unwrap()
    );
    calls.set(0);
    let function = realm
        .create_native_function("fresh", 0, |_, _| Ok(Value::Undefined))
        .unwrap();
    assert_eq!(
        function.prototype(),
        Some(realm.intrinsic(IntrinsicId::FunctionPrototype))
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn own_property_key_queries_preserve_order_brands_and_proxy_errors() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use temiya::runtime::{JsString, PropertyDescriptor, PropertyKey, RuntimeErrorKind, Symbol};
    assert!(matches!(
        evaluate(
            "let keys=Object.getOwnPropertyNames('ab'); Array.isArray(keys) && Array.isArray(Object.getOwnPropertySymbols({})) && Object.getOwnPropertyNames({a:1})[0]==='a' && keys.length===3 && keys[0]==='0' && keys[1]==='1' && keys[2]==='length';"
        ),
        Value::Boolean(true)
    ));
    let realm = Realm::ordinary_in(&allocation_context);
    let constructor = realm.intrinsic(IntrinsicId::Object);
    let Value::Object(names) = constructor.get(&"getOwnPropertyNames".into()).unwrap() else {
        panic!()
    };
    let Value::Object(symbols) = constructor.get(&"getOwnPropertySymbols".into()).unwrap() else {
        panic!()
    };
    let source = Object::new_in(&allocation_context);
    let symbol = Symbol::new(Some("own".into()));
    for (key, value) in [
        ("10".into(), Value::Number(10.0)),
        ("1".into(), Value::Number(1.0)),
        ("a".into(), Value::Number(2.0)),
        (PropertyKey::Symbol(symbol.clone()), Value::Number(3.0)),
    ] {
        source.create_data_property(key, value).unwrap();
    }
    source
        .define_own_property(
            "hidden".into(),
            PropertyDescriptor::data(Value::Undefined, false, false, false),
        )
        .unwrap();
    let Value::Object(list) = names
        .call(Value::Undefined, &[Value::Object(source.clone())])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        list.prototype(),
        Some(realm.intrinsic(IntrinsicId::ArrayPrototype))
    );
    for (index, expected) in ["1", "10", "a", "hidden"].into_iter().enumerate() {
        assert!(
            list.get(&index.to_string().as_str().into())
                .unwrap()
                .same_value(&Value::String(expected.into()))
        );
    }
    assert!(list.set(&"length".into(), Value::Number(1.0)).unwrap());
    assert!(list.get_own_property(&"1".into()).is_none());
    let Value::Object(list) = symbols
        .call(Value::Undefined, &[Value::Object(source)])
        .unwrap()
    else {
        panic!()
    };
    assert!(
        list.get(&"0".into())
            .unwrap()
            .same_value(&Value::Symbol(symbol))
    );
    let Value::Object(list) = names
        .call(
            Value::Undefined,
            &[Value::String(JsString::from_code_units_slice(&[0xd800]))],
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(
        list.get(&"length".into())
            .unwrap()
            .same_value(&Value::Number(2.0))
    );
    for query in [&names, &symbols] {
        for input in [Value::Null, Value::Undefined] {
            assert_eq!(
                query.call(Value::Undefined, &[input]).unwrap_err().kind(),
                RuntimeErrorKind::TypeError
            );
        }
    }
    let marker = Object::new_in(&allocation_context);
    let capture = marker.clone();
    let handler = Object::new_in(&allocation_context);
    handler
        .create_data_property(
            "ownKeys".into(),
            Value::Object(Object::native_function_in(
                &allocation_context,
                move |_, _| Err(RuntimeError::thrown(Value::Object(capture.clone()))),
            )),
        )
        .unwrap();
    let proxy =
        temiya::runtime::ProxyObject::new(Object::new_in(&allocation_context), handler).object();
    for query in [&names, &symbols] {
        let error = query
            .call(Value::Undefined, &[Value::Object(proxy.clone())])
            .unwrap_err();
        assert!(
            error
                .thrown_value()
                .unwrap()
                .same_value(&Value::Object(marker.clone()))
        );
    }
}

#[test]
fn own_descriptor_queries_convert_keys_before_receiver_and_keep_proxy_exceptions() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for name in ["hasOwnProperty", "propertyIsEnumerable"] {
        let source = format!(
            "let seen=''; let key={{[Symbol.toPrimitive](hint){{seen+=hint;throw 17;}}}}; let caught=false; try{{Object.prototype.{name}.call(null,key);}}catch(e){{caught=e===17;}} caught && seen==='string';"
        );
        assert!(matches!(evaluate(&source), Value::Boolean(true)));
    }
    assert!(matches!(
        evaluate(
            "Object.prototype.propertyIsEnumerable.call('hi','0') && !Object.prototype.propertyIsEnumerable.call('hi','length') && Object.prototype.hasOwnProperty.call('hi','length');"
        ),
        Value::Boolean(true)
    ));
    let realm = Realm::ordinary_in(&allocation_context);
    let marker = Object::new_in(&allocation_context);
    let capture = marker.clone();
    let handler = Object::new_in(&allocation_context);
    handler
        .create_data_property(
            "getOwnPropertyDescriptor".into(),
            Value::Object(Object::native_function_in(
                &allocation_context,
                move |_, _| Err(RuntimeError::thrown(Value::Object(capture.clone()))),
            )),
        )
        .unwrap();
    let proxy =
        temiya::runtime::ProxyObject::new(Object::new_in(&allocation_context), handler).object();
    for name in ["hasOwnProperty", "propertyIsEnumerable"] {
        let Value::Object(method) = realm
            .intrinsic(IntrinsicId::ObjectPrototype)
            .get(&name.into())
            .unwrap()
        else {
            panic!()
        };
        let error = method
            .call(Value::Object(proxy.clone()), &[Value::String("key".into())])
            .unwrap_err();
        assert!(
            error
                .thrown_value()
                .unwrap()
                .same_value(&Value::Object(marker.clone()))
        );
    }
}
