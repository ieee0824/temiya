use temiya::{
    parser::{ParserOptions, parse_function_body, parse_script},
    runtime::{
        Completion, Environment, JsString, Realm, RuntimeErrorKind, StatementEvaluator, Value,
    },
};

fn setup() -> (StatementEvaluator, Environment) {
    let realm = Realm::ordinary();
    let environment = Environment::declarative(Some(realm.global_environment()));
    (
        StatementEvaluator::with_environments(
            realm,
            environment.clone(),
            environment.clone(),
            Value::Undefined,
            false,
        ),
        environment,
    )
}

fn number(completion: Completion) -> f64 {
    let Completion::Normal(Some(Value::Number(value))) = completion else {
        panic!("expected a normal numeric completion")
    };
    value
}

#[test]
fn declarations_loops_and_control_flow_update_completion_values() {
    let (evaluator, _) = setup();
    let program = parse_script(
        "let sum = 0; for (let value of [1, 2, 3, 4]) { \
         if (value === 2) continue; sum += value; if (value === 3) break; } sum;",
    )
    .unwrap();

    assert_eq!(number(evaluator.evaluate_program(&program).unwrap()), 4.0);
}

#[test]
fn labelled_continue_and_break_target_the_matching_loop() {
    let (evaluator, _) = setup();
    let program = parse_script(
        "let count = 0; outer: for (let i = 0; i < 3; i++) { \
         for (let j = 0; j < 3; j++) { \
         if (j === 1) continue outer; count++; } } count;",
    )
    .unwrap();

    assert_eq!(number(evaluator.evaluate_program(&program).unwrap()), 3.0);
}

#[test]
fn switch_falls_through_and_consumes_unlabelled_break() {
    let (evaluator, _) = setup();
    let program = parse_script(
        "let result = ''; switch (2) { case 1: result += 'a'; \
         case 2: result += 'b'; case 3: result += 'c'; break; \
         default: result += 'd'; } result;",
    )
    .unwrap();

    let Completion::Normal(Some(Value::String(result))) =
        evaluator.evaluate_program(&program).unwrap()
    else {
        panic!("expected string completion")
    };
    assert_eq!(result, JsString::from("bc"));
}

#[test]
fn catch_handles_throw_and_normal_finally_preserves_prior_completion() {
    let (evaluator, _) = setup();
    let program =
        parse_script("try { throw 4; } catch (error) { error + 1; } finally { 99; }").unwrap();

    assert_eq!(number(evaluator.evaluate_program(&program).unwrap()), 5.0);
}

#[test]
fn abrupt_finally_replaces_return_and_throw_completions() {
    let (evaluator, _) = setup();
    let program = parse_function_body(
        "try { return 1; } finally { return 2; }",
        ParserOptions::default(),
    )
    .unwrap();
    let Completion::Return(value) = evaluator.evaluate_program(&program).unwrap() else {
        panic!("expected return completion")
    };
    assert!(value.same_value(&Value::Number(2.0)));

    let program = parse_function_body(
        "try { throw 1; } finally { return 3; }",
        ParserOptions::default(),
    )
    .unwrap();
    let Completion::Return(value) = evaluator.evaluate_program(&program).unwrap() else {
        panic!("expected finally return completion")
    };
    assert!(value.same_value(&Value::Number(3.0)));
}

#[test]
fn lexical_initializers_observe_tdz_and_const_rejects_assignment() {
    let (evaluator, _) = setup();
    let tdz = parse_script("let value = value;").unwrap();
    assert_eq!(
        evaluator.evaluate_program(&tdz).unwrap_err().kind(),
        RuntimeErrorKind::ReferenceError
    );

    let (evaluator, _) = setup();
    let constant = parse_script("const value = 1; value = 2;").unwrap();
    assert_eq!(
        evaluator.evaluate_program(&constant).unwrap_err().kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn uncaught_throw_is_a_completion_not_an_engine_error() {
    let (evaluator, _) = setup();
    let program = parse_script("throw 'failure';").unwrap();
    let Completion::Throw(Value::String(value)) = evaluator.evaluate_program(&program).unwrap()
    else {
        panic!("expected throw completion")
    };
    assert_eq!(value, JsString::from("failure"));
}
