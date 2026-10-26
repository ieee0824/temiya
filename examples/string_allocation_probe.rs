//! Allocation counts only: allocator instrumentation is excluded from timing workers.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use temiya::runtime::{JsString, Value};

struct CountingAllocator;
static ENABLED: AtomicBool = AtomicBool::new(false);
static REQUESTS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
fn record(bytes: usize) {
    if ENABLED.load(Ordering::Relaxed) {
        REQUESTS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(bytes, Ordering::Relaxed);
    }
}
// SAFETY: Every operation forwards the original pointer/layout unchanged to
// System. The counters allocate no memory and never access the returned blocks.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: The caller supplied System's required allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        // SAFETY: The caller supplied System's required allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies a live allocation and its original layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        // SAFETY: Pointer, layout and new size are forwarded without alteration.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn count(iterations: usize, mut operation: impl FnMut()) -> serde_json::Value {
    for _ in 0..128 {
        operation();
    }
    REQUESTS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        operation();
    }
    ENABLED.store(false, Ordering::Relaxed);
    serde_json::json!({"allocation_requests": REQUESTS.load(Ordering::Relaxed), "requested_bytes": BYTES.load(Ordering::Relaxed)})
}

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
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"iterations":n,"warmup_iterations":128,"scope":"single-thread standalone allocation requests, including realloc; not Engine throughput or RSS","timing_instrumentation":false,"js_string_bytes":std::mem::size_of::<JsString>(),"vec_bytes":std::mem::size_of::<Vec<u16>>(),"value_bytes":std::mem::size_of::<Value>(),"pointer_width":usize::BITS,"cases":rows})
        )?
    );
    Ok(())
}
