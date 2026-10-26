use std::cell::RefCell;
use std::rc::Rc;
use temiya::debug::{
    Breakpoint, DebugEvent, DebugHook, DebugPoint, DebugSession, OriginalLocation, PauseReason,
    ScopeBinding, ScopeSnapshot, SourceMap, SourceMapError, SourceMapping, StackFrame,
    StackTraceEntry, StepMode,
};
use temiya::source::{SourcePosition, SourceText};

fn position(offset: usize, line: usize, column: usize) -> SourcePosition {
    SourcePosition {
        offset,
        line,
        column,
    }
}

fn point(position: SourcePosition, depth: usize) -> DebugPoint {
    DebugPoint {
        source: "app.js".into(),
        position,
        stack_depth: depth,
        debugger_statement: false,
        exception: false,
        stack: vec![],
        scopes: vec![],
    }
}

#[test]
fn source_maps_resolve_utf16_offsets_and_reject_bad_order() {
    let generated = SourceText::new("a😀b");
    let after_emoji = generated.position("a😀".len()).unwrap();
    assert_eq!(after_emoji.offset, 3);

    let map = SourceMap::new(vec![
        SourceMapping {
            generated_offset: 0,
            original: OriginalLocation {
                source: "input.ts".into(),
                position: position(0, 0, 0),
                name: Some("a".into()),
            },
        },
        SourceMapping {
            generated_offset: after_emoji.offset,
            original: OriginalLocation {
                source: "input.ts".into(),
                position: position(8, 2, 1),
                name: Some("b".into()),
            },
        },
    ])
    .unwrap();

    assert_eq!(map.resolve(2).unwrap().name.as_deref(), Some("a"));
    assert_eq!(map.resolve(3).unwrap().position, position(8, 2, 1));
    assert_eq!(map.resolve(usize::MAX).unwrap().name.as_deref(), Some("b"));

    let duplicate = SourceMapping {
        generated_offset: 1,
        original: map.resolve(0).unwrap().clone(),
    };
    assert_eq!(
        SourceMap::new(vec![duplicate.clone(), duplicate]),
        Err(SourceMapError::UnorderedOrDuplicateOffset)
    );
}

#[derive(Clone)]
struct RecordingHook(Rc<RefCell<Vec<DebugEvent>>>);

impl DebugHook for RecordingHook {
    fn paused(&mut self, event: &DebugEvent) {
        self.0.borrow_mut().push(event.clone());
    }
}

#[test]
fn breakpoints_steps_and_exception_policy_are_deterministic() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut session = DebugSession::new();
    session.set_hook(RecordingHook(events.clone()));
    let breakpoint = Breakpoint {
        source: "app.js".into(),
        line: 1,
        column: 2,
    };
    assert!(session.add_breakpoint(breakpoint.clone()));
    assert!(!session.add_breakpoint(breakpoint.clone()));

    let hit = session.observe(point(position(7, 1, 2), 2)).unwrap();
    assert_eq!(hit.reason, PauseReason::Breakpoint);

    session.step(StepMode::Over, 2, 7);
    assert_eq!(session.observe(point(position(8, 2, 0), 3)), None);
    assert_eq!(
        session.observe(point(position(9, 2, 1), 2)).unwrap().reason,
        PauseReason::Step
    );
    assert_eq!(session.observe(point(position(10, 2, 2), 2)), None);

    session.set_pause_on_exceptions(true);
    let mut thrown = point(position(11, 3, 0), 1);
    thrown.exception = true;
    assert_eq!(
        session.observe(thrown).unwrap().reason,
        PauseReason::Exception
    );
    assert_eq!(events.borrow().len(), 3);
    assert!(session.remove_breakpoint(&breakpoint));
}

#[test]
fn stack_snapshots_represent_native_async_and_scopes() {
    let mut session = DebugSession::new();
    session.step(StepMode::Out, 2, 4);
    let mut pause = point(position(5, 0, 5), 1);
    pause.stack = vec![
        StackTraceEntry::Frame(StackFrame::Script {
            function: "resume".into(),
            source: "app.js".into(),
            position: position(5, 0, 5),
        }),
        StackTraceEntry::Frame(StackFrame::Native {
            function: "host_timer".into(),
        }),
        StackTraceEntry::AsyncBoundary {
            operation: "await".into(),
        },
    ];
    pause.scopes = vec![ScopeSnapshot {
        kind: "local".into(),
        bindings: vec![ScopeBinding {
            name: "answer".into(),
            value: "42".into(),
        }],
    }];

    let event = session.observe(pause).unwrap();
    assert_eq!(event.reason, PauseReason::Step);
    assert!(matches!(
        event.point.stack[1],
        StackTraceEntry::Frame(StackFrame::Native { .. })
    ));
    assert!(matches!(
        event.point.stack[2],
        StackTraceEntry::AsyncBoundary { .. }
    ));
    assert_eq!(event.point.scopes[0].bindings[0].value, "42");
}
