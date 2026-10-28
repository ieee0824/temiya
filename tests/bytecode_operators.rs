use temiya::{
    bytecode::{BytecodeCompiler, Constant, Instruction, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};

#[test]
fn local_number_updates_preserve_signed_zero_nonfinite_and_fallback_types() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "(function(){var x=-0;var old=x++;return 1/old===-Infinity && x===1;})();",
        "(function(){let x=0;var a=--x;var b=x--;return a===-1 && b===-1 && x===-2;})();",
        "(function(){var x=Infinity;var old=x++;return old===Infinity && x===Infinity;})();",
        "(function(){var x=NaN;var old=x++;return old!==old && x!==x;})();",
        "(function(){var x='41';var old=x++;return old===41 && x===42;})();",
        "(function(){var x=41n;var old=x++;return old===41n && x===42n;})();",
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm)
                    .same_value(&Value::Boolean(true)),
                "{source}"
            );
        }
    }
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var read;function f(){const x=41;read=function(){return x;};x++;}f();";
        assert_eq!(
            execute(&realm, source, vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert!(result(&realm, "read();", vm).same_value(&Value::Number(41.0)));
    }
}

#[test]
fn lexical_updates_capture_verified_binding_before_numeric_coercion() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let source = "var calls=0;function f(){var x=new Object();x.valueOf=function(){calls++;x=99;return 41;};for(var i=0;i<1;i++){var old=x++;}return old+':'+x+':'+calls;}f();";
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(code.bytecode().constants.iter().any(|constant| {
        match constant {
            Constant::Function(template) => {
                template.body.bytecode().instructions.iter().any(|op| {
                    matches!(
                        op,
                        Instruction::ResolveBinding { .. } | Instruction::UpdateBinding { .. }
                    )
                })
            }
            Constant::Scope(scope) => scope.functions.iter().any(|declaration| {
                declaration
                    .template
                    .body
                    .bytecode()
                    .instructions
                    .iter()
                    .any(|op| {
                        matches!(
                            op,
                            Instruction::ResolveBinding { .. } | Instruction::UpdateBinding { .. }
                        )
                    })
            }),
            _ => false,
        }
    }));
    for vm in [false, true] {
        assert!(
            result(&Realm::ordinary_in(&allocation_context), source, vm)
                .same_value(&Value::String("41:42:1".into()))
        );
    }
}

#[test]
fn local_compound_assignments_snapshot_values_and_preserve_abrupt_ordering() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "(function(){var x=20;var y=(x+=(x=100,22));return x===42&&y===42;})();",
        "(function(){let x=20;function rhs(){x=100;return 22;}return (x+=rhs())===42&&x===42;})();",
        "(function(){var x={valueOf:function(){x=99;return 40;}};var y={valueOf:function(){x=100;return 2;}};return (x+=y)===42&&x===42;})();",
        "(function(){let x=43;x-=1;x*=2;x/=2;x%=100;x**=1;x|=0;x&=63;x^=0;x<<=1;x>>=1;x>>>=0;return x===42;})();",
        "(function(){var x=41n;return (x+=1n)===42n&&x===42n;})();",
        "(function(){var x='old';return (x+=(x='changed','!'))==='old!'&&x==='old!';})();",
    ] {
        for vm in [false, true] {
            assert!(
                result(&Realm::ordinary_in(&allocation_context), source, vm)
                    .same_value(&Value::Boolean(true)),
                "{source}"
            );
        }
    }
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var calls=0;function f(){const x=41;x+=(calls++,1);}f();";
        assert_eq!(
            execute(&realm, source, vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert!(result(&realm, "calls;", vm).same_value(&Value::Number(1.0)));
        let realm = Realm::ordinary_in(&allocation_context);
        let source = "var calls=0;function f(){x+=(calls++,1);let x;}f();";
        assert_eq!(
            execute(&realm, source, vm).unwrap_err().kind(),
            RuntimeErrorKind::ReferenceError
        );
        assert!(result(&realm, "calls;", vm).same_value(&Value::Number(0.0)));
    }
}

#[test]
fn loop_compound_binding_retains_identity_across_coercion_and_capture() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let source = "var calls=0;function f(){var x={valueOf:function(){calls++;x=99;return 41;}};function read(){return x;}for(var i=0;i<1;i++){x+=1;}return read()+':'+calls;}f();";
    for vm in [false, true] {
        assert!(
            result(&Realm::ordinary_in(&allocation_context), source, vm)
                .same_value(&Value::String("42:1".into()))
        );
    }
}

#[test]
fn global_update_observes_proxy_has_trap() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::{Object, ProxyObject};
    for vm in [false, true] {
        let count = Rc::new(Cell::new(0));
        let target = Object::new_in(&allocation_context);
        let handler = Object::new_in(&allocation_context);
        let proxy = ProxyObject::new(target.clone(), handler.clone()).object();
        let realm = Realm::new(proxy.clone(), Value::Object(proxy));
        let observed = count.clone();
        handler
            .create_data_property(
                "has".into(),
                Value::Object(Object::native_function_in(
                    &allocation_context,
                    move |_, args| {
                        let Value::Object(target) = &args[0] else {
                            panic!("target expected")
                        };
                        let key = args[1].to_property_key()?;
                        if key == "x".into() {
                            observed.set(observed.get() + 1);
                        }
                        Ok(Value::Boolean(target.has_property(&key)?))
                    },
                )),
            )
            .unwrap();
        let reset = count.clone();
        target
            .create_data_property(
                "reset".into(),
                Value::Object(Object::native_function_in(
                    &allocation_context,
                    move |_, _| {
                        reset.set(0);
                        Ok(Value::Undefined)
                    },
                )),
            )
            .unwrap();
        let source = "var x=41;reset();x++;";
        assert!(result(&realm, source, vm).same_value(&Value::Number(41.0)));
        assert!(
            count.get() > 0,
            "global name lookup must invoke the Proxy has trap"
        );
    }
}

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
    let allocation_context = temiya::runtime::AllocationContext::new();
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
            let value = result(&Realm::ordinary_in(&allocation_context), source, vm);
            assert!(value.same_value(&expected), "{source}: {value:?}");
        }
    }
}

#[test]
fn computed_update_converts_key_once_and_obeys_get_numeric_set_order() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let source = "var log='';var calls=0;var stored;var key=new Object();key[Symbol.toPrimitive]=function(hint){calls++;log=log+hint;return calls===1?'first':'other';};var number=new Object();number[Symbol.toPrimitive]=function(hint){log=log+hint;return 41;};var obj=new Object();var desc=new Object();desc.get=function(){if(!(this===obj)){throw 99;}log=log+'G';return number;};desc.set=function(v){if(!(this===obj)){throw 99;}log=log+'S';stored=v;};desc.configurable=true;Object.defineProperty(obj,'first',desc);var old=(obj[key])++;old+':'+stored+':'+log;";
    for vm in [false, true] {
        let realm = Realm::ordinary_in(&allocation_context);
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
    let allocation_context = temiya::runtime::AllocationContext::new();
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
            let realm = Realm::ordinary_in(&allocation_context);
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
    let allocation_context = temiya::runtime::AllocationContext::new();
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
                result(&Realm::ordinary_in(&allocation_context), source, vm)
                    .same_value(&Value::String(expected.into()))
            );
        }
    }
    for vm in [false, true] {
        assert_eq!(
            execute(
                &Realm::ordinary_in(&allocation_context),
                "typeof x;let x=1;",
                vm
            )
            .unwrap_err()
            .kind(),
            RuntimeErrorKind::ReferenceError
        );
    }
}

#[test]
fn public_references_retain_coerced_object_keys_between_get_and_put() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::{JsString, Object, Reference, ReferencedName};
    let object = Object::new_in(&allocation_context);
    object
        .create_data_property("first".into(), Value::Number(1.0))
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let key = Object::new_in(&allocation_context);
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
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::rc::Rc;
    use temiya::runtime::{Object, PropertyDescriptor, Reference, ReferencedName};
    let marker = Rc::new(());
    let weak = Rc::downgrade(&marker);
    let key = Object::new_in(&allocation_context);
    key.set_to_primitive_hook(move |_| {
        let _retain_until_conversion = &marker;
        Ok(Value::String("answer".into()))
    });
    let object = Object::new_in(&allocation_context);
    let getter = Object::native_function_with_context(&allocation_context, move |context, _, _| {
        context.collect_garbage()?;
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
    let allocation_context = temiya::runtime::AllocationContext::new();
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
                result(&Realm::ordinary_in(&allocation_context), source, vm).same_value(&expected),
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
                execute(&Realm::ordinary_in(&allocation_context), source, vm)
                    .unwrap_err()
                    .kind(),
                RuntimeErrorKind::TypeError,
                "{source}"
            );
        }
    }
}

#[test]
fn computed_key_conversion_can_reenter_the_same_public_reference() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{
        cell::{Cell, OnceCell},
        rc::Rc,
    };
    use temiya::runtime::{Object, Reference, ReferencedName};
    let object = Object::new_in(&allocation_context);
    object
        .create_data_property("answer".into(), Value::Number(42.0))
        .unwrap();
    let slot: Rc<OnceCell<Reference>> = Rc::new(OnceCell::new());
    let weak = Rc::downgrade(&slot);
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let key = Object::new_in(&allocation_context);
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

#[test]
fn global_name_resolution_has_traps_precede_reads_and_initializer_values() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::RefCell, rc::Rc};
    use temiya::runtime::{Object, ProxyObject};
    let mut failures = Vec::new();
    for (source, expected) in [
        ("var x=41;reset();x;", "HH"),
        ("var x=41;reset();x=rhs();", "HR"),
        ("var x=41;reset();var x=rhs();", "HR"),
    ] {
        for vm in [false, true] {
            let log = Rc::new(RefCell::new(String::new()));
            let target = Object::new_in(&allocation_context);
            let handler = Object::new_in(&allocation_context);
            let proxy = ProxyObject::new(target.clone(), handler.clone()).object();
            let realm = Realm::new(proxy.clone(), Value::Object(proxy));
            let observed = log.clone();
            handler
                .create_data_property(
                    "has".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, args| {
                            let Value::Object(target) = &args[0] else {
                                panic!()
                            };
                            let key = args[1].to_property_key()?;
                            if key == "x".into() {
                                observed.borrow_mut().push('H');
                            }
                            Ok(Value::Boolean(target.has_property(&key)?))
                        },
                    )),
                )
                .unwrap();
            let reset = log.clone();
            target
                .create_data_property(
                    "reset".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, _| {
                            reset.borrow_mut().clear();
                            Ok(Value::Undefined)
                        },
                    )),
                )
                .unwrap();
            let rhs = log.clone();
            target
                .create_data_property(
                    "rhs".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, _| {
                            rhs.borrow_mut().push('R');
                            Ok(Value::Number(42.0))
                        },
                    )),
                )
                .unwrap();
            assert!(matches!(
                execute(&realm, source, vm).unwrap(),
                Completion::Normal(_)
            ));
            let actual = log.borrow().clone();
            println!("source={source:?} vm={vm} expected={expected} actual={actual}");
            if actual != expected {
                failures.push(format!("{source}: vm={vm} {actual} != {expected}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn var_initializer_uses_the_with_environment_reference_in_ast() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    assert!(
        result(
            &Realm::ordinary_in(&allocation_context),
            "var x=1;var object={x:2};with(object){var x=3;}object.x===3&&x===1;",
            false
        )
        .same_value(&Value::Boolean(true))
    );
}

#[test]
fn resolved_global_reference_retains_source_strictness_after_deletion() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::{cell::Cell, rc::Rc};
    use temiya::runtime::{Object, ProxyObject};
    for strict in [false, true] {
        for vm in [false, true] {
            let target = Object::new_in(&allocation_context);
            target
                .create_data_property("x".into(), Value::Number(42.0))
                .unwrap();
            let handler = Object::new_in(&allocation_context);
            let proxy = ProxyObject::new(target.clone(), handler.clone()).object();
            let realm = Realm::new(proxy.clone(), Value::Object(proxy));
            let reads = Rc::new(Cell::new(99));
            let seen = reads.clone();
            handler
                .create_data_property(
                    "has".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, args| {
                            let Value::Object(target) = &args[0] else {
                                panic!()
                            };
                            let key = args[1].to_property_key()?;
                            if key == "x".into() {
                                let count = seen.get();
                                seen.set(count + 1);
                                if count == 0 {
                                    assert!(target.delete(&key));
                                    return Ok(Value::Boolean(true));
                                }
                            }
                            Ok(Value::Boolean(target.has_property(&key)?))
                        },
                    )),
                )
                .unwrap();
            let counter = reads.clone();
            target
                .create_data_property(
                    "reset".into(),
                    Value::Object(Object::native_function_in(
                        &allocation_context,
                        move |_, _| {
                            counter.set(0);
                            Ok(Value::Undefined)
                        },
                    )),
                )
                .unwrap();
            let source = if strict {
                "'use strict';var x;reset();x;"
            } else {
                "var x;reset();x;"
            };
            let value = execute(&realm, source, vm);
            if strict {
                assert_eq!(value.unwrap_err().kind(), RuntimeErrorKind::ReferenceError);
            } else {
                assert!(matches!(
                    value.unwrap(),
                    Completion::Normal(Some(Value::Undefined))
                ));
            }
            assert_eq!(reads.get(), 2);
        }
    }
}

#[test]
#[ignore = "requires pinned Test262 corpus in TEST262_DIR"]
fn pinned_test262_var_binding_resolution() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    use std::process::Command;
    use temiya::test262::TEST262_REVISION;
    let root = std::env::var("TEST262_DIR").expect("TEST262_DIR required");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", &format!("safe.directory={root}"), "-C", &root])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(git(&["rev-parse", "HEAD"]).trim(), TEST262_REVISION);
    let read = |path: &str| git(&["show", &format!("{TEST262_REVISION}:{path}")]);
    let path = "test/language/statements/variable/binding-resolution.js";
    let realm = Realm::ordinary_in(&allocation_context);
    for name in ["sta.js", "assert.js"] {
        assert!(matches!(
            execute(&realm, &read(&format!("harness/{name}")), false).unwrap(),
            Completion::Normal(_)
        ));
    }
    let source = read(path);
    assert!(matches!(
        execute(&realm, &source, false).unwrap(),
        Completion::Normal(_)
    ));
    let vm_error = BytecodeCompiler::compile(&source).unwrap_err();
    assert!(
        vm_error
            .message()
            .contains("unsupported bytecode statement")
    );
    println!(
        "{path} blob={} AST pass; VM unsupported with: {vm_error}",
        git(&["rev-parse", &format!("{TEST262_REVISION}:{path}")]).trim()
    );
}
