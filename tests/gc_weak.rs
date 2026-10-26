use std::{cell::RefCell, rc::Rc};

use temiya::gc::{FinalizationRegistry, Handle, Heap, Trace, Tracer};

struct Node {
    edges: Vec<Handle<Node>>,
}

impl Trace for Node {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        for edge in &self.edges {
            tracer.mark(*edge);
        }
    }
}

fn node() -> Node {
    Node { edges: Vec::new() }
}

#[test]
fn ephemerons_reach_a_fixpoint_without_retaining_keys() {
    let mut heap = Heap::new();
    let first_key = heap.allocate(node());
    let second_key = heap.allocate(node());
    let value = heap.allocate(node());
    let second_handle = second_key.handle();
    let value_handle = value.handle();
    heap.add_ephemeron(second_handle, value_handle).unwrap();
    heap.add_ephemeron(first_key.handle(), second_handle)
        .unwrap();
    drop(second_key);
    drop(value);

    assert_eq!(heap.collect().retained, 3);
    assert!(heap.with(value_handle, |_| ()).is_some());
    drop(first_key);
    assert_eq!(heap.collect().reclaimed, 3);
}

#[test]
fn weak_handles_never_keep_targets_alive() {
    let mut heap = Heap::new();
    let target = heap.allocate(node());
    let weak = target.handle().downgrade();
    assert!(weak.upgrade(&heap).is_some());
    drop(target);
    heap.collect();
    assert!(weak.upgrade(&heap).is_none());
}

#[test]
fn cleanup_runs_after_sweep_in_registration_order() {
    let mut heap = Heap::new();
    let observed = Rc::new(RefCell::new(Vec::new()));
    let output = observed.clone();
    let registry = FinalizationRegistry::new(&mut heap, move |held| {
        output.borrow_mut().push(held);
    });
    let first = heap.allocate(node());
    let second = heap.allocate(node());
    let first_weak = first.handle().downgrade();
    registry.register(first.handle(), "first");
    registry.register(second.handle(), "second");
    drop(first);
    drop(second);

    heap.collect();
    assert!(first_weak.upgrade(&heap).is_none());
    assert_eq!(registry.pending_len(), 2);
    assert!(observed.borrow().is_empty());
    heap.run_cleanup_jobs();
    assert_eq!(&*observed.borrow(), &["first", "second"]);
    assert_eq!(registry.pending_len(), 0);
}

#[test]
fn live_targets_do_not_schedule_cleanup() {
    let mut heap = Heap::new();
    let registry = FinalizationRegistry::new(&mut heap, |_: ()| panic!("unexpected cleanup"));
    let target = heap.allocate(node());
    registry.register(target.handle(), ());
    heap.collect();
    heap.run_cleanup_jobs();
    assert_eq!(registry.pending_len(), 0);
}
