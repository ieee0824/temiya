use std::{cell::Cell, rc::Rc};

use temiya::gc::{Handle, Heap, HeapOptions, OutOfMemory, Trace, Tracer};

struct Node {
    edge: Option<Handle<Node>>,
    drops: Rc<Cell<usize>>,
}

impl Trace for Node {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Some(edge) = self.edge {
            tracer.mark(edge);
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn node(drops: &Rc<Cell<usize>>) -> Node {
    Node {
        edge: None,
        drops: drops.clone(),
    }
}

#[test]
fn collector_reclaims_unrooted_cycles() {
    let drops = Rc::new(Cell::new(0));
    let mut heap = Heap::new();
    let first = heap.allocate(node(&drops));
    let second = heap.allocate(node(&drops));
    let first_handle = first.handle();
    let second_handle = second.handle();
    heap.with_mut(first_handle, |value| value.edge = Some(second_handle));
    heap.with_mut(second_handle, |value| value.edge = Some(first_handle));
    drop(first);
    drop(second);

    let stats = heap.collect();
    assert_eq!(stats.reclaimed, 2);
    assert_eq!(stats.retained, 0);
    assert_eq!(drops.get(), 2);
    assert!(heap.with(first_handle, |_| ()).is_none());
}

#[test]
fn collector_retains_the_transitive_root_graph() {
    let drops = Rc::new(Cell::new(0));
    let mut heap = Heap::new();
    let child = heap.allocate(node(&drops));
    let child_handle = child.handle();
    let parent = heap.allocate(Node {
        edge: Some(child_handle),
        drops: drops.clone(),
    });
    drop(child);

    assert_eq!(heap.collect().retained, 2);
    assert!(heap.with(child_handle, |_| ()).is_some());
    drop(parent);
    assert_eq!(heap.collect().reclaimed, 2);
}

#[test]
fn allocation_threshold_collects_before_reusing_a_slot() {
    let drops = Rc::new(Cell::new(0));
    let mut heap = Heap::with_options(HeapOptions {
        allocation_threshold: 1,
        max_allocations: None,
    });
    let old = heap.allocate(node(&drops));
    let stale = old.handle();
    drop(old);
    let replacement = heap.allocate(node(&drops));

    assert_eq!(drops.get(), 1);
    assert_eq!(heap.live_allocations(), 1);
    assert!(heap.with(stale, |_| ()).is_none());
    assert!(heap.with(replacement.handle(), |_| ()).is_some());
}

#[test]
fn allocation_limit_reports_oom_only_after_collection() {
    let drops = Rc::new(Cell::new(0));
    let mut heap = Heap::with_options(HeapOptions {
        allocation_threshold: usize::MAX,
        max_allocations: Some(1),
    });
    let live = heap.try_allocate(node(&drops)).unwrap();
    assert!(matches!(heap.try_allocate(node(&drops)), Err(OutOfMemory)));
    assert_eq!(heap.live_allocations(), 1);

    drop(live);
    assert!(heap.try_allocate(node(&drops)).is_ok());
}
