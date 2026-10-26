use temiya::gc::{Handle, Heap, Trace, Tracer};

struct Node {
    value: i32,
    child: Option<Handle<Node>>,
}

impl Trace for Node {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Some(child) = self.child {
            tracer.mark(child);
        }
    }
}

#[test]
fn handles_use_scoped_typed_access() {
    let mut heap = Heap::new();
    let root = heap.allocate(Node {
        value: 1,
        child: None,
    });
    let handle = root.handle();
    assert_eq!(heap.with(handle, |node| node.value), Some(1));
    heap.with_mut(handle, |node| node.value += 1).unwrap();
    assert_eq!(heap.with(handle, |node| node.value), Some(2));
}

#[test]
fn roots_are_counted_by_raii() {
    let mut heap = Heap::new();
    let root = heap.allocate(Node {
        value: 0,
        child: None,
    });
    let handle = root.handle();
    assert_eq!(heap.root_count(handle), 1);
    let second = root.clone();
    assert_eq!(heap.root_count(handle), 2);
    drop(second);
    assert_eq!(heap.root_count(handle), 1);
    drop(root);
    assert_eq!(heap.root_count(handle), 0);
    let rerooted = heap.root(handle).unwrap();
    assert_eq!(heap.root_count(rerooted.handle()), 1);
}

#[test]
fn handles_cannot_be_used_with_another_heap() {
    let mut first = Heap::new();
    let root = first.allocate(Node {
        value: 7,
        child: None,
    });
    let second = Heap::new();
    assert_eq!(second.with(root.handle(), |node| node.value), None);
    assert!(second.root(root.handle()).is_none());
}

#[test]
fn tracing_reports_roots_and_outgoing_edges() {
    let mut heap = Heap::new();
    let child = heap.allocate(Node {
        value: 1,
        child: None,
    });
    let child_handle = child.handle();
    let parent = heap.allocate(Node {
        value: 2,
        child: Some(child_handle),
    });
    drop(child);

    let mut seen = Vec::new();
    heap.trace_roots(&mut |handle| seen.push(handle));
    assert!(seen.contains(&parent.handle().erase()));
    assert!(seen.contains(&child_handle.erase()));
}
