//! Per-execution Test262 host environment. Native callbacks retain only a weak
//! owner: globals cannot create an untraced Rust cycle back to rooted Realms.

use std::{cell::RefCell, rc::Rc};

use temiya::{
    Engine,
    embedding::RealmHandle,
    runtime::{
        ArrayBuffer, Completion, IntrinsicId, Object, PropertyDescriptor, RuntimeError,
        RuntimeResult, ScriptRecord, Value,
    },
};

pub struct HostEnvironment {
    _state: Rc<HostState>,
}

struct HostState {
    engine: Rc<Engine>,
    realms: RefCell<Vec<RealmHandle>>,
}

impl HostEnvironment {
    pub fn install(engine: Rc<Engine>, realm: RealmHandle) -> RuntimeResult<Self> {
        let state = Rc::new(HostState {
            engine,
            realms: RefCell::new(vec![realm]),
        });
        install_realm(&state, 0)?;
        Ok(Self { _state: state })
    }
}

fn install_realm(state: &Rc<HostState>, index: usize) -> RuntimeResult<Object> {
    let realm = state.realms.borrow()[index].clone();
    let global = realm.record().global_object();
    let host = Object::new_in(global.allocation_context());
    if !host.try_set_prototype(Some(realm.record().intrinsic(IntrinsicId::ObjectPrototype)))? {
        return Err(RuntimeError::internal_error(
            "cannot initialize Test262 host prototype",
        ));
    }
    host.create_data_property("global".into(), Value::Object(global.clone()))?;

    let weak = Rc::downgrade(state);
    let create = realm
        .record()
        .create_native_function("createRealm", 0, move |_, _| {
            let state = weak.upgrade().ok_or_else(expired)?;
            let child = state.engine.create_realm();
            let index = {
                let mut realms = state.realms.borrow_mut();
                let index = realms.len();
                realms.push(child);
                index
            };
            install_realm(&state, index).map(Value::Object)
        })?;
    host.create_data_property("createRealm".into(), Value::Object(create))?;

    let weak = Rc::downgrade(state);
    let evaluate = realm
        .record()
        .create_native_function("evalScript", 1, move |_, args| {
            let state = weak.upgrade().ok_or_else(expired)?;
            let Some(Value::String(source)) = args.first() else {
                return Err(RuntimeError::type_error("evalScript requires a string"));
            };
            // Release the registry borrow before JavaScript reenters createRealm,
            // evalScript or the collector. The local handle roots this invocation.
            let realm = state.realms.borrow()[index].clone();
            let source = String::from_utf16(source.code_units()).map_err(|_| {
                RuntimeError::internal_error(
                    "Script parser cannot represent unpaired UTF-16 source units",
                )
            })?;
            let script = ScriptRecord::parse(realm.record().clone(), &source)?;
            match script.evaluate()? {
                Completion::Normal(value) => Ok(value.unwrap_or(Value::Undefined)),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                _ => Err(RuntimeError::internal_error("unexpected Script completion")),
            }
        })?;
    host.create_data_property("evalScript".into(), Value::Object(evaluate))?;

    let detach = realm
        .record()
        .create_native_function("detachArrayBuffer", 1, |_, args| {
            let Some(Value::Object(object)) = args.first() else {
                return Err(RuntimeError::type_error(
                    "detachArrayBuffer requires an ArrayBuffer",
                ));
            };
            let buffer = ArrayBuffer::from_object(object.clone()).ok_or_else(|| {
                RuntimeError::type_error("detachArrayBuffer requires an ArrayBuffer")
            })?;
            // Existing ArrayBuffers have the default undefined [[ArrayBufferDetachKey]].
            if args
                .get(1)
                .is_some_and(|key| !matches!(key, Value::Undefined))
            {
                return Err(RuntimeError::type_error(
                    "ArrayBuffer detach key does not match",
                ));
            }
            buffer.detach();
            Ok(Value::Undefined)
        })?;
    host.create_data_property("detachArrayBuffer".into(), Value::Object(detach))?;

    let weak = Rc::downgrade(state);
    let gc = realm
        .record()
        .create_native_function("gc", 0, move |_, _| {
            let state = weak.upgrade().ok_or_else(expired)?;
            state
                .engine
                .collect_garbage()
                .map_err(|error| RuntimeError::internal_error(error.to_string()))?;
            Ok(Value::Undefined)
        })?;
    host.create_data_property("gc".into(), Value::Object(gc))?;
    // Separate host capabilities must not accidentally satisfy a language
    // TypeError negative through a missing method on undefined.
    for name in ["agent", "AbstractModuleSource"] {
        let getter = realm
            .record()
            .create_native_function(name, 0, move |_, _| {
                Err(RuntimeError::internal_error(format!(
                    "unsupported Test262 host capability: {name}"
                )))
            })?;
        host.define_own_property(
            name.into(),
            PropertyDescriptor::accessor(Some(getter), None, false, true),
        )?;
    }
    global.create_data_property("$262".into(), Value::Object(host.clone()))?;
    Ok(host)
}

fn expired() -> RuntimeError {
    RuntimeError::internal_error("Test262 host environment has ended")
}

#[cfg(test)]
mod tests {
    use super::*;
    use temiya::runtime::{DataView, RuntimeErrorKind, TypedArray};

    #[test]
    fn script_hook_detaches_existing_runtime_views() {
        let engine = Rc::new(Engine::default());
        let realm = engine.create_realm();
        let _host = HostEnvironment::install(engine.clone(), realm.clone()).unwrap();
        let script = engine
            .compile(&realm, "var buffer = new ArrayBuffer(8); buffer;")
            .unwrap();
        let value = engine.evaluate(&script).unwrap();
        let Value::Object(object) = value.value() else {
            panic!("ArrayBuffer object")
        };
        let buffer = ArrayBuffer::from_object(object.clone()).unwrap();
        let typed = TypedArray::new(&buffer, 0, 8).unwrap();
        let view = DataView::new(&buffer, 0, 8).unwrap();
        view.set_uint8(0, 29).unwrap();
        assert!(matches!(typed.get(0).unwrap(), Value::Number(value) if value == 29.0));
        let script = engine
            .compile(&realm, "$262.createRealm().detachArrayBuffer(buffer);")
            .unwrap();
        engine.evaluate(&script).unwrap();
        assert!(buffer.is_detached());
        assert_eq!(buffer.byte_length(), 0);
        assert_eq!(
            typed.get(0).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert_eq!(
            typed.set(0, 1).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert_eq!(
            view.get_uint8(0).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
        assert_eq!(
            view.set_uint8(0, 1).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
    }

    #[test]
    fn global_callbacks_do_not_keep_the_host_owner_alive() {
        let engine = Rc::new(Engine::default());
        let realm = engine.create_realm();
        let host = HostEnvironment::install(engine.clone(), realm.clone()).unwrap();
        let weak = Rc::downgrade(&host._state);
        let script = engine
            .compile(&realm, "$262.createRealm(); $262.gc();")
            .unwrap();
        engine.evaluate(&script).unwrap();
        assert_eq!(host._state.realms.borrow().len(), 2);
        drop(host);
        assert!(
            weak.upgrade().is_none(),
            "global callbacks must use weak ownership"
        );
        engine.collect_garbage().unwrap();
        let script = engine.compile(&realm, "$262.createRealm();").unwrap();
        assert!(engine.evaluate(&script).is_err());
    }
}
