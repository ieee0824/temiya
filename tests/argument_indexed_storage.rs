use temiya::{
    Engine,
    embedding::ScriptBackend,
    runtime::{Object, Value},
};

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
            "{backend:?}"
        );
    }
}

#[test]
fn indexed_arguments_keep_live_descriptors_after_delete_reinsert_and_accessors() {
    check(
        "function make(){'use strict';return arguments;}let values=[];for(let i=0;i<64;i++)values[i]=i;let a=make(...values);let before=a.length===64&&a[63]===63;delete a[0];delete a[32];a[0]=100;a[32]=200;let calls=0;Object.defineProperty(a,'16',{get(){calls++;return this===a?416:0;},configurable:true});a['01']=77;let d=Object.getOwnPropertyDescriptor(a,'1');before&&a[1]===1&&a[63]===63&&a[0]===100&&a[32]===200&&a[16]===416&&calls===1&&a['01']===77&&a.length===64&&d.value===1&&d.writable&&d.enumerable&&d.configurable;",
    );
    check("function f(x){'use strict';arguments[0]=42;return x;}f(41)===41;");
}

#[test]
fn numeric_lookup_fallback_keeps_inherited_accessor_receiver() {
    check(
        "let calls=0;let proto={get 63(){calls++;return this.marker;}};let o=Object.create(proto);for(let i=0;i<32;i++)o[i]=i;o.marker=42;let ok=o[31]===31&&o[63]===42&&calls===1;delete o[1];ok&&o[2]===2&&o[63]===42&&calls===2&&o[1]===undefined;",
    );
}

#[test]
fn arguments_values_survive_gc_and_release_their_cycles() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = Object::native_function_with_context(
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
            let script = engine.compile(&realm, "function keep(){'use strict';return arguments;}let a=keep({value:41},{value:42});gcNow();let retained=a[1];let before=a[0].value===41&&retained.value===42;delete a[0];a[1]=a;gcNow();before&&a[1]===a&&retained.value===42;").unwrap();
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
