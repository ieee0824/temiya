#![cfg(feature = "jit")]

use temiya::{
    bytecode::BytecodeCompiler,
    jit::{BaselineJit, JitErrorKind},
};

#[cfg(all(target_arch = "x86_64", unix))]
use temiya::{
    bytecode::VirtualMachine,
    runtime::{Completion, Value},
};

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn generated_machine_code_matches_interpreter_contract() {
    for source in ["40 + 2;", "(9 - 2) * 6;", "84 / 2;"] {
        let bytecode = BytecodeCompiler::compile(source).unwrap();
        let Completion::Normal(Some(Value::Number(expected))) =
            VirtualMachine::execute(&bytecode).unwrap()
        else {
            panic!("expected numeric VM completion")
        };
        let code = BaselineJit::compile(&bytecode).unwrap();
        assert!(code.code_size() > 1);
        assert_eq!(code.execute(), expected);
    }
}

#[cfg(all(target_arch = "x86_64", unix))]
#[test]
fn unsupported_verified_instructions_are_explicit_errors() {
    let bytecode = BytecodeCompiler::compile("1 === 1;").unwrap();
    let error = BaselineJit::compile(&bytecode).unwrap_err();
    assert_eq!(error.kind(), JitErrorKind::UnsupportedInstruction);
    assert!(error.offset().is_some());
}

#[cfg(not(all(target_arch = "x86_64", unix)))]
#[test]
fn unsupported_platform_is_explicit() {
    let bytecode = BytecodeCompiler::compile("42;").unwrap();
    assert_eq!(
        BaselineJit::compile(&bytecode).unwrap_err().kind(),
        JitErrorKind::UnsupportedPlatform
    );
}
