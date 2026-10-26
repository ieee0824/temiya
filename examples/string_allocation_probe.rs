//! Allocation counts only: allocator instrumentation is excluded from timing workers.
use std::hint::black_box;
use temiya::runtime::{JsString, Value};
#[path = "support/allocations.rs"]
mod allocations;
use allocations::count;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() > 1 {
        return Err("usage: string_allocation_probe [ITERATIONS=10000]".into());
    }
    let n = args
        .first()
        .map_or(Ok(10_000), |arg| arg.parse::<usize>())?;
    if !(1..=10_000_000).contains(&n) {
        return Err("iterations must be 1..10000000".into());
    }
    let mut rows = Vec::new();
    for (name, text) in [
        ("tiny", "key"),
        ("short", "charCodeAt"),
        (
            "long",
            "a shared immutable UTF-16 string whose buffer is longer than the inline capacity",
        ),
    ] {
        let old = text.encode_utf16().collect::<Vec<_>>();
        let new = JsString::from(text);
        let old_construct = count(n, || {
            black_box(black_box(text).encode_utf16().collect::<Vec<_>>());
        });
        let new_construct = count(n, || {
            black_box(JsString::from(black_box(text)));
        });
        let old_clone = count(n, || {
            black_box(black_box(&old).clone());
        });
        let new_clone = count(n, || {
            black_box(black_box(&new).clone());
        });
        rows.push(serde_json::json!({"case":name,"utf16_length":old.len(),"vec_construct":old_construct,"compact_construct":new_construct,"vec_clone":old_clone,"compact_clone":new_clone}));
    }
    let concat_iterations = n.min(100);
    let mut concatenations = Vec::new();
    for (name, chunk, steps) in [
        ("inline-pair", "ab", 2),
        ("append-chain", "0123456789abcdef", 128),
    ] {
        let chunk = JsString::from(chunk);
        let flat = count(concat_iterations, || {
            let mut string = JsString::default();
            for _ in 0..steps {
                // The interpreter's pre-rope concatenation, including Vec growth.
                let mut units = string.code_units().to_vec();
                units.extend_from_slice(chunk.code_units());
                string = JsString::from_code_units(units);
            }
            assert_eq!(string.len(), chunk.len() * steps);
            black_box(string.code_units());
        });
        let compact = count(concat_iterations, || {
            let mut string = JsString::default();
            for _ in 0..steps {
                string = string.concat(chunk.clone());
            }
            assert_eq!(string.len(), chunk.len() * steps);
            // Include final flattening, rather than counting an unobserved rope.
            black_box(string.code_units());
        });
        concatenations.push(serde_json::json!({"case":name,"steps":steps,"chunk_units":chunk.len(),"flat":flat,"compact":compact}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"iterations":n,"warmup_iterations":128,"scope":"single-thread standalone allocation requests, including realloc; not Engine throughput or RSS","timing_instrumentation":false,"js_string_bytes":std::mem::size_of::<JsString>(),"vec_bytes":std::mem::size_of::<Vec<u16>>(),"value_bytes":std::mem::size_of::<Value>(),"pointer_width":usize::BITS,"cases":rows,"concat_iterations":concat_iterations,"concatenations":concatenations})
        )?
    );
    Ok(())
}
