//! Debug metadata, stack traces, and engine-to-debugger event hooks.

use crate::source::SourcePosition;
use std::collections::BTreeSet;

/// A location in an original source file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalLocation {
    /// Stable source identifier or URL.
    pub source: String,
    /// UTF-16 position in the original source.
    pub position: SourcePosition,
    /// Optional original symbol name.
    pub name: Option<String>,
}

/// One generated-to-original source mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapping {
    /// UTF-16 offset in generated code.
    pub generated_offset: usize,
    /// Corresponding original location.
    pub original: OriginalLocation,
}

/// An immutable, validated source map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMap {
    mappings: Vec<SourceMapping>,
}

/// Invalid source-map metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceMapError {
    /// Generated offsets must increase strictly.
    UnorderedOrDuplicateOffset,
}

impl std::fmt::Display for SourceMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("source-map offsets must be strictly increasing")
    }
}

impl std::error::Error for SourceMapError {}

impl SourceMap {
    /// Validates and stores mappings ordered by generated UTF-16 offset.
    pub fn new(mappings: Vec<SourceMapping>) -> Result<Self, SourceMapError> {
        if mappings
            .windows(2)
            .any(|pair| pair[0].generated_offset >= pair[1].generated_offset)
        {
            return Err(SourceMapError::UnorderedOrDuplicateOffset);
        }
        Ok(Self { mappings })
    }

    /// Resolves to the closest mapping at or before a generated offset.
    #[must_use]
    pub fn resolve(&self, generated_offset: usize) -> Option<&OriginalLocation> {
        let index = self
            .mappings
            .partition_point(|mapping| mapping.generated_offset <= generated_offset);
        index
            .checked_sub(1)
            .map(|index| &self.mappings[index].original)
    }
}

/// A JavaScript or native call frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StackFrame {
    /// A frame executing JavaScript source.
    Script {
        /// Display name, or an empty string for anonymous code.
        function: String,
        /// Stable source identifier or URL.
        source: String,
        /// Current UTF-16 source position.
        position: SourcePosition,
    },
    /// A host/native callback frame without a JavaScript position.
    Native {
        /// Embedder-provided display name.
        function: String,
    },
}

/// A synchronous frame or a boundary to an async parent stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StackTraceEntry {
    /// A synchronous frame, youngest first.
    Frame(StackFrame),
    /// The following frames belong to an async parent operation.
    AsyncBoundary {
        /// Operation that scheduled the async continuation.
        operation: String,
    },
}

/// An inspectable binding captured at a pause point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeBinding {
    /// Binding name.
    pub name: String,
    /// Debug display value. This is a snapshot and does not expose engine memory.
    pub value: String,
}

/// A lexical scope snapshot, innermost first in a pause event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeSnapshot {
    /// Human-readable scope kind such as `local`, `closure`, or `global`.
    pub kind: String,
    /// Bindings in deterministic declaration order.
    pub bindings: Vec<ScopeBinding>,
}

/// A UTF-16 breakpoint location.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Breakpoint {
    /// Stable source identifier or URL.
    pub source: String,
    /// Zero-based line.
    pub line: usize,
    /// Zero-based UTF-16 column.
    pub column: usize,
}

/// Requested stepping behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepMode {
    /// Pause at the next distinct source position, including callees.
    Into,
    /// Pause at the next distinct position at the current or shallower depth.
    Over,
    /// Pause after returning to a shallower stack depth.
    Out,
}

/// Why execution paused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseReason {
    /// A configured breakpoint matched.
    Breakpoint,
    /// A stepping request completed.
    Step,
    /// A JavaScript `debugger` statement executed.
    DebuggerStatement,
    /// The engine reported an exception and exception pausing is enabled.
    Exception,
}

/// Information supplied by the engine at an observable execution point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugPoint {
    /// Current source identifier or URL.
    pub source: String,
    /// Current UTF-16 position.
    pub position: SourcePosition,
    /// Number of active synchronous frames.
    pub stack_depth: usize,
    /// Whether this point represents a `debugger` statement.
    pub debugger_statement: bool,
    /// Whether an exception is being thrown at this point.
    pub exception: bool,
    /// Stack snapshot, including native and async entries.
    pub stack: Vec<StackTraceEntry>,
    /// Inspectable lexical scopes, innermost first.
    pub scopes: Vec<ScopeSnapshot>,
}

/// A pause notification delivered to a debugger frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugEvent {
    /// Reason for the pause.
    pub reason: PauseReason,
    /// Complete engine snapshot for the pause point.
    pub point: DebugPoint,
}

/// Consumer of synchronous debugger pause events.
pub trait DebugHook {
    /// Receives an immutable snapshot. The engine owns resumption policy.
    fn paused(&mut self, event: &DebugEvent);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StepRequest {
    mode: StepMode,
    start_depth: usize,
    start_offset: usize,
}

/// Mutable breakpoint and stepping state used by an engine instance.
#[derive(Default)]
pub struct DebugSession {
    breakpoints: BTreeSet<Breakpoint>,
    step: Option<StepRequest>,
    pause_on_exceptions: bool,
    hook: Option<Box<dyn DebugHook>>,
}

impl DebugSession {
    /// Creates a session without a notification hook.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs or replaces the synchronous notification hook.
    pub fn set_hook(&mut self, hook: impl DebugHook + 'static) {
        self.hook = Some(Box::new(hook));
    }

    /// Adds a breakpoint, returning whether it was newly inserted.
    pub fn add_breakpoint(&mut self, breakpoint: Breakpoint) -> bool {
        self.breakpoints.insert(breakpoint)
    }

    /// Removes a breakpoint, returning whether it existed.
    pub fn remove_breakpoint(&mut self, breakpoint: &Breakpoint) -> bool {
        self.breakpoints.remove(breakpoint)
    }

    /// Enables or disables pausing when the engine reports a throw.
    pub fn set_pause_on_exceptions(&mut self, enabled: bool) {
        self.pause_on_exceptions = enabled;
    }

    /// Arms a one-shot step request at the current depth and position.
    pub fn step(&mut self, mode: StepMode, stack_depth: usize, generated_offset: usize) {
        self.step = Some(StepRequest {
            mode,
            start_depth: stack_depth,
            start_offset: generated_offset,
        });
    }

    /// Cancels an outstanding step request.
    pub fn cancel_step(&mut self) {
        self.step = None;
    }

    /// Processes an engine point and notifies the hook when execution should pause.
    ///
    /// The returned event is also useful to embedders that poll instead of installing
    /// a hook. A completed step is consumed exactly once.
    pub fn observe(&mut self, point: DebugPoint) -> Option<DebugEvent> {
        let breakpoint = Breakpoint {
            source: point.source.clone(),
            line: point.position.line,
            column: point.position.column,
        };
        let step_completed = self.step.is_some_and(|step| match step.mode {
            StepMode::Into => point.position.offset != step.start_offset,
            StepMode::Over => {
                point.stack_depth <= step.start_depth && point.position.offset != step.start_offset
            }
            StepMode::Out => point.stack_depth < step.start_depth,
        });
        let reason = if point.debugger_statement {
            Some(PauseReason::DebuggerStatement)
        } else if self.pause_on_exceptions && point.exception {
            Some(PauseReason::Exception)
        } else if self.breakpoints.contains(&breakpoint) {
            Some(PauseReason::Breakpoint)
        } else if step_completed {
            Some(PauseReason::Step)
        } else {
            None
        }?;

        if step_completed {
            self.step = None;
        }
        let event = DebugEvent { reason, point };
        if let Some(hook) = &mut self.hook {
            hook.paused(&event);
        }
        Some(event)
    }
}
