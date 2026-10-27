use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, RuntimeResult, StatementEvaluator, Value},
};
fn execute(realm: &Realm, source: &str, vm: bool) -> RuntimeResult<Completion> {
    if vm {
        VirtualMachine::execute_in_realm(&BytecodeCompiler::compile(source).unwrap(), realm)
    } else {
        StatementEvaluator::new(realm.clone()).evaluate_program(&parse_script(source).unwrap())
    }
}
fn check(realm: &Realm, source: &str, vm: bool) {
    assert!(
        matches!(
            execute(realm, source, vm).unwrap(),
            Completion::Normal(Some(Value::Boolean(true)))
        ),
        "{source}"
    );
}
#[test]
fn holes_empty_spreads_and_trailing_commas_preserve_length_and_own_properties() {
    for vm in [false, true] {
        for source in [
            "var a=[,,42,,];a.length===4&&!(0 in a)&&!(1 in a)&&a[2]===42&&!(3 in a);",
            "var a=[...[]];var b=[...[],];var c=[,...[],];a.length===0&&b.length===0&&c.length===1&&!(0 in c);",
            "var a=[...[,,]];a.length===2&&Object.prototype.hasOwnProperty.call(a,'0')&&a[0]===undefined&&Object.prototype.hasOwnProperty.call(a,'1');",
            "var a=[...'\u{1f600}'];a.length===1&&a[0].length===2;",
            "var a=[1,[2,,3],4];a.length===3&&a[1].length===3&&!(1 in a[1]);",
        ] {
            check(&Realm::ordinary(), source, vm);
        }
    }
}
#[test]
fn custom_iterator_captures_next_once_and_preserves_evaluation_order() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(&realm,"var log='';var n=0;function mark(x){log+=x;return x;}var iterator={next:function(){log+='N';iterator.next=function(){throw 99;};n++;return {done:n>2,value:n};},return:function(){log+='R';return {};}};var iterable={[Symbol.iterator]:function(){log+='I';return iterator;}};",false).unwrap();
        check(
            &realm,
            "var a=[mark(0),...iterable,mark(9)];a.length===4&&a[0]===0&&a[1]===1&&a[2]===2&&a[3]===9&&log==='0INNN9';",
            vm,
        );
    }
}
#[test]
fn iterator_failures_propagate_and_do_not_close_or_evaluate_later_elements() {
    for failing in ["next-get", "next-call", "done", "value"] {
        for vm in [false, true] {
            let realm = Realm::ordinary();
            let setup = format!(
                "var log='';var marker={{}};var failing='{failing}';var iterator={{get next(){{log+='G';if(failing==='next-get')throw marker;return function(){{log+='N';if(failing==='next-call')throw marker;return {{get done(){{log+='D';if(failing==='done')throw marker;return false;}},get value(){{log+='V';throw marker;}}}};}};}},return:function(){{log+='R';return {{}};}}}};var iterable={{[Symbol.iterator]:function(){{log+='I';return iterator;}}}};function later(){{log+='L';return 0;}}"
            );
            execute(&realm, &setup, false).unwrap();
            let err = execute(&realm, "[...iterable,later()];", vm).unwrap_err();
            assert!(
                err.thrown_value()
                    .unwrap()
                    .same_value(&realm.global_object().get(&"marker".into()).unwrap())
            );
            let expected = match failing {
                "next-get" => "IG",
                "next-call" => "IGN",
                "done" => "IGND",
                _ => "IGNDV",
            };
            check(&realm, &format!("log==='{expected}';"), false);
        }
    }
}
#[test]
fn array_like_objects_require_an_iterator() {
    for vm in [false, true] {
        let realm = Realm::ordinary();
        execute(&realm, "var value={0:42,length:1};", false).unwrap();
        assert_eq!(
            execute(&realm, "[...value];", vm).unwrap_err().kind(),
            RuntimeErrorKind::TypeError
        );
    }
}

#[test]
fn array_instructions_check_scope_stack_state_and_length_overflow() {
    use temiya::bytecode::{
        Bytecode, Constant, Instruction, ScopeDescriptor, ScopeKind, VerifyErrorKind,
    };
    let scope = Constant::Scope(ScopeDescriptor {
        kind: ScopeKind::Global,
        strict: false,
        bindings: vec![],
        functions: vec![],
    });
    let code = Bytecode {
        constants: vec![],
        instructions: vec![Instruction::CreateArray, Instruction::Return],
        local_count: 0,
    };
    assert_eq!(
        code.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
    for op in [
        Instruction::AppendArray,
        Instruction::SpreadArray,
        Instruction::ElideArray,
        Instruction::FinishArray,
    ] {
        let code = Bytecode {
            constants: vec![scope.clone()],
            instructions: vec![Instruction::EnterScope(0), op, Instruction::Return],
            local_count: 0,
        };
        assert_eq!(
            code.verify().unwrap_err().kind(),
            VerifyErrorKind::StackUnderflow
        );
    }
    let code = Bytecode {
        constants: vec![scope, Constant::Number(f64::from(u32::MAX))],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::CreateArray,
            Instruction::Pop,
            Instruction::LoadConstant(1),
            Instruction::ElideArray,
            Instruction::FinishArray,
            Instruction::Return,
        ],
        local_count: 0,
    }
    .verify()
    .unwrap();
    assert_eq!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::RangeError
    );
}
