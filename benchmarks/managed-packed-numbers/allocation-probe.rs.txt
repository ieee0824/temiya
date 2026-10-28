//! Rust allocation requests for clearing/refilling an existing numeric array.
//! The metered phase excludes key preparation and bootstrap; it is not a timer.
use temiya::runtime::{AllocationContext, ArrayObject, PropertyKey, Value};

#[path = "support/allocations.rs"]
mod allocations;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() > 1 {
        return Err("usage: array_allocation_probe [ELEMENTS=4096]".into());
    }
    let count = args.first().map_or(Ok(4096), |s| s.parse::<usize>())?;
    if !(1..=65_536).contains(&count) {
        return Err("elements must be 1..65536".into());
    }
    let context = AllocationContext::new();
    let array = ArrayObject::new(&context, 0)?;
    let object = array.object();
    let keys: Vec<_> = (0..count)
        .map(|index| PropertyKey::from(index.to_string().as_str()))
        .collect();
    let length = PropertyKey::from("length");
    let requests = allocations::count(1, || {
        assert!(object.set(&length, Value::Number(0.0)).unwrap());
        for (index, key) in keys.iter().enumerate() {
            assert!(
                object
                    .create_data_property(key.clone(), Value::Number(index as f64))
                    .unwrap()
            );
        }
    });
    assert_eq!(array.length(), count as u32);
    let mut sum = 0.0;
    for key in &keys {
        let Value::Number(value) = object.get(key)? else {
            return Err("numeric array read returned a non-number".into());
        };
        sum += value;
    }
    let expected = (count * (count - 1) / 2) as f64;
    assert_eq!(sum, expected);
    drop((object, array));
    assert_eq!(context.collect_garbage()?.retained, 0);
    println!(
        "{}",
        serde_json::json!({
            "format_version": 1,
            "architecture": std::env::consts::ARCH,
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "elements": count,
            "warmup_refills": 128,
            "measured_refills": 1,
            "expected_sum": expected,
            "validated_sum": sum,
            "allocation_requests": requests["allocation_requests"],
            "requested_bytes": requests["requested_bytes"],
            "final_retained_cells": 0,
            "scope": "Public ArrayObject clear/default-number refill; prebuilt keys, allocation requests only, not peak/RSS or competitive timing"
        })
    );
    Ok(())
}
