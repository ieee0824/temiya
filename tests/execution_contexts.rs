use temiya::parser::parse_script;
use temiya::runtime::{
    Agent, ContextState, Environment, ExecutionContext, Object, Realm, RuntimeError,
    RuntimeErrorKind, ScriptRecord, Value,
};

fn context(realm: &Realm) -> ExecutionContext {
    let environment = Environment::declarative(Some(realm.global_environment()));
    ExecutionContext::new(realm.clone(), None, environment.clone(), environment)
}

#[test]
fn realm_owns_global_object_and_environment() {
    let realm = Realm::ordinary();
    let global = realm.global_object();
    assert!(
        realm
            .global_environment()
            .global_this_value()
            .unwrap()
            .same_value(&Value::Object(global.clone()))
    );
    assert_eq!(realm.global_environment().binding_object(), Some(global));
    assert_ne!(realm, Realm::ordinary());
}

#[test]
fn script_records_and_contexts_preserve_ownership() {
    let realm = Realm::ordinary();
    let program = parse_script("let answer = 42;").unwrap();
    let script = ScriptRecord::new(realm.clone(), program.clone());
    assert_eq!(script.realm(), realm);
    assert_eq!(script.program(), &program);

    let context = ExecutionContext::for_script(script.clone());
    assert_eq!(context.realm(), &realm);
    assert_eq!(context.script(), Some(&script));
    assert!(context.function().is_none());
    assert_eq!(context.lexical_environment(), &realm.global_environment());
    assert_eq!(context.variable_environment(), &realm.global_environment());
}

#[test]
fn execution_context_environments_can_change_at_block_boundaries() {
    let realm = Realm::ordinary();
    let mut context = context(&realm);
    let lexical = Environment::declarative(Some(context.lexical_environment().clone()));
    let variable = Environment::declarative(Some(context.variable_environment().clone()));
    context.set_lexical_environment(lexical.clone());
    context.set_variable_environment(variable.clone());
    assert_eq!(context.lexical_environment(), &lexical);
    assert_eq!(context.variable_environment(), &variable);
}

#[test]
fn enter_and_leave_suspend_and_resume_callers() {
    let realm = Realm::ordinary();
    let outer = context(&realm);
    let outer_id = outer.identity();
    let inner = context(&realm);
    let inner_id = inner.identity();
    let mut agent = Agent::new();

    agent.enter(outer).unwrap();
    assert_eq!(agent.stack_depth(), 1);
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    agent.enter(inner).unwrap();
    assert_eq!(agent.stack_depth(), 2);
    assert_eq!(agent.current_context().unwrap().identity(), inner_id);
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    assert_eq!(agent.leave().unwrap().identity(), inner_id);
    assert_eq!(agent.current_context().unwrap().identity(), outer_id);
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    assert_eq!(agent.leave().unwrap().identity(), outer_id);
    assert_eq!(agent.stack_depth(), 0);
}

#[test]
fn suspend_and_resume_validate_state_transitions() {
    let realm = Realm::ordinary();
    let mut agent = Agent::new();
    assert_eq!(
        agent.suspend().unwrap_err().kind(),
        RuntimeErrorKind::InternalError
    );
    agent.enter(context(&realm)).unwrap();
    assert_eq!(
        agent.resume().unwrap_err().kind(),
        RuntimeErrorKind::InternalError
    );
    agent.suspend().unwrap();
    assert_eq!(agent.current_state(), Some(ContextState::Suspended));
    assert_eq!(
        agent.suspend().unwrap_err().kind(),
        RuntimeErrorKind::InternalError
    );
    agent.enter(context(&realm)).unwrap();
    assert_eq!(agent.stack_depth(), 2);
    agent.leave().unwrap();
    assert_eq!(agent.stack_depth(), 1);
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    agent.suspend().unwrap();
    agent.resume().unwrap();
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    agent.leave().unwrap();
    assert_eq!(
        agent.leave().unwrap_err().kind(),
        RuntimeErrorKind::InternalError
    );
}

#[test]
fn run_in_context_restores_stack_after_abrupt_completion() {
    let realm = Realm::ordinary();
    let root = context(&realm);
    let root_id = root.identity();
    let child = context(&realm);
    let grandchild = context(&realm);
    let mut agent = Agent::new();
    agent.enter(root).unwrap();

    let error = agent
        .run_in_context(child, |agent| {
            agent.enter(grandchild)?;
            Err::<(), _>(RuntimeError::type_error("abrupt"))
        })
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
    assert_eq!(error.message(), "abrupt");
    assert_eq!(agent.stack_depth(), 1);
    assert_eq!(agent.current_context().unwrap().identity(), root_id);
    assert_eq!(agent.current_state(), Some(ContextState::Running));
    agent.leave().unwrap();
}

#[test]
fn run_in_context_returns_values_and_cleans_top_level_context() {
    let realm = Realm::ordinary();
    let mut agent = Agent::new();
    let value = agent
        .run_in_context(context(&realm), |agent| {
            assert_eq!(agent.stack_depth(), 1);
            Ok(Value::Number(7.0))
        })
        .unwrap();
    assert!(value.same_value(&Value::Number(7.0)));
    assert_eq!(agent.stack_depth(), 0);
}

#[test]
fn scoped_execution_protects_and_restores_a_suspended_caller() {
    let realm = Realm::ordinary();
    let mut agent = Agent::new();
    agent.enter(context(&realm)).unwrap();
    agent.suspend().unwrap();
    let error = agent
        .run_in_context(context(&realm), |agent| {
            agent.leave()?;
            agent.leave()?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::InternalError);
    assert_eq!(agent.stack_depth(), 1);
    assert_eq!(agent.current_state(), Some(ContextState::Suspended));
    agent.resume().unwrap();
    agent.leave().unwrap();
}

#[test]
fn function_context_records_function_and_realm_identity() {
    let realm = Realm::ordinary();
    let function = Object::native_function(|_, _| Ok(Value::Undefined));
    let environment = Environment::function(
        Some(realm.global_environment()),
        function.clone(),
        None,
        temiya::runtime::ThisBindingStatus::Uninitialized,
    );
    let context = ExecutionContext::new(
        realm.clone(),
        Some(function.clone()),
        environment.clone(),
        environment,
    );
    assert_eq!(context.realm(), &realm);
    assert_eq!(context.function(), Some(&function));
}
