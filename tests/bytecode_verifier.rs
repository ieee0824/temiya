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

#[test]
fn scope_operands_and_control_flow_are_verified() {
    use temiya::bytecode::{BindingKind, ScopeBinding, ScopeDescriptor, ScopeKind};
    let plan = |kind, binding| {
        Constant::Scope(ScopeDescriptor {
            kind,
            strict: false,
            bindings: vec![ScopeBinding {
                name: "x".into(),
                kind: binding,
            }],
        })
    };
    let check = |constants, instructions| {
        Bytecode {
            constants,
            instructions,
            local_count: 0,
        }
        .verify()
        .unwrap_err()
        .kind()
    };
    let global = plan(ScopeKind::Global, BindingKind::Let);
    assert_eq!(
        check(
            vec![global.clone()],
            vec![Instruction::LoadConstant(0), Instruction::Return]
        ),
        VerifyErrorKind::InvalidConstantKind
    );
    assert_eq!(
        check(vec![], vec![Instruction::ExitScope]),
        VerifyErrorKind::ScopeMismatch
    );
    assert_eq!(
        check(
            vec![global.clone()],
            vec![
                Instruction::EnterScope(0),
                Instruction::LoadBinding { scope: 0, slot: 1 },
                Instruction::Return
            ]
        ),
        VerifyErrorKind::BindingOutOfBounds
    );
    assert_eq!(
        check(
            vec![global.clone()],
            vec![
                Instruction::EnterScope(0),
                Instruction::ExitScope,
                Instruction::LoadBinding { scope: 0, slot: 0 },
                Instruction::Return
            ]
        ),
        VerifyErrorKind::BindingOutOfBounds
    );
    assert_eq!(
        check(
            vec![plan(ScopeKind::Block, BindingKind::Var)],
            vec![Instruction::EnterScope(0)]
        ),
        VerifyErrorKind::InvalidScopeDescriptor
    );
    assert_eq!(
        check(
            vec![
                plan(ScopeKind::Global, BindingKind::Var),
                Constant::Undefined
            ],
            vec![
                Instruction::EnterScope(0),
                Instruction::LoadConstant(1),
                Instruction::InitializeBinding { scope: 0, slot: 0 },
                Instruction::Return
            ]
        ),
        VerifyErrorKind::BindingOutOfBounds
    );
    // One edge exits the scope and the other retains it at the join.
    assert_eq!(
        check(
            vec![global, Constant::Boolean(true)],
            vec![
                Instruction::EnterScope(0),
                Instruction::LoadConstant(1),
                Instruction::JumpIfFalse(5),
                Instruction::ExitScope,
                Instruction::Jump(6),
                Instruction::Jump(6),
                Instruction::LoadConstant(1),
                Instruction::Return
            ]
        ),
        VerifyErrorKind::ScopeMismatch
    );
    assert_eq!(
        check(
            vec![Constant::String("x".into())],
            vec![Instruction::LoadName(0), Instruction::Return]
        ),
        VerifyErrorKind::ScopeMismatch
    );
}

#[test]
fn retained_name_references_are_checked() {
    use temiya::bytecode::{ScopeDescriptor, ScopeKind};
    let scope = Constant::Scope(ScopeDescriptor {
        kind: ScopeKind::Global,
        strict: false,
        bindings: vec![],
    });
    let check = |code| {
        Bytecode {
            constants: vec![
                scope.clone(),
                Constant::String("x".into()),
                Constant::Number(1.0),
            ],
            instructions: code,
            local_count: 0,
        }
        .verify()
        .unwrap_err()
        .kind()
    };
    assert_eq!(
        check(vec![
            Instruction::EnterScope(0),
            Instruction::LoadConstant(2),
            Instruction::StoreReference,
            Instruction::Return
        ]),
        VerifyErrorKind::ScopeMismatch
    );
    assert_eq!(
        check(vec![
            Instruction::EnterScope(0),
            Instruction::ResolveName {
                name: 2,
                strict: false
            },
            Instruction::Return
        ]),
        VerifyErrorKind::InvalidConstantKind
    );
    assert_eq!(
        check(vec![
            Instruction::EnterScope(0),
            Instruction::ResolveName {
                name: 1,
                strict: false
            },
            Instruction::ExitScope,
            Instruction::LoadConstant(2),
            Instruction::Complete
        ]),
        VerifyErrorKind::ScopeMismatch
    );
}
