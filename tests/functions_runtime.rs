use temiya::{
    parser::parse_script,
    runtime::{Completion, Environment, Realm, RuntimeErrorKind, StatementEvaluator, Value},
};

fn run(source: &str) -> Result<Completion, temiya::runtime::RuntimeError> {
    let realm = Realm::ordinary();
    let environment = Environment::declarative(Some(realm.global_environment()));
    let evaluator = StatementEvaluator::with_environments(
        realm,
        environment.clone(),
        environment,
        Value::Undefined,
        false,
    );
    evaluator.evaluate_program(&parse_script(source).unwrap())
}

fn number(source: &str) -> f64 {
    let Completion::Normal(Some(Value::Number(value))) = run(source).unwrap() else {
        panic!("expected normal numeric completion")
    };
    value
}

#[test]
fn closures_capture_lexical_environments_and_declarations_are_hoisted() {
    assert_eq!(
        number("let captured = 6; result(); function result() { return captured + 1; }"),
        7.0
    );
}

#[test]
fn default_rest_and_arguments_bind_in_call_order() {
    assert_eq!(
        number(
            "function total(first, second = first + 1, ...rest) { \
             return second + rest.length + arguments.length; } \
             total(3, void 0, 8, 9);"
        ),
        10.0
    );
}

#[test]
fn methods_receive_dynamic_this_and_arrows_capture_it() {
    assert_eq!(
        number(
            "let object = { value: 11, make() { return () => this.value; } }; \
             let callback = object.make(); callback();"
        ),
        11.0
    );
}

#[test]
fn getters_and_setters_are_executable_methods() {
    assert_eq!(
        number(
            "let object = { stored: 1, get value() { return this.stored; }, \
             set value(next) { this.stored = next; } }; object.value = 9; object.value;"
        ),
        9.0
    );
}

#[test]
fn ordinary_functions_construct_instances_with_their_prototype() {
    assert_eq!(
        number(
            "function Point(value) { this.value = value; } \
             Point.prototype.read = function() { return this.value; }; \
             let point = new Point(42); point.read();"
        ),
        42.0
    );
}

#[test]
fn constructor_object_return_replaces_the_allocated_instance() {
    assert_eq!(
        number("function Factory() { return { value: 5 }; } new Factory().value;"),
        5.0
    );
}

#[test]
fn arrows_are_not_constructors() {
    let error = run("let arrow = () => 1; new arrow();").unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::TypeError);
}

#[test]
fn named_function_expressions_can_recur_through_their_inner_name() {
    assert_eq!(
        number(
            "let factorial = function inner(value) { \
             return value <= 1 ? 1 : value * inner(value - 1); }; factorial(5);"
        ),
        120.0
    );
}
