use temiya::gc::{Handle, Heap, Trace, Tracer, WriteBarrierError};

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

#[test]
fn write_barrier_keeps_young_values_reachable_from_old_values() {
    let mut heap = Heap::new();
    let owner = heap.allocate(Node { edges: Vec::new() });
    let owner_handle = owner.handle();
    heap.collect();

    let young = heap.allocate(Node { edges: Vec::new() });
    let young_handle = young.handle();
    heap.with_mut(owner_handle, |node| node.edges.push(young_handle));
    heap.write_barrier(owner_handle, young_handle).unwrap();
    drop(young);

    let stats = heap.collect_minor();
    assert_eq!(stats.reclaimed, 0);
    assert!(heap.with(young_handle, |_| ()).is_some());
    assert!(heap.verify_write_barriers().is_ok());
}

#[test]
fn verifier_detects_a_missing_old_to_young_barrier() {
    let mut heap = Heap::new();
    let owner = heap.allocate(Node { edges: Vec::new() });
    let owner_handle = owner.handle();
    heap.collect();
    let young = heap.allocate(Node { edges: Vec::new() });
    let young_handle = young.handle();
    heap.with_mut(owner_handle, |node| node.edges.push(young_handle));

    assert!(matches!(
        heap.verify_write_barriers(),
        Err(WriteBarrierError::MissingBarrier(_))
    ));
}

#[test]
fn minor_collection_matches_full_collection_under_stress() {
    fn exercise(minor: bool) -> (usize, usize) {
        let mut heap = Heap::new();
        let owner = heap.allocate(Node { edges: Vec::new() });
        let owner_handle = owner.handle();
        heap.collect();
        let mut retained = Vec::new();
        for index in 0..200 {
            let value = heap.allocate(Node { edges: Vec::new() });
            let handle = value.handle();
            if index % 3 == 0 {
                heap.with_mut(owner_handle, |node| node.edges.push(handle));
                heap.write_barrier(owner_handle, handle).unwrap();
                retained.push(value);
            } else {
                drop(value);
            }
        }
        for value in retained {
            drop(value);
        }
        let stats = if minor {
            heap.collect_minor()
        } else {
            heap.collect()
        };
        (stats.retained, stats.reclaimed)
    }

    assert_eq!(exercise(true), exercise(false));
}

#[test]
fn barrier_rejects_dead_targets() {
    let mut heap = Heap::new();
    let owner = heap.allocate(Node { edges: Vec::new() });
    heap.collect();
    let target = heap.allocate(Node { edges: Vec::new() });
    let target_handle = target.handle();
    drop(target);
    heap.collect_minor();
    assert_eq!(
        heap.write_barrier(owner.handle(), target_handle),
        Err(WriteBarrierError::InvalidTarget)
    );
}
