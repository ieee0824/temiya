use temiya::{Engine, embedding::ScriptBackend, runtime::Value};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine
                    .evaluate(&script)
                    .unwrap_or_else(|error| panic!("{backend:?}: {source}: {error:?}"))
                    .value(),
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
fn supplied_rest_keeps_arrays_defaults_and_body_cells() {
    check(
        "function Item(value=1,...items){this.value=value;this.items=items;this.target=new.target;}let a=new Item(20,42),b=new Item(30,22);a.value===20&&a.items[0]===42&&a.target===Item&&a.items!==b.items&&b.items[0]===22&&Item.length===0;",
    );
    check(
        "function f(value=1,...items){var value,items;return items;}let a=f(20,42),b=f(20,42);a!==b&&Array.isArray(a)&&a.length===1&&a[0]===42&&b[0]===42&&f(20).length===0&&f.length===0;",
    );
    check(
        "function f(a,b,...items){return [a,b,items];}let x=f();x[0]===undefined&&x[1]===undefined&&Array.isArray(x[2])&&x[2].length===0&&f.length===2;",
    );
    check(
        "let effects=0;function f(value=++effects,...items){var value;return value+items.length;}f(20,1,2)===22&&effects===0&&f(undefined,1)===2&&effects===1&&f()===2&&effects===2;",
    );
    check(
        "function f(value=1,read=()=>value,...items){var value=99;return read();}f(20,()=>22,3)===22&&f(20,undefined,3)===20;",
    );
}

#[test]
fn supplied_rest_uses_intrinsics_without_iterator_or_prototype_setters() {
    check(
        "let proto=Array.prototype;function f(...items){return items;}let a=f(20);Array=function(){throw 99;};let b=f(42);a!==b&&Object.getPrototypeOf(b)===proto&&b[0]===42;",
    );
    check(
        "let hits=0;Object.defineProperty(Array.prototype,'0',{set:function(){hits++;},configurable:true});Array.prototype[Symbol.iterator]=function(){throw 99;};function f(value=1,...items){var value;return items[0];}f(20,42)===42&&hits===0;",
    );
    check(
        "let order='';function source(){order+='a';return 42;}function f(value=1,...items){order+='b';return items[0];}f(source(),source())===42&&order==='aab';",
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

#[test]
fn rest_arrays_preserve_special_values_and_descriptor_transitions() {
    check(
        "function f(...items){return items;}let o={},s=Symbol('s');let a=f(-0,NaN,Infinity,-Infinity);let b=f(undefined,null,true,'text',s,123n,o);1/a[0]===-Infinity&&Number.isNaN(a[1])&&a[2]===Infinity&&a[3]===-Infinity&&b.length===7&&b[0]===undefined&&b[1]===null&&b[2]===true&&b[3]==='text'&&b[4]===s&&b[5]===123n&&b[6]===o&&Object.getPrototypeOf(b)===Array.prototype;",
    );
    check(
        "function f(...items){return items;}let a=f(1,2,3);let d=Object.getOwnPropertyDescriptor(a,'0'),l=Object.getOwnPropertyDescriptor(a,'length');let initial=d.value===1&&d.writable&&d.enumerable&&d.configurable&&l.value===3&&l.writable&&!l.enumerable&&!l.configurable;a[1]={value:42};delete a[0];Object.defineProperty(a,'2',{get(){return 20;},configurable:false});a.length=0;let keys=Reflect.ownKeys(a);initial&&a.length===3&&!(0 in a)&&a[1].value===42&&a[2]===20&&keys.length===3&&keys[0]==='1'&&keys[1]==='2'&&keys[2]==='length';",
    );
}

#[test]
fn rest_materialization_is_shared_by_fallback_arrows_and_generators() {
    check(
        "let arrow=(...items)=>items;function f(value=1,...items){return [arguments[0],items];}function* g(...items){yield items;}let a=arrow(1,2),b=f(undefined,3,4),c=g(5,6).next().value;Array.isArray(a)&&a.length===2&&a[1]===2&&b[0]===undefined&&b[1].length===2&&b[1][0]===3&&c.length===2&&c[1]===6&&arrow()!==arrow();",
    );
}
