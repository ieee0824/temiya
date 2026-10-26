//! Public-Engine ownership observations shared by the probe and regressions.
use std::{error::Error, io, rc::Rc};
use temiya::{Engine, runtime::Value};

pub struct Case {
    pub name: &'static str,
    pub source: &'static str,
    pub constructor_root: bool,
    pub callable_root: bool,
}

pub const CASES: &[Case] = &[
    Case {
        name: "acyclic-object",
        source: "var value = {probe: marker}; delete marker; value;",
        constructor_root: false,
        callable_root: false,
    },
    Case {
        name: "self-cycle",
        source: "var value = {probe: marker}; value.self = value; delete marker; value;",
        constructor_root: false,
        callable_root: false,
    },
    Case {
        name: "two-object-cycle",
        source: "var value = {probe: marker}; value.other = {owner: value}; delete marker; value;",
        constructor_root: false,
        callable_root: false,
    },
    Case {
        name: "arrow-function",
        source: "var value = () => 42; value.probe = marker; delete marker; value;",
        constructor_root: false,
        callable_root: true,
    },
    Case {
        name: "ordinary-function",
        source: "var value = function() {return 42;}; value.probe = marker; delete marker; value;",
        constructor_root: false,
        callable_root: true,
    },
    Case {
        name: "escaped-prototype",
        source: "var value = function() {return 42;}; value.probe = marker; var escaped = value.prototype; delete marker; escaped;",
        constructor_root: true,
        callable_root: true,
    },
    Case {
        name: "lexical-closure",
        source: "var value = (function(m) {return function() {return m();};})(marker); value.probe = marker; delete marker; value;",
        constructor_root: false,
        callable_root: true,
    },
];

#[derive(Debug)]
pub struct Observation {
    pub name: &'static str,
    pub alive_with_handle: bool,
    pub alive_with_clone: bool,
    pub alive_after_drop: bool,
}

fn ensure(condition: bool, message: &str) -> Result<(), Box<dyn Error>> {
    if condition {
        Ok(())
    } else {
        Err(io::Error::other(message).into())
    }
}

pub fn observe(case: &Case) -> Result<Observation, Box<dyn Error>> {
    let engine = Engine::default();
    let realm = engine.create_realm();
    let token = Rc::new(());
    let weak = Rc::downgrade(&token);
    engine.register_global_function(&realm, "marker", move |context, _, _| {
        // Capture a host-owned token without exposing a runtime weak-pointer API.
        let _ = Rc::strong_count(&token);
        Ok(context.value(Value::Number(42.0)))
    })?;
    let script = engine.compile(&realm, case.source)?;
    let root = engine.evaluate(&script)?;
    let clone = root.clone();
    let cleanup = engine.compile(&realm, "value = null; var escaped = null;")?;
    drop(engine.evaluate(&cleanup)?);
    drop(cleanup);
    drop(script);
    drop(realm);
    drop(engine);
    let alive_with_handle = weak.upgrade().is_some();
    ensure(
        alive_with_handle,
        "host token lost while a value handle is live",
    )?;
    let Value::Object(root_object) = root.value() else {
        return Err(io::Error::other("fixture did not return an object").into());
    };
    // A retained prototype must retain its constructor, and an escaped closure
    // must retain its lexical captures, even after the Engine has been dropped.
    let target = if case.constructor_root {
        root_object.get(&"constructor".into())?
    } else {
        Value::Object(root_object.clone())
    };
    let Value::Object(target) = target else {
        return Err(io::Error::other("escaped prototype lost its constructor").into());
    };
    if case.callable_root {
        ensure(
            matches!(target.call(Value::Undefined, &[])?, Value::Number(42.0)),
            "live function/capture became invalid",
        )?;
    }
    let Value::Object(marker) = target.get(&"probe".into())? else {
        return Err(io::Error::other("live object lost its host callback").into());
    };
    ensure(
        matches!(marker.call(Value::Undefined, &[])?, Value::Number(42.0)),
        "host callback became invalid",
    )?;
    drop(marker);
    drop(target);
    drop(root);
    let alive_with_clone = weak.upgrade().is_some();
    ensure(
        alive_with_clone,
        "dropping one handle invalidated its clone",
    )?;
    drop(clone);
    Ok(Observation {
        name: case.name,
        alive_with_handle,
        alive_with_clone,
        alive_after_drop: weak.upgrade().is_some(),
    })
}
