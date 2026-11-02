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
fn supplied_defaults_skip_effects_but_undefined_and_missing_arguments_run_them() {
    check(
        "let calls=0;function f(value=++calls){var value;return value;}f(20)===20&&calls===0&&f(undefined)===1&&f()===2&&f(null)===null&&f(false)===false&&f('')===''&&calls===2&&f.length===0;",
    );
    check(
        "function f(value=1,get=()=>value){var value=42;return get();}function g(value=1){var value;return ()=>value;}f(20,()=>22)===22&&f(20)===20&&f(undefined)===1&&g(42)()===42;",
    );
}

#[test]
fn supplied_defaults_keep_arguments_and_constructor_context() {
    check("function f(value=1){var value=42;return arguments[0];}f(20)===20&&f()===undefined;");
    check(
        "let calls=0;function Item(value=++calls){var value;this.value=value;this.target=new.target;}let a=new Item(20);let b=new Item(undefined);a.value===20&&a.target===Item&&b.value===1&&b.target===Item&&calls===1&&Item.length===0;",
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

#[test]
fn generator_and_async_bodies_keep_defaults_outside_body_vars() {
    check(
        "let x=42;function* f(a=()=>x){var x=1;yield a();}function* g(a=1,b=()=>a){var a=42;yield b();}let answer=0;async function h(a=()=>x){var x=1;answer=a();}h();f().next().value===42&&g().next().value===1&&answer===42;",
    );
}

#[test]
fn parameter_tdz_and_rest_without_expressions_preserve_cells() {
    check(
        "let tdz=0;function f(a=b,b=42){}for(let i=0;i<2;i++){try{f();}catch(e){if(e instanceof ReferenceError)tdz++;}}function rest(...items){var items;return items[0];}function plain({value}){var value;return value;}tdz===2&&rest(42)===42&&plain({value:42})===42;",
    );
}

#[test]
fn escaping_defaults_and_suspended_generator_keep_cells_across_gc() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = temiya::runtime::Object::native_function_with_context(
                engine.allocation_context(),
                |context, _, _| {
                    context.collect_garbage()?;
                    Ok(Value::Undefined)
                },
            );
            engine
                .set_global(
                    &realm,
                    "gcNow",
                    engine.value(&realm, Value::Object(gc)).unwrap(),
                )
                .unwrap();
            let source = "let saved;function f(a=()=>value,value={answer:20}){var value={answer:99};gcNow();saved=a;return value.answer;}let body=f();gcNow();let first=saved().answer;function* g(a=()=>value,value={answer:22}){var value={answer:99};gcNow();yield a();}let it=g();let yielded=it.next().value;gcNow();first+yielded.answer===42&&body===99;";
            let script = engine.compile(&realm, source).unwrap();
            assert!(
                matches!(
                    engine.evaluate(&script).unwrap().value(),
                    Value::Boolean(true)
                ),
                "{backend:?}"
            );
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn large_parameter_and_var_sets_preserve_initial_values_and_duplicates() {
    let parameters = (0..192)
        .map(|i| format!("值{i}={}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let vars = (0..192)
        .map(|i| format!("值{i},值{i}"))
        .collect::<Vec<_>>()
        .join(",");
    check(&format!(
        "function f({parameters}){{var {vars};return 值0+值191;}}f()===193;"
    ));
}

#[test]
fn prepared_body_keeps_function_context_and_parameter_closures() {
    // Auto lowers these bodies after parameter initialization, including the
    // assignment-pattern bridge. The separate body record must retain the
    // actual Function record for this/new.target and default closures.
    for source in [
        "function F(read=()=>new.target){var read;[this.answer=read()===new.target]=[];}new F().answer;",
        "function F(read=()=>this){var read;({missing:this.answer=read()===this}={});}new F().answer;",
        "function f(a=1,read=()=>a){function a(){return 42;}return typeof a==='function'&&a()===42&&read()===1;}f();",
    ] {
        check(source);
    }
}
