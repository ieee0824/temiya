use std::{cell::Cell, rc::Rc};
use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, RuntimeError, Value},
};

#[test]
fn numeric_results_preserve_array_object_and_pending_call_roots() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        for (source, expected) in [
            (
                "function sum(a,b,c){return a+b+c;} var a=[1,2+3,sum(4,5*6,7)]; \
              var o={value:40+2,marker:a};o.value+o.marker[0]+o.marker[1]+o.marker[2];",
                Value::Number(89.0),
            ),
            (
                "var a=[-0/3,0/0,1/0,2<3];1/a[0]===-1/0&&a[1]!==a[1]&&a[2]===1/0&&a[3]===true;",
                Value::Boolean(true),
            ),
        ] {
            let script = engine.compile(&realm, source).unwrap();
            assert!(
                engine
                    .evaluate(&script)
                    .unwrap_or_else(|error| panic!("{backend:?}: {source}: {error:?}"))
                    .value()
                    .same_value(&expected)
            );
        }
    }
}

#[test]
fn non_number_operands_keep_ordered_coercion_and_string_concatenation() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine
            .compile(
                &realm,
                "var log='';var left={valueOf:function(){log+='L';return 6;}}; \
             var right={valueOf:function(){log+='R';return 7;}}; \
             var a=[7,left*right,'v'+(1+2)];a[0]+':'+a[1]+':'+a[2]+':'+log;",
            )
            .unwrap();
        assert!(
            engine
                .evaluate(&script)
                .unwrap()
                .value()
                .same_value(&Value::String("7:42:v3:LR".into()))
        );
    }
}

#[test]
fn coercion_throw_keeps_identity_and_skips_the_right_callback() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let calls = Rc::new(Cell::new(0));
        let reason = Value::Object(Object::new_in(engine.allocation_context()));
        for (name, throws) in [("left", true), ("right", false)] {
            let object = Object::new_in(engine.allocation_context());
            let observed = calls.clone();
            let thrown = reason.clone();
            object
                .create_data_property(
                    "valueOf".into(),
                    Value::Object(Object::native_function_in(
                        engine.allocation_context(),
                        move |_, _| {
                            observed.set(observed.get() + 1);
                            if throws {
                                Err(RuntimeError::thrown(thrown.clone()))
                            } else {
                                Ok(Value::Number(7.0))
                            }
                        },
                    )),
                )
                .unwrap();
            engine
                .set_global(
                    &realm,
                    name,
                    engine.value(&realm, Value::Object(object)).unwrap(),
                )
                .unwrap();
        }
        let script = engine.compile(&realm, "[1,left+right];").unwrap();
        let error = engine.evaluate(&script).unwrap_err();
        assert!(error.thrown().unwrap().value().same_value(&reason));
        assert_eq!(calls.get(), 1);
    }
}

#[test]
fn numeric_binding_reads_preserve_bits_and_observe_captured_updates() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        for bits in [
            0,
            1,
            0x8000_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            0x7ff8_dead_beef_1234,
            0x4340_0000_0000_0000,
        ] {
            engine
                .set_global(
                    &realm,
                    "input",
                    engine
                        .value(&realm, Value::Number(f64::from_bits(bits)))
                        .unwrap(),
                )
                .unwrap();
            let script = engine
                .compile(
                    &realm,
                    "function pass(value){let local=0;local=value;return local;}pass(input);",
                )
                .unwrap();
            let value = engine.evaluate(&script).unwrap();
            let Value::Number(number) = value.value() else {
                panic!("Number result")
            };
            assert_eq!(number.to_bits(), bits);
        }
        let script = engine.compile(&realm,"function f(){let x=1;function read(){return x;}var before=read();x=2;return before*10+read();}f();").unwrap();
        assert!(
            engine
                .evaluate(&script)
                .unwrap()
                .value()
                .same_value(&Value::Number(12.0))
        );
        for (source, kind) in [
            (
                "{x;let x=1;}",
                temiya::embedding::ExceptionKind::ReferenceError,
            ),
            (
                "const fixed=42;fixed=43;",
                temiya::embedding::ExceptionKind::TypeError,
            ),
        ] {
            let script = engine.compile(&realm, source).unwrap();
            assert_eq!(engine.evaluate(&script).unwrap_err().kind(), kind);
        }
    }
}

#[test]
#[ignore = "requires the pinned Test262 corpus; see docs/typed-number-dispatch.md"]
fn pinned_test262_typed_number_dispatch() {
    use std::process::Command;
    use temiya::test262::{TEST262_REVISION, TestCase, TestVariant};

    let corpus = std::env::var("TEST262_DIR").expect("set TEST262_DIR");
    let read = |path: &str| {
        let output = Command::new("git")
            .args([
                "-c",
                &format!("safe.directory={corpus}"),
                "-C",
                &corpus,
                "show",
                &format!("{TEST262_REVISION}:{path}"),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "missing pinned source {path}");
        String::from_utf8(output.stdout).unwrap()
    };
    let harness = format!("{}\n{}", read("harness/sta.js"), read("harness/assert.js"));
    let mut passed = 0;
    for name in [
        "modulus/S11.5.3_A4_T7.js",
        "modulus/S11.5.3_A4_T5.js",
        "modulus/S11.5.3_A4_T6.js",
        "multiplication/S11.5.1_A4_T6.js",
        "modulus/bigint-arithmetic.js",
    ] {
        let path = format!("test/language/expressions/{name}");
        let case = TestCase::parse(&path, read(&path)).unwrap();
        assert!(case.metadata.includes.is_empty() && case.metadata.negative.is_none());
        for variant in case.variants() {
            assert_ne!(variant, TestVariant::Module);
            for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
                let engine = Engine::builder().script_backend(backend).build();
                let realm = engine.create_realm();
                let setup = engine.compile(&realm, &harness).unwrap();
                engine.evaluate(&setup).unwrap();
                let script = engine.compile(&realm, &case.source_for(variant)).unwrap();
                if backend == ScriptBackend::Auto {
                    assert_eq!(script.backend(), ScriptBackend::Bytecode, "{path}");
                }
                engine
                    .evaluate(&script)
                    .unwrap_or_else(|error| panic!("{path} {variant:?} {backend:?}: {error:?}"));
                passed += 1;
                println!("PASS {path} {variant:?} {backend:?}");
            }
        }
    }
    assert_eq!(passed, 20);
    println!("Test262 {TEST262_REVISION}: {passed} passed; 0 failed; 0 skipped");
}
