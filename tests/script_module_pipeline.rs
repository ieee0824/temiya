use std::{cell::RefCell, rc::Rc};

use temiya::runtime::{
    Completion, JsString, ModuleGraph, ModuleStatus, Realm, RuntimeError, ScriptRecord, Value,
};

#[test]
fn scripts_parse_and_evaluate_in_their_realm() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let script = ScriptRecord::parse(
        Realm::ordinary_in(&allocation_context),
        "let value = 40; value + 2;",
    )
    .unwrap();
    let Completion::Normal(Some(value)) = script.evaluate().unwrap() else {
        panic!("expected normal Script completion")
    };
    assert!(value.same_value(&Value::Number(42.0)));
}

fn graph(sources: &[(&str, &str)]) -> (ModuleGraph, Rc<RefCell<Vec<String>>>) {
    let allocation_context = temiya::runtime::AllocationContext::new();
    let sources = sources
        .iter()
        .map(|(name, source)| ((*name).to_owned(), (*source).to_owned()))
        .collect::<Vec<_>>();
    let loads = Rc::new(RefCell::new(Vec::new()));
    let observed = loads.clone();
    let graph = ModuleGraph::new(Realm::ordinary_in(&allocation_context), move |request| {
        let name = String::from_utf16_lossy(&request.specifier);
        observed.borrow_mut().push(name.clone());
        sources
            .iter()
            .find(|(candidate, _)| candidate == &name)
            .map(|(_, source)| source.clone())
            .ok_or_else(|| RuntimeError::reference_error(format!("unknown module {name}")))
    });
    (graph, loads)
}

#[test]
fn cyclic_dependencies_resolve_and_load_each_module_once() {
    let (graph, loads) = graph(&[
        (
            "a",
            "import { getB } from 'b'; export function getA() { return 1; } \
             export function result() { return getB(); }",
        ),
        (
            "b",
            "import { getA } from 'a'; export function getB() { return getA() + 1; }",
        ),
    ]);
    let module = graph.load("a").unwrap();
    module.link().unwrap();
    module.evaluate().unwrap();

    assert_eq!(module.status(), ModuleStatus::Evaluated);
    let Value::Object(result) = module.get_export(&JsString::from("result")).unwrap() else {
        panic!("expected exported function")
    };
    assert!(
        result
            .call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(2.0))
    );
    assert_eq!(&*loads.borrow(), &["a", "b"]);
}

#[test]
fn imports_are_live_bindings_after_dependency_evaluation() {
    let (graph, _) = graph(&[
        (
            "counter",
            "export let value = 1; export function increment() { value++; }",
        ),
        (
            "reader",
            "import { value, increment } from 'counter'; \
             export function read() { increment(); return value; }",
        ),
    ]);
    let reader = graph.load("reader").unwrap();
    reader.link().unwrap();
    reader.evaluate().unwrap();
    let Value::Object(read) = reader.get_export(&JsString::from("read")).unwrap() else {
        panic!("expected read function")
    };

    assert!(
        read.call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(2.0))
    );
    assert!(
        read.call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(3.0))
    );
}

#[test]
fn named_and_star_reexports_resolve_to_original_bindings() {
    let (graph, _) = graph(&[
        ("source", "export let value = 7;"),
        ("middle", "export { value as renamed } from 'source';"),
        ("entry", "export * from 'middle';"),
    ]);
    let entry = graph.load("entry").unwrap();
    entry.link().unwrap();
    entry.evaluate().unwrap();

    assert!(
        entry
            .get_export(&JsString::from("renamed"))
            .unwrap()
            .same_value(&Value::Number(7.0))
    );
}

#[test]
fn namespace_imports_and_reexports_expose_live_getters() {
    let (graph, _) = graph(&[
        (
            "source",
            "export let value = 1; export function increment() { value++; }",
        ),
        ("namespace", "export * as ns from 'source';"),
        (
            "entry",
            "import { ns } from 'namespace'; \
             export function read() { ns.increment(); return ns.value; }",
        ),
    ]);
    let entry = graph.load("entry").unwrap();
    entry.evaluate().unwrap();
    let Value::Object(read) = entry.get_export(&JsString::from("read")).unwrap() else {
        panic!("expected read function")
    };

    assert!(
        read.call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(2.0))
    );
    assert!(
        read.call(Value::Undefined, &[])
            .unwrap()
            .same_value(&Value::Number(3.0))
    );
}

#[test]
fn top_level_await_produces_a_cached_evaluation_promise() {
    let (graph, _) = graph(&[("async", "export const answer = await 42;")]);
    let module = graph.load("async").unwrap();
    let first = module.evaluate().unwrap();
    let second = module.evaluate().unwrap();

    assert_eq!(module.status(), ModuleStatus::Evaluated);
    assert_eq!(first.object(), second.object());
    assert!(
        module
            .get_export(&JsString::from("answer"))
            .unwrap()
            .same_value(&Value::Number(42.0))
    );
}

#[test]
fn evaluation_failure_is_cached_and_not_reexecuted() {
    let (graph, loads) = graph(&[("broken", "throw 'failure';")]);
    let module = graph.load("broken").unwrap();
    let first = module.evaluate().unwrap_err();
    let second = module.evaluate().unwrap_err();

    assert_eq!(module.status(), ModuleStatus::Errored);
    assert_eq!(first, second);
    assert_eq!(&*loads.borrow(), &["broken"]);
}
