//! Shared counters for standalone allocation probes; never linked into timing workers.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

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

pub fn count(iterations: usize, mut operation: impl FnMut()) -> serde_json::Value {
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
