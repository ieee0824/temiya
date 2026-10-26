#![no_main]

use libfuzzer_sys::fuzz_target;
use temiya::{
    gc::{Handle, Heap, Root, Trace, Tracer},
    runtime::{Realm, RuntimeError, ScriptRecord},
};

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

fuzz_target!(|data: &[u8]| {
    let mut heap = Heap::stress();
    let realm = Realm::ordinary();
    let queue = realm.job_queue();
    let mut handles = Vec::new();
    let mut roots: Vec<Option<Root<Node>>> = Vec::new();

    for (step, byte) in data.iter().copied().take(4_096).enumerate() {
        let index = if handles.is_empty() {
            0
        } else {
            (byte as usize + step) % handles.len()
        };
        match byte % 10 {
            0 | 1 => {
                let root = heap.allocate(Node { edges: Vec::new() });
                handles.push(root.handle());
                roots.push(Some(root));
            }
            2 if !handles.is_empty() => roots[index] = None,
            3 if !handles.is_empty() => {
                if roots[index].is_none() {
                    roots[index] = heap.root(handles[index]);
                }
            }
            4 if handles.len() > 1 => {
                let owner = handles[index];
                let target = handles[(index + byte as usize + 1) % handles.len()];
                if heap.with(owner, |_| ()).is_some() && heap.with(target, |_| ()).is_some() {
                    heap.write_barrier(owner, target).unwrap();
                    heap.with_mut(owner, |node| node.edges.push(target))
                        .unwrap();
                }
            }
            5 => {
                heap.collect();
            }
            6 => {
                heap.collect_minor();
            }
            7 => {
                let source =
                    ["1 + 2;", "throw 1;", "let x = 1; x += 2;", "(() => 3)();"][byte as usize % 4];
                if let Ok(script) = ScriptRecord::parse(realm.clone(), source) {
                    let _ = script.evaluate();
                }
            }
            8 => queue.enqueue(move || {
                if byte & 0x80 == 0 {
                    Ok(())
                } else {
                    Err(RuntimeError::type_error("generated job failure"))
                }
            }),
            9 => {
                let _ = queue.drain();
            }
            _ => {}
        }
        heap.verify_write_barriers().unwrap();
        heap.verify().unwrap();
    }
    let _ = queue.drain();
    heap.collect();
    heap.verify().unwrap();
});
