//! Runnable examples of the public execution paths and their current limits.

use temiya::{
    Engine,
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_script,
    runtime::{Completion, Realm, RuntimeErrorKind, Value},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let realm = engine.create_realm();
    for source in [
        "function twice(x) { return x * 2; } twice(21);",
        "({ answer: 42 }).answer;",
    ] {
        let script = engine.compile(&realm, source)?;
        assert!(matches!(
            engine.evaluate(&script)?.value(),
            Value::Number(42.0)
        ));
        assert!(BytecodeCompiler::compile(source).is_err());
    }

    // Syntax acceptance and runtime support are separate contracts.
    let class = "(class Example {});";
    assert!(parse_script(class).is_ok());
    let script = engine.compile(&realm, class)?;
    assert!(engine.evaluate(&script).is_err());

    let bytecode = BytecodeCompiler::compile("40 + 2;")?;
    assert!(matches!(
        VirtualMachine::execute(&bytecode)?,
        Completion::Normal(Some(Value::Number(42.0)))
    ));

    let bytecode_realm = Realm::ordinary();
    for (source, expected) in [
        ("var total=1; {let total=2; total;} total;", 1.0),
        ("var total; total+41;", 42.0),
    ] {
        let code = BytecodeCompiler::compile(source)?;
        let result = VirtualMachine::execute_in_realm(&code, &bytecode_realm)?;
        let Completion::Normal(Some(Value::Number(value))) = result else {
            panic!("expected numeric completion")
        };
        assert_eq!(value, expected);
    }
    let tdz = BytecodeCompiler::compile("{before; let before=1;}")?;
    assert_eq!(
        VirtualMachine::execute_in_realm(&tdz, &bytecode_realm)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );

    let property = BytecodeCompiler::compile("this.answer=42; this?.answer;")?;
    assert!(matches!(
        VirtualMachine::execute(&property)?,
        Completion::Normal(Some(Value::Number(42.0)))
    ));

    #[cfg(feature = "jit")]
    check_jit()?;

    println!("AST and bytecode examples and rejection cases passed");
    let calls = BytecodeCompiler::compile("Math.abs(0 - 42);")?;
    assert!(matches!(
        VirtualMachine::execute_in_realm(&calls, &bytecode_realm)?,
        Completion::Normal(Some(Value::Number(42.0)))
    ));
    Ok(())
}

#[cfg(feature = "jit")]
fn check_jit() -> Result<(), Box<dyn std::error::Error>> {
    use temiya::jit::{BaselineJit, JitErrorKind};

    let numeric = BytecodeCompiler::compile("40 + 2;")?;
    let equality = BytecodeCompiler::compile("1 === 1;")?;
    #[cfg(all(target_arch = "x86_64", unix))]
    {
        assert_eq!(BaselineJit::compile(&numeric)?.execute(), 42.0);
        assert_eq!(
            BaselineJit::compile(&equality).unwrap_err().kind(),
            JitErrorKind::UnsupportedInstruction
        );
        println!("x86-64 Unix JIT execution and rejection passed");
    }
    #[cfg(not(all(target_arch = "x86_64", unix)))]
    {
        for bytecode in [&numeric, &equality] {
            assert_eq!(
                BaselineJit::compile(bytecode).unwrap_err().kind(),
                JitErrorKind::UnsupportedPlatform
            );
        }
        println!("JIT correctly reports UnsupportedPlatform on this target");
    }
    Ok(())
}
