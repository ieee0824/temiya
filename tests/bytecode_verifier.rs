use temiya::bytecode::{Bytecode, Constant, Instruction, VerifyErrorKind};

fn verify(instructions: Vec<Instruction>) -> Result<usize, VerifyErrorKind> {
    Bytecode {
        constants: vec![Constant::Number(1.0), Constant::Number(2.0)],
        instructions,
        local_count: 1,
    }
    .verify()
    .map(|verified| verified.max_stack())
    .map_err(|error| error.kind())
}

#[test]
fn computes_stack_bound_across_branches_and_locals() {
    let bytecode = Bytecode {
        constants: vec![
            Constant::Boolean(true),
            Constant::Number(40.0),
            Constant::Number(2.0),
        ],
        instructions: vec![
            Instruction::LoadConstant(0),
            Instruction::JumpIfFalse(5),
            Instruction::LoadConstant(1),
            Instruction::StoreLocal(0),
            Instruction::Jump(7),
            Instruction::LoadConstant(2),
            Instruction::StoreLocal(0),
            Instruction::LoadLocal(0),
            Instruction::LoadConstant(2),
            Instruction::Add,
            Instruction::Return,
        ],
        local_count: 1,
    };
    let verified = bytecode.clone().verify().unwrap();
    assert_eq!(verified.max_stack(), 2);
    assert_eq!(verified.bytecode(), &bytecode);
}

#[test]
fn rejects_bad_constant_local_and_jump_operands() {
    assert_eq!(
        verify(vec![Instruction::LoadConstant(2), Instruction::Return]),
        Err(VerifyErrorKind::ConstantOutOfBounds)
    );
    assert_eq!(
        verify(vec![Instruction::LoadLocal(1), Instruction::Return]),
        Err(VerifyErrorKind::LocalOutOfBounds)
    );
    assert_eq!(
        verify(vec![Instruction::Jump(2), Instruction::Return]),
        Err(VerifyErrorKind::JumpOutOfBounds)
    );
}

#[test]
fn rejects_underflow_and_invalid_return_depths() {
    assert_eq!(
        verify(vec![Instruction::Pop, Instruction::Return]),
        Err(VerifyErrorKind::StackUnderflow)
    );
    assert_eq!(
        verify(vec![
            Instruction::LoadConstant(0),
            Instruction::LoadConstant(1),
            Instruction::Return,
        ]),
        Err(VerifyErrorKind::InvalidReturnDepth)
    );
}

#[test]
fn rejects_inconsistent_joins_fallthrough_and_dead_code() {
    assert_eq!(
        verify(vec![
            Instruction::LoadConstant(0),
            Instruction::JumpIfFalse(3),
            Instruction::LoadConstant(1),
            Instruction::LoadConstant(0),
            Instruction::Return,
        ]),
        Err(VerifyErrorKind::StackDepthMismatch)
    );
    assert_eq!(
        verify(vec![Instruction::LoadConstant(0)]),
        Err(VerifyErrorKind::MissingReturn)
    );
    assert_eq!(
        verify(vec![
            Instruction::LoadConstant(0),
            Instruction::Return,
            Instruction::LoadConstant(1),
            Instruction::Return,
        ]),
        Err(VerifyErrorKind::UnreachableInstruction)
    );
}

#[test]
fn accepts_a_stack_neutral_loop_with_an_exit() {
    assert_eq!(
        verify(vec![
            Instruction::LoadConstant(0),
            Instruction::JumpIfFalse(3),
            Instruction::Jump(0),
            Instruction::LoadConstant(1),
            Instruction::Return,
        ]),
        Ok(1)
    );
}
