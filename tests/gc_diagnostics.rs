use temiya::gc::{Handle, Heap, HeapVerificationError, Trace, Tracer};

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
fn stress_mode_collects_before_every_allocation() {
    let mut heap = Heap::stress();
    let first = heap.allocate(node());
    drop(first);
    let second = heap.allocate(node());
    drop(second);
    let diagnostics = heap.diagnostics();
    assert_eq!(diagnostics.collections, 2);
    assert_eq!(diagnostics.reclaimed_total, 1);
    assert_eq!(diagnostics.live, 1);
    assert_eq!(diagnostics.peak_live, 1);
    assert_eq!(diagnostics.slots, 1);
}

#[test]
fn verification_detects_a_stale_traced_edge() {
    let mut heap = Heap::new();
    let owner = heap.allocate(node());
    let target = heap.allocate(node());
    let stale = target.handle();
    drop(target);
    heap.collect();
    heap.with_mut(owner.handle(), |node| node.edges.push(stale));

    assert!(matches!(
        heap.verify(),
        Err(HeapVerificationError::DanglingEdge { edge, .. }) if edge == stale.erase()
    ));
}

#[test]
fn diagnostics_track_roots_slots_and_reclamation() {
    let mut heap = Heap::new();
    let live = heap.allocate(node());
    let dead = heap.allocate(node());
    drop(dead);
    let stats = heap.collect();
    assert_eq!(stats.reclaimed, 1);
    assert!(heap.verify().is_ok());

    let diagnostics = heap.diagnostics();
    assert_eq!(diagnostics.live, 1);
    assert_eq!(diagnostics.rooted, 1);
    assert_eq!(diagnostics.free, 1);
    assert_eq!(diagnostics.peak_live, 2);
    drop(live);
}

#[test]
fn empty_heap_verifies_and_reports_zeroes() {
    let heap = Heap::new();
    assert!(heap.verify().is_ok());
    assert_eq!(heap.diagnostics().live, 0);
    assert_eq!(heap.diagnostics().collections, 0);
}
