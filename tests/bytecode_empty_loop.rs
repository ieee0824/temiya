use temiya::{
    bytecode::{
        BindingKind, Bytecode, Constant, Instruction, ScopeBinding, ScopeDescriptor, ScopeKind,
        VirtualMachine,
    },
    runtime::{
        AllocationContext, Completion, Environment, Object, Realm, ThisBindingStatus, Value,
    },
};

#[test]
fn empty_loop_clone_keeps_prepared_function_bindings() {
    let context = AllocationContext::new();
    let realm = Realm::ordinary_in(&context);
    let code = Bytecode {
        constants: vec![
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Function,
                strict: true,
                bindings: vec![ScopeBinding {
                    name: "answer".into(),
                    kind: BindingKind::Var,
                }],
                functions: vec![],
            }),
            Constant::Number(42.0),
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Block,
                strict: true,
                bindings: vec![],
                functions: vec![],
            }),
            Constant::String("answer".into()),
        ],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::LoadConstant(1),
            Instruction::StoreBinding { scope: 0, slot: 0 },
            Instruction::EnterScope(2),
            Instruction::CloneLoopBindings,
            Instruction::LoadName(3),
            Instruction::ExitScope,
            Instruction::Return,
        ],
        local_count: 0,
    }
    .verify()
    .unwrap();
    for outer in [None, Some(realm.global_environment())] {
        let frame = Environment::function(
            outer,
            Object::new_in(&context),
            None,
            ThisBindingStatus::Uninitialized,
        );
        assert!(matches!(
            VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap(),
            Completion::Return(Value::Number(42.0))
        ));
    }
}
