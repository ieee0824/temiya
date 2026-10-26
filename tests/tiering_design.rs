use temiya::tiering::{
    DeoptFrame, DeoptMetadata, ExecutionTier, MetadataError, OsrEntry, SafepointId, SafepointKind,
    StackMap, StackSlotKind, TieringMetadata, TieringState, TransitionReason,
};

fn metadata() -> TieringMetadata {
    TieringMetadata {
        stack_maps: vec![StackMap {
            id: SafepointId(7),
            bytecode_offset: 12,
            kind: SafepointKind::LoopBackedge,
            slots: vec![StackSlotKind::Value, StackSlotKind::Scalar],
        }],
        deopts: vec![DeoptMetadata {
            safepoint: SafepointId(7),
            frames: vec![DeoptFrame {
                bytecode_offset: 12,
                local_count: 1,
                stack_depth: 0,
            }],
        }],
        osr_entries: vec![OsrEntry {
            bytecode_offset: 12,
            safepoint: SafepointId(7),
            stack_depth: 0,
        }],
    }
}

#[test]
fn coherent_safepoint_deopt_and_osr_metadata_validates() {
    metadata().validate().unwrap();
}

#[test]
fn cross_table_and_boundary_errors_are_rejected() {
    let mut missing = metadata();
    missing.deopts[0].safepoint = SafepointId(8);
    assert_eq!(
        missing.validate(),
        Err(MetadataError::MissingStackMap(SafepointId(8)))
    );

    let mut empty = metadata();
    empty.deopts[0].frames.clear();
    assert_eq!(
        empty.validate(),
        Err(MetadataError::EmptyDeoptFrames(SafepointId(7)))
    );

    let mut wrong_osr = metadata();
    wrong_osr.osr_entries[0].bytecode_offset = 13;
    assert_eq!(
        wrong_osr.validate(),
        Err(MetadataError::InvalidOsrEntry(SafepointId(7)))
    );
}

#[test]
fn hotness_promotes_and_observable_events_deopt() {
    let mut state = TieringState::new();
    state
        .transition(ExecutionTier::Baseline, TransitionReason::HotFunction)
        .unwrap();
    state
        .transition(ExecutionTier::Optimizing, TransitionReason::HotLoop)
        .unwrap();
    state
        .transition(ExecutionTier::Interpreter, TransitionReason::Exception)
        .unwrap();
    assert_eq!(state.current(), ExecutionTier::Interpreter);

    assert!(
        state
            .transition(ExecutionTier::Optimizing, TransitionReason::HotFunction)
            .is_err()
    );
}
