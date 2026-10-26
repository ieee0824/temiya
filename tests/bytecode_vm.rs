use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, StatementEvaluator, Value},
};

fn evaluate_both(source: &str) -> (Completion, Completion) {
    let program = parse_script(source).unwrap();
    let interpreted = StatementEvaluator::new(Realm::ordinary())
        .evaluate_program(&program)
        .unwrap();
    let bytecode = BytecodeCompiler::compile_program(&program).unwrap();
    let executed = VirtualMachine::execute(&bytecode).unwrap();
    (interpreted, executed)
}

fn assert_same_completion(left: &Completion, right: &Completion) {
    match (left, right) {
        (Completion::Normal(Some(left)), Completion::Normal(Some(right)))
        | (Completion::Return(left), Completion::Return(right))
        | (Completion::Throw(left), Completion::Throw(right)) => {
            assert!(left.same_value(right), "{left:?} != {right:?}");
        }
        (Completion::Normal(None), Completion::Normal(None)) => {}
        _ => panic!("completion mismatch: {left:?} != {right:?}"),
    }
}

#[test]
fn arithmetic_conditionals_and_strings_match_ast_interpreter() {
    for source in [
        "40 + 2;",
        "true ? 6 * 7 : 0;",
        "false ? 0 : 84 / 2;",
        "'left' + 'right';",
        "!(1 === 2);",
    ] {
        let (interpreted, executed) = evaluate_both(source);
        assert_same_completion(&interpreted, &executed);
    }
}

#[test]
fn local_side_effect_order_and_control_flow_match() {
    for source in [
        "let x = 1; (x = x + 1) + (x = x * 10); x;",
        "let x = 0; if (true) { x = 42; } else { x = 1; } x;",
        "let x = 0; let run = true; while (run) { x = x + 1; run = false; } x;",
    ] {
        let (interpreted, executed) = evaluate_both(source);
        assert_same_completion(&interpreted, &executed);
    }
}

#[test]
fn throw_completion_preserves_its_value() {
    let (interpreted, executed) = evaluate_both("throw 'boom';");
    assert_same_completion(&interpreted, &executed);
}

#[test]
fn source_compile_reports_parse_and_unsupported_errors() {
    assert!(BytecodeCompiler::compile("let = ;").is_err());
    let unsupported = BytecodeCompiler::compile("({ answer: 42 });").unwrap_err();
    assert!(
        unsupported
            .message()
            .contains("unsupported bytecode expression")
    );
    assert!(unsupported.span().is_some());
    let unbound = BytecodeCompiler::compile("missing;").unwrap();
    assert_eq!(
        VirtualMachine::execute(&unbound).unwrap_err().kind(),
        temiya::runtime::RuntimeErrorKind::ReferenceError
    );
}

#[test]
fn division_edges_retain_javascript_number_behavior() {
    let bytecode = BytecodeCompiler::compile("1 / 0;").unwrap();
    let Completion::Normal(Some(Value::Number(value))) =
        VirtualMachine::execute(&bytecode).unwrap()
    else {
        panic!("expected number completion")
    };
    assert_eq!(value, f64::INFINITY);
}
