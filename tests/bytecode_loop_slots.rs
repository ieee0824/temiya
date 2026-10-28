use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    runtime::{Completion, Realm, Value},
};
#[test]
fn loop_binding_writes_remain_visible_to_captured_functions() {
    let allocation_context = temiya::runtime::AllocationContext::new();
    for source in [
        "function f(n){var s=0;function read(){return s;}for(var i=0;i<n;i++){s+=i;if(read()!==s)throw 99;}return s;}f(10);",
        "function f(n){var s=0;function add(){s++;}while(n>0){n--;add();}return s;}f(45);",
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        assert!(matches!(
            VirtualMachine::execute_in_realm(&code, &Realm::ordinary_in(&allocation_context))
                .unwrap(),
            Completion::Normal(Some(Value::Number(45.0)))
        ));
    }
}
