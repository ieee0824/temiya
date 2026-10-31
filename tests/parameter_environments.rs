use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn defaults_do_not_capture_body_vars_and_redeclared_parameters_are_copied() {
    check(
        "let x=42;function f(a=()=>x){var x=1;return a();}function g(a=1,b=()=>a){var a=42;return b();}f()===42&&g()===1;",
    );
}
#[test]
fn arguments_in_default_closures_survive_body_redeclaration() {
    check("function f(a,b=()=>arguments[0]){var arguments={0:42};return b();}f(20)===20;");
}
#[test]
fn nested_patterns_and_derived_constructor_keep_parameter_cells() {
    check(
        "let x=42;function f({a=()=>x}={}){var x=1;return a();}class Base{}class Child extends Base{constructor(a=1,b=()=>a){super();var a=42;this.answer=b();}}f()===42&&new Child().answer===1;",
    );
}
