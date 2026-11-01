use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use temiya::{
    Engine,
    runtime::{Object, Value},
};

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn thread_local_engine_releases_payloads_after_object_storage_tls_teardown() {
    let drops = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&drops);
    std::thread::spawn(move || {
        thread_local! { static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) }; }
        // Initialize the host TLS before any Realm/Object is allocated, so the
        // host Engine outlives optional object storage during TLS destruction.
        ENGINE.with(|slot| *slot.borrow_mut() = Some(Engine::default()));
        ENGINE.with(|slot| {
            let borrowed = slot.borrow();
            let engine = borrowed.as_ref().unwrap();
            let realm = engine.create_realm();
            let marker = Dropped(counter);
            let callback = Object::native_function_in(engine.allocation_context(), move |_, _| {
                let _ = &marker;
                Ok(Value::Undefined)
            });
            engine
                .set_global(
                    &realm,
                    "marker",
                    engine.value(&realm, Value::Object(callback)).unwrap(),
                )
                .unwrap();
            let script = engine
                .compile(&realm, "let map=new Map();map.set('answer',42);marker();")
                .unwrap();
            engine.evaluate(&script).unwrap();
        });
    })
    .join()
    .unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
