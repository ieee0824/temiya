//! Separate Realm startup timings and allocation requests, outside invocation timing.
use std::{hint::black_box, time::Instant};

use temiya::{
    Engine,
    runtime::{IntrinsicId, Object, PropertyDescriptor, Realm, Value},
};

#[path = "support/allocations.rs"]
mod allocations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("build this probe in release mode".into());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let samples = match args.as_slice() {
        [] => 50,
        [count] => count.parse::<usize>()?,
        _ => return Err("usage: realm_startup_probe [SAMPLES=50]".into()),
    };
    if !(1..=100).contains(&samples) {
        return Err("samples must be 1..100; bound the existing intrinsic cycles".into());
    }

    let realm = Realm::ordinary();
    let mut profiles = Vec::new();
    profiles.push(serde_json::json!({
        "case": "raw native callable; no Realm prototype or metadata",
        "iterations": 1000,
        "allocations": allocations::count(1000, || {
            black_box(Object::native_function_in(realm.allocation_context(), |_, _| Ok(Value::Undefined)));
        }),
    }));
    for name in ["short", "native method with a long UTF-16 name: 日本語😀"] {
        // Both paths define the same actual own descriptors. This reference
        // isolates generic DefineOwnProperty cost; it is not a timing oracle.
        let reference = || {
            let function =
                Object::native_function_in(realm.allocation_context(), |_, _| Ok(Value::Undefined));
            function.set_prototype(Some(realm.intrinsic(IntrinsicId::FunctionPrototype)));
            for (key, value) in [
                ("name", Value::String(name.into())),
                ("length", Value::Number(1.0)),
            ] {
                assert!(
                    function
                        .define_own_property(
                            key.into(),
                            PropertyDescriptor::data(value, false, false, true)
                        )
                        .unwrap()
                );
            }
            function
        };
        let current = || {
            realm
                .create_native_function(name, 1, |_, _| Ok(Value::Undefined))
                .unwrap()
        };
        let (old, new) = (reference(), current());
        assert_eq!(old.own_property_keys(), new.own_property_keys());
        assert_eq!(old.prototype(), new.prototype());
        for key in old.own_property_keys() {
            let a = old.get_own_property(&key).unwrap();
            let b = new.get_own_property(&key).unwrap();
            assert!(a.value.unwrap().same_value(&b.value.unwrap()));
            assert_eq!(
                (a.writable, a.enumerable, a.configurable),
                (b.writable, b.enumerable, b.configurable)
            );
        }
        profiles.push(serde_json::json!({
            "case": "native callable with actual name/length descriptors",
            "name": name, "iterations": 1000, "validated_metadata": true,
            "generic_definition": allocations::count(1000, || { black_box(reference()); }),
            "realm_factory": allocations::count(1000, || { black_box(current()); }),
        }));
    }
    let startup_allocations = allocations::count(10, || {
        let engine = Engine::default();
        black_box(engine.create_realm());
    });
    let mut startup_samples_ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let engine = Engine::default();
        let realm = engine.create_realm();
        let elapsed = start.elapsed().as_nanos();
        black_box(&realm);
        startup_samples_ns.push(elapsed);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "architecture": std::env::consts::ARCH,
            "profile": "release",
            "scope": "Engine::default plus Engine::create_realm; parsing, setup and invocation excluded; drops outside timed interval",
            "allocation_profiles": profiles,
            "startup_allocations": {"iterations":10,"allocations":startup_allocations},
            "allocation_warmup_iterations":128,
            "startup_samples_ns": startup_samples_ns,
            "limitations": "Single process, allocation counters active only for allocation profiles; startup timings collected with counters disabled. Existing intrinsic reference cycles are not collected; sample count bounded. Use isolated workers and alternating paired repetitions, not absolute cross-machine comparisons."
        }))?
    );
    Ok(())
}
