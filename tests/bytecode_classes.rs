use temiya::{
    Engine,
    ast::ExprKind,
    bytecode::{BytecodeCompiler, Constant, Instruction, VirtualMachine},
    embedding::ScriptBackend,
    runtime::{
        Completion, Environment, IntrinsicId, Realm, RuntimeErrorKind, ThisBindingStatus, Value,
    },
};

fn check(source: &str) {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        let script = engine.compile(&realm, source).unwrap();
        assert!(
            matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ),
            "{backend:?}: {source}"
        );
    }
}

#[test]
fn function_class_expressions_emit_verified_class_instructions() {
    let code=BytecodeCompiler::compile("function make(){let C=class{constructor(value){this.value=value;}read(){return this.value;}};return new C(42);}make().read()===42;").unwrap();
    assert!(code.bytecode().constants.iter().any(|c| matches!(c,Constant::Scope(scope) if scope.functions.iter().any(|f| f.template.body.bytecode().instructions.iter().any(|op|matches!(op,Instruction::CreateClass(_)))))));
    let realm = Realm::ordinary();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &realm).unwrap(),
        Completion::Normal(Some(Value::Boolean(true)))
    ));
    assert!(BytecodeCompiler::compile("let C=class{};").is_err());
}

#[test]
fn class_names_identity_heritage_and_outer_bindings_survive_the_bridge() {
    for source in [
        "function make(value){let A=class{constructor(v){this.value=v;}};let B=class extends A{constructor(v){super(v);}read(){return value;}};return B;}let A=make(42),B=make(99);let p=new A(7);A!==B && A.prototype!==B.prototype && A.name==='B' && p.value===7 && p.read()===42 && new B().read()===99;",
        "function make(){let A=(class{});let B=class Inner{};let C;C=class{};return A.name==='A' && B.name==='Inner' && C.name==='C';}make();",
        "function make(){let classes=[];let i=0;while(i<3){let value=i;classes.push(class{read(){return value;}});i++;}return classes;}let values=make();new values[0]().read()===0 && new values[1]().read()===1 && new values[2]().read()===2;",
        "function make(){let caught=false;try{let C=class extends C{};}catch(e){caught=e instanceof ReferenceError;}return caught;}make();",
        "function make(){return class Named{};}make().name==='Named';",
        "function Make(){this.C=class{[new.target.name](){return 42;}};}let p=new Make();new p.C().Make()===42;",
    ] {
        check(source);
    }
}

#[test]
fn vm_class_captures_keep_distinct_for_iteration_bindings() {
    let code=BytecodeCompiler::compile("function make(){let classes=[];for(let i=0;i<3;i++){classes.push(class{read(){return i;}});}return classes;}let values=make();new values[0]().read()===0 && new values[1]().read()===1 && new values[2]().read()===2;").unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
        Completion::Normal(Some(Value::Boolean(true)))
    ));
}

#[test]
fn prepared_uninitialized_this_is_read_only_when_class_source_requires_it() {
    let realm = Realm::ordinary();
    let frame = Environment::function(
        Some(realm.global_environment()),
        realm.intrinsic(IntrinsicId::Object),
        None,
        ThisBindingStatus::Uninitialized,
    );
    let compile = |source| {
        let expression = temiya::parser::parse_expression(source).unwrap();
        let ExprKind::Function(function) = expression.kind else {
            panic!("function")
        };
        BytecodeCompiler::compile_function_body(&function).unwrap()
    };
    assert!(
        VirtualMachine::execute_in_function(
            &compile("function(){return class{};}"),
            &realm,
            &frame
        )
        .is_ok()
    );
    let error = VirtualMachine::execute_in_function(
        &compile("function(){return class{[this.key](){}};}"),
        &realm,
        &frame,
    )
    .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
}

#[test]
fn class_metadata_cannot_be_loaded_as_a_value_or_used_with_wrong_operands() {
    let expression = temiya::parser::parse_expression("function(){return class{};}").unwrap();
    let ExprKind::Function(function) = expression.kind else {
        panic!("function")
    };
    let body = BytecodeCompiler::compile_function_body(&function)
        .unwrap()
        .into_bytecode();
    let offset = body
        .instructions
        .iter()
        .position(|op| matches!(op, Instruction::CreateClass(_)))
        .unwrap();
    let Instruction::CreateClass(index) = body.instructions[offset] else {
        unreachable!()
    };
    let mut load = body.clone();
    load.instructions[offset] = Instruction::LoadConstant(index);
    assert!(load.verify().is_err());
    let mut wrong = body.clone();
    wrong.constants[index as usize] = Constant::Number(1.0);
    assert!(wrong.verify().is_err());
    let mut outside = body;
    outside.instructions[offset] = Instruction::CreateClass(u32::MAX);
    assert!(outside.verify().is_err());
}

#[test]
fn class_bridge_roots_live_bindings_during_reentrant_collection() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = temiya::runtime::Object::native_function_with_context(
                engine.allocation_context(),
                |context, _, _| {
                    context.collect_garbage()?;
                    Ok(Value::Undefined)
                },
            );
            engine
                .set_global(
                    &realm,
                    "gcNow",
                    engine.value(&realm, Value::Object(gc)).unwrap(),
                )
                .unwrap();
            let script=engine.compile(&realm,"function A(v){gcNow();this.value=v;}function make(value){let D=class extends (gcNow(),A){constructor(v){super(v);}[ (gcNow(),'read') ](){gcNow();return value;}};return new D(value);}let marker={};let instance=make(marker);gcNow();instance.value===marker && instance.read()===marker;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}
