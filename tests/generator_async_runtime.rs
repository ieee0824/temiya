use temiya::{
    parser::parse_script,
    runtime::{
        Completion, Environment, Promise, PromiseState, Realm, RuntimeErrorKind,
        StatementEvaluator, Value,
    },
};

fn run(source: &str) -> Result<Completion, temiya::runtime::RuntimeError> {
    let context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&context);
    let environment = Environment::declarative(realm.global_environment());
    StatementEvaluator::with_environments(
        realm,
        environment.clone(),
        environment,
        Value::Undefined,
        false,
    )
    .evaluate_program(&parse_script(source).unwrap())
}

fn boolean(source: &str) -> bool {
    let Completion::Normal(Some(Value::Boolean(value))) = run(source).unwrap() else {
        panic!("expected Boolean completion")
    };
    value
}

#[test]
fn generator_suspends_resumes_and_receives_next_values() {
    assert!(boolean(
        "function* values() { let resumed = yield 1; yield resumed; return 3; } \
         let iterator = values(); let first = iterator.next(); \
         let second = iterator.next(2); let third = iterator.next(); \
         first.value === 1 && first.done === false && \
         second.value === 2 && second.done === false && \
         third.value === 3 && third.done === true;"
    ));
}

#[test]
fn yield_star_delegates_in_order_and_then_continues() {
    assert!(boolean(
        "function* values() { yield* [1, 2]; return 3; } let iterator = values(); \
         let a = iterator.next(); let b = iterator.next(); let c = iterator.next(); \
         a.value === 1 && b.value === 2 && c.value === 3 && c.done;"
    ));
}

#[test]
fn generator_loops_resume_without_replaying_prior_iterations() {
    assert!(boolean(
        "function* values() { for (let index = 0; index < 3; index++) yield index; } \
         let iterator = values(); let a = iterator.next(); let b = iterator.next(); \
         let c = iterator.next(); let end = iterator.next(); \
         a.value === 0 && b.value === 1 && c.value === 2 && end.done;"
    ));
}

#[test]
fn return_resume_completes_and_subsequent_next_stays_done() {
    assert!(boolean(
        "function* values() { yield 1; yield 2; } let iterator = values(); \
         iterator.next(); let ended = iterator.return(9); let after = iterator.next(); \
         ended.value === 9 && ended.done && after.done;"
    ));
}

#[test]
fn throw_resume_abruptly_completes_the_generator() {
    let error = run("function* values() { yield 1; } let iterator = values(); \
         iterator.next(); iterator.throw('failure');")
    .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::ThrownValue);
    assert!(
        matches!(error.thrown_value(), Some(Value::String(value)) if value.to_string_lossy() == "failure")
    );
}

#[test]
fn async_functions_return_promises_and_await_fulfilled_values() {
    assert!(boolean(
        "async function base() { return 4; } \
         async function derived() { return (await base()) + 1; } \
         let promise = derived(); promise.state === 'fulfilled';"
    ));
}

#[test]
fn async_abrupt_completion_rejects_instead_of_escaping_call() {
    assert!(boolean(
        "async function fail() { throw 'failure'; } \
         let promise = fail(); promise.state === 'rejected';"
    ));
}

#[test]
fn public_promise_api_exposes_pending_and_settled_states() {
    let context = temiya::runtime::AllocationContext::new();
    let pending = Promise::pending(&context).unwrap();
    assert_eq!(pending.state(), PromiseState::Pending);
    assert_eq!(
        pending.await_result().unwrap_err().kind(),
        RuntimeErrorKind::InternalError
    );

    let fulfilled = Promise::fulfilled(&context, Value::Number(7.0)).unwrap();
    assert_eq!(fulfilled.state(), PromiseState::Fulfilled);
    assert!(
        fulfilled
            .await_result()
            .unwrap()
            .same_value(&Value::Number(7.0))
    );
    let reason = temiya::runtime::Object::new_in(&context);
    reason
        .create_data_property("self".into(), Value::Object(reason.clone()))
        .unwrap();
    let rejected = Promise::rejected(
        &context,
        temiya::runtime::RuntimeError::thrown(Value::Object(reason.clone())),
    )
    .unwrap();
    context.collect_garbage().unwrap();
    let error = rejected.await_result().unwrap_err();
    assert!(
        error
            .thrown_value()
            .unwrap()
            .same_value(&Value::Object(reason.clone()))
    );
    drop((pending, fulfilled, rejected, reason));
    assert_eq!(context.collect_garbage().unwrap().retained, 1);
    let Value::Object(reason) = error.thrown_value().unwrap() else {
        panic!("object rejection")
    };
    assert!(
        reason
            .get(&"self".into())
            .unwrap()
            .same_value(error.thrown_value().unwrap())
    );
    drop(error);
    assert_eq!(context.collect_garbage().unwrap().retained, 0);
}
