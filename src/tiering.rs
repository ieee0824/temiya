//! Validated metadata and state transitions for future optimizing tiers.

use std::{collections::HashMap, fmt};

/// Execution tier participating in the transition state machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionTier {
    /// AST or bytecode interpreter with fully materialized state.
    Interpreter,
    /// Feature-gated non-optimizing machine code.
    Baseline,
    /// Future speculative optimizing machine code.
    Optimizing,
}

/// Cause of a tier transition or reconstruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionReason {
    /// A function entry counter crossed its threshold.
    HotFunction,
    /// A loop backedge counter crossed its threshold, permitting OSR.
    HotLoop,
    /// A speculative guard failed.
    GuardFailure,
    /// Optimized execution produced an ECMAScript exception.
    Exception,
    /// Debugger observation requires canonical interpreter frames.
    Debugger,
    /// GC requested a stop at a declared safepoint.
    GarbageCollection,
}

/// Stable identifier shared by stack maps and deoptimization metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SafepointId(pub u32);

/// Reason generated code may stop with precise roots available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafepointKind {
    /// Call into runtime or host code.
    Call,
    /// Backedge used for GC polling and OSR.
    LoopBackedge,
    /// Allocation slow path.
    Allocation,
    /// Explicit debugger or interrupt poll.
    Poll,
}

/// Logical machine slot classification at a safepoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StackSlotKind {
    /// A tagged JavaScript value that must be traced.
    Value,
    /// An unboxed scalar that is not a GC root.
    Scalar,
    /// A slot with no live value.
    Dead,
}

/// Precise root layout for one generated-code safepoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackMap {
    /// Shared safepoint identity.
    pub id: SafepointId,
    /// Baseline bytecode offset represented by this machine position.
    pub bytecode_offset: u32,
    /// Why execution may stop here.
    pub kind: SafepointKind,
    /// Frame slots in deterministic frame-layout order.
    pub slots: Vec<StackSlotKind>,
}

/// One interpreter frame reconstructed during deoptimization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptFrame {
    /// Resume bytecode offset, before the next observable operation.
    pub bytecode_offset: u32,
    /// Number of materialized local values.
    pub local_count: u32,
    /// Number of materialized operand-stack values.
    pub stack_depth: u32,
}

/// Reconstruction recipe attached to a generated-code safepoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeoptMetadata {
    /// Safepoint whose stack map describes all referenced values.
    pub safepoint: SafepointId,
    /// Inline frames from outermost caller to innermost resume point.
    pub frames: Vec<DeoptFrame>,
}

/// On-stack-replacement entry from an interpreter loop backedge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OsrEntry {
    /// Interpreter bytecode offset at the loop backedge.
    pub bytecode_offset: u32,
    /// Loop-backedge safepoint used to import roots and values.
    pub safepoint: SafepointId,
    /// Operand depth required before entering generated code.
    pub stack_depth: u32,
}

/// Immutable metadata bundle installed atomically with generated code.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TieringMetadata {
    /// Precise GC stack maps.
    pub stack_maps: Vec<StackMap>,
    /// Guard/exception/debugger reconstruction recipes.
    pub deopts: Vec<DeoptMetadata>,
    /// Legal OSR entry points.
    pub osr_entries: Vec<OsrEntry>,
}

impl TieringMetadata {
    /// Validates cross-table identities and OSR/deoptimization invariants.
    pub fn validate(&self) -> Result<(), MetadataError> {
        let mut maps = HashMap::new();
        for map in &self.stack_maps {
            if maps.insert(map.id, map).is_some() {
                return Err(MetadataError::DuplicateSafepoint(map.id));
            }
        }
        for deopt in &self.deopts {
            if !maps.contains_key(&deopt.safepoint) {
                return Err(MetadataError::MissingStackMap(deopt.safepoint));
            }
            if deopt.frames.is_empty() {
                return Err(MetadataError::EmptyDeoptFrames(deopt.safepoint));
            }
        }
        for entry in &self.osr_entries {
            let map = maps
                .get(&entry.safepoint)
                .ok_or(MetadataError::MissingStackMap(entry.safepoint))?;
            if map.kind != SafepointKind::LoopBackedge
                || map.bytecode_offset != entry.bytecode_offset
            {
                return Err(MetadataError::InvalidOsrEntry(entry.safepoint));
            }
        }
        Ok(())
    }
}

/// Metadata validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetadataError {
    /// Two stack maps use one identity.
    DuplicateSafepoint(SafepointId),
    /// Metadata references a missing stack map.
    MissingStackMap(SafepointId),
    /// A deoptimization recipe cannot reconstruct any frame.
    EmptyDeoptFrames(SafepointId),
    /// OSR does not point at the matching loop-backedge map.
    InvalidOsrEntry(SafepointId),
}

impl fmt::Display for MetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid tiering metadata: {self:?}")
    }
}

impl std::error::Error for MetadataError {}

/// Observable tier-transition state for one function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TieringState {
    current: ExecutionTier,
}

impl TieringState {
    /// Creates an interpreter-owned state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: ExecutionTier::Interpreter,
        }
    }

    /// Returns the active execution tier.
    #[must_use]
    pub fn current(&self) -> ExecutionTier {
        self.current
    }

    /// Applies one legal transition and rejects unsafe tier jumps.
    pub fn transition(
        &mut self,
        target: ExecutionTier,
        reason: TransitionReason,
    ) -> Result<(), TransitionError> {
        let legal = match (self.current, target, reason) {
            (
                ExecutionTier::Interpreter,
                ExecutionTier::Baseline,
                TransitionReason::HotFunction,
            )
            | (ExecutionTier::Interpreter, ExecutionTier::Baseline, TransitionReason::HotLoop)
            | (ExecutionTier::Baseline, ExecutionTier::Optimizing, TransitionReason::HotFunction)
            | (ExecutionTier::Baseline, ExecutionTier::Optimizing, TransitionReason::HotLoop) => {
                true
            }
            (_, ExecutionTier::Interpreter, TransitionReason::GuardFailure)
            | (_, ExecutionTier::Interpreter, TransitionReason::Exception)
            | (_, ExecutionTier::Interpreter, TransitionReason::Debugger) => true,
            (tier, target, TransitionReason::GarbageCollection) => tier == target,
            _ => false,
        };
        if !legal {
            return Err(TransitionError {
                from: self.current,
                to: target,
                reason,
            });
        }
        self.current = target;
        Ok(())
    }
}

impl Default for TieringState {
    fn default() -> Self {
        Self::new()
    }
}

/// Rejected tier transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionError {
    /// Active tier before the attempt.
    pub from: ExecutionTier,
    /// Requested tier.
    pub to: ExecutionTier,
    /// Requested transition reason.
    pub reason: TransitionReason,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "illegal tier transition {:?} -> {:?} ({:?})",
            self.from, self.to, self.reason
        )
    }
}

impl std::error::Error for TransitionError {}
