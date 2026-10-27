use temiya::{
    Engine,
    bytecode::{
        BinaryOperator, Bytecode, BytecodeCompiler, Constant, Instruction, VerifyErrorKind,
    },
    embedding::ScriptBackend,
    runtime::{Object, RuntimeError, Value},
};

#[test]
fn verifier_checks_constant_kind_bounds_and_one_operand() {
    use Instruction::{BinaryNumberConstant, LoadConstant, Return};
    assert_eq!(std::mem::size_of::<Instruction>(), 16);
    for (constants, instructions, expected) in [
        (
            vec![Constant::Number(2.0)],
            vec![
                LoadConstant(0),
                BinaryNumberConstant {
                    operator: BinaryOperator::Add,
                    constant: 1,
                },
                Return,
            ],
            VerifyErrorKind::ConstantOutOfBounds,
        ),
        (
            vec![Constant::String("2".into())],
            vec![
                LoadConstant(0),
                BinaryNumberConstant {
                    operator: BinaryOperator::Add,
                    constant: 0,
                },
                Return,
            ],
            VerifyErrorKind::InvalidConstantKind,
        ),
        (
            vec![Constant::Number(2.0)],
            vec![
                BinaryNumberConstant {
                    operator: BinaryOperator::Add,
                    constant: 0,
                },
                Return,
            ],
            VerifyErrorKind::StackUnderflow,
        ),
    ] {
        assert_eq!(
            Bytecode {
                constants,
                instructions,
                local_count: 0
            }
            .verify()
            .unwrap_err()
            .kind(),
            expected
        );
    }
    let code = BytecodeCompiler::compile("40+(2);").unwrap();
    assert!(
        code.bytecode()
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, BinaryNumberConstant { .. }))
    );
    assert_eq!(code.max_stack(), 1);
    // The final literal in an arbitrary RHS expression must not be removed:
    // branches can jump across it and have their own stack states.
    let code = BytecodeCompiler::compile("1+(true?2:3);").unwrap();
    assert!(
        !code
            .bytecode()
            .instructions
            .iter()
            .any(|instruction| matches!(instruction, BinaryNumberConstant { .. }))
    );
}

#[test]
fn literal_rhs_keeps_ieee_coercion_bigint_and_control_flow_semantics() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        for (source, expected) in [
            ("1/(-0*2);", Value::Number(f64::NEG_INFINITY)),
            ("var n=0/0;n*2!==n*2;", Value::Boolean(true)),
            ("var inf=1/0;inf%2!==inf%2;", Value::Boolean(true)),
            ("var inf=1/0;inf/2;", Value::Number(f64::INFINITY)),
            ("var s='v';s+2;", Value::String("v2".into())),
            (
                "var o={valueOf:function(){return 6;}};o*7;",
                Value::Number(42.0),
            ),
            ("1n===1;", Value::Boolean(false)),
            ("2**3;", Value::Number(8.0)),
            (
                "var count=0;function next(){count++;return 3;}var a=[1+(true?2:3),1+(false?2:3),1+next(),count];a[0]*1000+a[1]*100+a[2]*10+a[3];",
                Value::Number(3441.0),
            ),
            (
                "function pick(v){return [10,v*3,99];}var a=pick(7);a[0]+a[1]+a[2];",
                Value::Number(130.0),
            ),
        ] {
            let script = engine.compile(&realm, source).unwrap();
            assert!(
                engine
                    .evaluate(&script)
                    .unwrap_or_else(|error| panic!("{backend:?}: {source}: {error:?}"))
                    .value()
                    .same_value(&expected),
                "{backend:?}: {source}"
            );
        }
        for source in ["1n+1;", "1 in 2;", "1 instanceof 2;"] {
            let script = engine.compile(&realm, source).unwrap();
            assert_eq!(
                engine.evaluate(&script).unwrap_err().kind(),
                temiya::embedding::ExceptionKind::TypeError
            );
        }
    }
}

#[test]
fn literal_rhs_preserves_native_coercion_throw_identity() {
    use std::{cell::Cell, rc::Rc};
    for backend in [ScriptBackend::Ast, ScriptBackend::Bytecode] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let calls = Rc::new(Cell::new(0));
        let reason = Value::Object(Object::new());
        let object = Object::new();
        let observed = calls.clone();
        let thrown = reason.clone();
        object
            .create_data_property(
                "valueOf".into(),
                Value::Object(Object::native_function(move |_, _| {
                    observed.set(observed.get() + 1);
                    Err(RuntimeError::thrown(thrown.clone()))
                })),
            )
            .unwrap();
        engine
            .set_global(
                &realm,
                "input",
                engine.value(&realm, Value::Object(object)).unwrap(),
            )
            .unwrap();
        let script = engine.compile(&realm, "[1,input*3,4];").unwrap();
        let error = engine.evaluate(&script).unwrap_err();
        assert!(error.thrown().unwrap().value().same_value(&reason));
        assert_eq!(calls.get(), 1);
    }
}
