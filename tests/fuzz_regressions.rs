use temiya::{
    gc::{Handle, Heap, Root, Trace, Tracer},
    lexer::tokenize,
    parser::{parse_expression, parse_module, parse_script},
    runtime::{Realm, RuntimeError, ScriptRecord},
    source::SourceText,
};

const REGRESSIONS: &[&str] = &[include_str!(
    "../fuzz/regressions/lexer_parser/escaped-line.js"
)];

#[test]
fn lexer_parser_regression_corpus_never_panics_or_corrupts_ranges() {
    for source in REGRESSIONS {
        if let Ok(tokens) = tokenize(source) {
            for token in tokens {
                assert!(token.span.start <= token.span.end && token.span.end <= source.len());
                assert!(source.is_char_boundary(token.span.start));
                assert!(source.is_char_boundary(token.span.end));
            }
        }
        if let Ok(expression) = parse_expression(source) {
            expression.validate(&SourceText::new(source)).unwrap();
        }
        for program in [parse_script(source), parse_module(source)]
            .into_iter()
            .flatten()
        {
            program.validate(&SourceText::new(source)).unwrap();
        }
    }
}

struct StateNode {
    edges: Vec<Handle<StateNode>>,
}

impl Trace for StateNode {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        for edge in &self.edges {
            tracer.mark(*edge);
        }
    }
}

#[test]
fn gc_interpreter_regression_seed_preserves_invariants() {
    let data = include_bytes!("../fuzz/regressions/gc_interpreter/root-mutation-seed");
    let mut heap = Heap::stress();
    let realm = Realm::ordinary();
    let queue = realm.job_queue();
    let mut handles = Vec::new();
    let mut roots: Vec<Option<Root<StateNode>>> = Vec::new();
    for (step, byte) in data.iter().copied().enumerate() {
        let index = if handles.is_empty() {
            0
        } else {
            (byte as usize + step) % handles.len()
        };
        match byte % 10 {
            0 | 1 => {
                let root = heap.allocate(StateNode { edges: Vec::new() });
                handles.push(root.handle());
                roots.push(Some(root));
            }
            2 if !handles.is_empty() => roots[index] = None,
            3 if !handles.is_empty() => roots[index] = heap.root(handles[index]),
            4 if handles.len() > 1 => {
                let owner = handles[index];
                let target = handles[(index + 1) % handles.len()];
                if heap.with(owner, |_| ()).is_some() && heap.with(target, |_| ()).is_some() {
                    heap.write_barrier(owner, target).unwrap();
                    heap.with_mut(owner, |node| node.edges.push(target));
                }
            }
            5 => {
                heap.collect();
            }
            6 => {
                heap.collect_minor();
            }
            7 => {
                let _ = ScriptRecord::parse(realm.clone(), "throw 1;")
                    .and_then(|script| script.evaluate());
            }
            8 => queue.enqueue(|| Err(RuntimeError::type_error("fixture job failure"))),
            9 => {
                let _ = queue.drain();
            }
            _ => {}
        }
        heap.verify_write_barriers().unwrap();
        heap.verify().unwrap();
    }
}

#[test]
fn parser_errors_stay_on_utf8_boundaries() {
    for source in ["雪 +", "`𐐀${", "#私的", "\\u{10ffff}"] {
        for error in [parse_script(source).err(), parse_module(source).err()]
            .into_iter()
            .flatten()
        {
            assert!(error.span.start <= error.span.end && error.span.end <= source.len());
            assert!(source.is_char_boundary(error.span.start));
            assert!(source.is_char_boundary(error.span.end));
        }
    }
}
