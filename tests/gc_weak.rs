use std::{cell::RefCell, rc::Rc};

use temiya::gc::{FinalizationRegistry, Handle, Heap, Trace, Tracer};

struct Node {
    edges: Vec<Handle<Node>>,
    conditional: Vec<(Handle<Node>, Handle<Node>)>,
}

impl Trace for Node {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        for edge in &self.edges {
            tracer.mark(*edge);
        }
        for &(key, value) in &self.conditional {
            tracer.ephemeron(key, value);
        }
    }
}

fn node() -> Node {
    Node {
        edges: Vec::new(),
        conditional: Vec::new(),
    }
}

#[test]
fn automatic_collection_preserves_edges_in_the_incoming_allocation() {
    let mut heap = Heap::stress();
    let target = heap.allocate(node());
    let handle = target.handle();
    drop(target);
    let owner = heap.allocate(Node {
        edges: vec![handle],
        conditional: Vec::new(),
    });
    assert!(heap.with(handle, |_| ()).is_some());
    heap.verify().unwrap();
    assert_eq!(heap.live_allocations(), 2);
    drop(owner);
    assert_eq!(heap.collect().reclaimed, 2);
}

#[test]
fn owned_ephemerons_require_a_live_owner_and_do_not_bootstrap_key_cycles() {
    let mut heap = Heap::new();
    let owner = heap.allocate(node());
    let key = heap.allocate(node());
    let value = heap.allocate(node());
    let value_handle = value.handle();
    heap.with_mut(owner.handle(), |node| {
        node.conditional.push((key.handle(), value_handle))
    })
    .unwrap();
    heap.with_mut(value_handle, |node| node.edges.push(key.handle()))
        .unwrap();
    drop(value);
    assert_eq!(heap.collect().retained, 3);
    drop(owner);
    assert_eq!(heap.collect().reclaimed, 2);
    assert!(heap.with(value_handle, |_| ()).is_none());
    let owner = heap.allocate(node());
    let value = heap.allocate(node());
    heap.with_mut(owner.handle(), |node| {
        node.conditional.push((key.handle(), value.handle()))
    })
    .unwrap();
    heap.with_mut(value.handle(), |node| node.edges.push(key.handle()))
        .unwrap();
    drop((key, value));
    assert_eq!(heap.collect().retained, 1);
    heap.verify().unwrap();
}

#[test]
fn owned_ephemerons_discover_nested_owners_at_a_fixed_point() {
    let mut heap = Heap::new();
    let owner = heap.allocate(node());
    let key = heap.allocate(node());
    let nested = heap.allocate(node());
    let second_key = heap.allocate(node());
    let value = heap.allocate(node());
    heap.with_mut(owner.handle(), |node| {
        node.conditional.push((key.handle(), nested.handle()))
    })
    .unwrap();
    heap.with_mut(nested.handle(), |node| {
        node.edges.push(second_key.handle());
        node.conditional.push((second_key.handle(), value.handle()));
    })
    .unwrap();
    drop((nested, second_key, value));
    assert_eq!(heap.collect().retained, 5);
    drop(key);
    assert_eq!(heap.collect().retained, 1);
}

#[test]
fn owned_ephemeron_barriers_cover_young_values_and_minor_collection() {
    let mut heap = Heap::new();
    let owner = heap.allocate(node());
    let key = heap.allocate(node());
    heap.collect();
    let value = heap.allocate(node());
    let value_handle = value.handle();
    heap.with_mut(owner.handle(), |node| {
        node.conditional.push((key.handle(), value_handle))
    })
    .unwrap();
    assert!(heap.verify_write_barriers().is_err());
    heap.write_barrier(owner.handle(), value_handle).unwrap();
    heap.verify_write_barriers().unwrap();
    drop(value);
    assert_eq!(heap.collect_minor().reclaimed, 0);
    assert!(heap.with(value_handle, |_| ()).is_some());
    drop(owner);
    assert_eq!(heap.collect().retained, 1);
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

#[test]
fn minor_owned_ephemerons_distinguish_live_young_keys_and_unreachable_owners() {
    for live_key in [true, false] {
        let mut heap = Heap::new();
        let owner = heap.allocate(node());
        heap.collect();
        let key = heap.allocate(node());
        let value = heap.allocate(node());
        let key_handle = key.handle();
        let value_handle = value.handle();
        heap.with_mut(owner.handle(), |node| {
            node.conditional.push((key_handle, value_handle))
        })
        .unwrap();
        heap.with_mut(value_handle, |node| node.edges.push(key_handle))
            .unwrap();
        heap.write_barrier(owner.handle(), key_handle).unwrap();
        heap.write_barrier(owner.handle(), value_handle).unwrap();
        heap.verify_write_barriers().unwrap();
        drop(value);
        let key = live_key.then_some(key);
        assert_eq!(heap.collect_minor().reclaimed, if live_key { 0 } else { 2 });
        heap.verify().unwrap();
        drop(key);
        assert_eq!(heap.collect().retained, 1);
    }
    let mut heap = Heap::new();
    let old_key = heap.allocate(node());
    heap.collect();
    let owner = heap.allocate(node());
    let value = heap.allocate(node());
    heap.with_mut(owner.handle(), |node| {
        node.conditional.push((old_key.handle(), value.handle()))
    })
    .unwrap();
    drop((owner, value));
    assert_eq!(heap.collect_minor().reclaimed, 2);
    assert_eq!(heap.live_allocations(), 1);
}
