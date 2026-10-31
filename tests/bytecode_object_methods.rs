use temiya::{
    Engine,
    ast::ExprKind,
    bytecode::{Bytecode, BytecodeCompiler, Constant, Instruction, VirtualMachine},
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
        let result = engine
            .evaluate(&script)
            .unwrap_or_else(|error| panic!("{backend:?}: {error}: {source}"));
        assert!(
            matches!(result.value(), Value::Boolean(true)),
            "{backend:?}: {source}: {:?}",
            result.value()
        );
    }
}

fn function_body(source: &str) -> temiya::bytecode::VerifiedBytecode {
    let expression = temiya::parser::parse_expression(source).unwrap();
    let ExprKind::Function(function) = expression.kind else {
        panic!("function")
    };
    BytecodeCompiler::compile_function_body(&function).unwrap()
}

#[test]
fn literal_accessor_redefinitions_merge_halves_and_preserve_key_order() {
    check(
        "let saved=0;let object={first:1,get x(){return 1;},set x(value){saved=value;},get x(){return 42;},last:2};object.x=20;let d=Object.getOwnPropertyDescriptor(object,'x');let keys=Reflect.ownKeys(object);object.x===42&&saved===20&&typeof d.get==='function'&&typeof d.set==='function'&&d.enumerable&&d.configurable&&keys[0]==='first'&&keys[1]==='x'&&keys[2]==='last';",
    );
    check(
        "let saved=0;let key=Symbol.iterator;let object={set [key](value){saved=value;},get [key](){return 22;},data:1,get data(){return 42;},get replaced(){throw 99;},replaced:20};object[key]=42;let d=Object.getOwnPropertyDescriptor(object,'data');let replaced=Object.getOwnPropertyDescriptor(object,'replaced');object[key]===22&&saved===42&&object.data===42&&d.set===undefined&&!('value' in d)&&replaced.value===20&&replaced.writable&&!('get' in replaced)&&Reflect.ownKeys(object)[2]===key;",
    );
}

#[test]
fn function_object_initializers_emit_verified_metadata() {
    let code = BytecodeCompiler::compile("function make(value){return {read(){return value;},get value(){return value;}};}let object=make(42);object.read()===42 && object.value===42;").unwrap();
    assert!(code.bytecode().constants.iter().any(|constant| matches!(constant, Constant::Scope(scope) if scope.functions.iter().any(|f| f.template.body.bytecode().instructions.iter().any(|op|matches!(op,Instruction::CreateObjectLiteral(_)))))));
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
        Completion::Normal(Some(Value::Boolean(true)))
    ));
    assert!(BytecodeCompiler::compile("({read(){return 42;}});").is_err());
}

#[test]
fn object_methods_preserve_receiver_captures_order_and_descriptors() {
    for source in [
        "function make(value){let object={__proto__:{read(){return this.saved;}},saved:value,read(){return super.read();},get value(){return this.saved;},set value(value){this.saved=value;}};return object;}let first=make(42),second=make(99);let read=first.read;let descriptor=Object.getOwnPropertyDescriptor(first,'value');read.call(second)===99 && first.read()===42 && descriptor.enumerable && descriptor.configurable && typeof descriptor.get==='function' && typeof descriptor.set==='function';",
        "function make(){let effects=0;let key={toString(){effects++;return 'read';}};let source={get copied(){effects++;return 42;}};let object={...source,[(effects++,key)](){return this.copied;}};return effects===3 && object.read()===42;}make();",
        "function Make(){return {[new.target.name](){return this;}};}let object=new Make();object.Make()===object;",
        "function make(){'use strict';return {read(){return this;}};}let read=make().read;read()===undefined;",
        "class A{get key(){return this.keyName;}}class B extends A{make(value){return {[super.key](){return value;}};}}let owner=new B();owner.keyName='read';let object=owner.make(42);object.read()===42;",
        "function make(){let values=[];let i=0;while(i<3){let value=i;values.push({read(){return value;}});i++;}return values;}let values=make();values[0].read()===0 && values[1].read()===1 && values[2].read()===2;",
        "function make(value){return {read(){return arguments.length;},outer:()=>arguments[0]};}let object=make(42);object.read(1,2)===2 && object.outer()===42;",
        "let calls=0;let marker={};let key={toString(){calls++;throw marker;}};function make(){return {[key](){return 42;}};}let caught=false;try{make();}catch(e){caught=e===marker;}caught && calls===1;",
    ] {
        check(source);
    }
}

#[test]
fn vm_object_methods_retain_distinct_for_iteration_bindings() {
    let code=BytecodeCompiler::compile("function make(){let values=[];for(let i=0;i<3;i++){values.push({read(){return i;}});}return values;}let values=make();values[0].read()===0 && values[1].read()===1 && values[2].read()===2;").unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
        Completion::Normal(Some(Value::Boolean(true)))
    ));
}

#[test]
fn object_initializers_read_uninitialized_this_only_when_source_requires_it() {
    let realm = Realm::ordinary();
    let frame = Environment::function(
        Some(realm.global_environment()),
        realm.intrinsic(IntrinsicId::Object),
        None,
        ThisBindingStatus::Uninitialized,
    );
    assert!(
        VirtualMachine::execute_in_function(
            &function_body("function(){return {read(){return 42;}};}"),
            &realm,
            &frame
        )
        .is_ok()
    );
    let error = VirtualMachine::execute_in_function(
        &function_body("function(){return {[this.key](){return 42;}};}"),
        &realm,
        &frame,
    )
    .unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::ReferenceError);
}

#[test]
fn object_initializer_metadata_rejects_invalid_kinds_loads_indices_and_scopes() {
    let body = function_body("function(){return {read(){return 42;}};}").into_bytecode();
    let offset = body
        .instructions
        .iter()
        .position(|op| matches!(op, Instruction::CreateObjectLiteral(_)))
        .unwrap();
    let Instruction::CreateObjectLiteral(index) = body.instructions[offset] else {
        unreachable!()
    };
    let mut load = body.clone();
    load.instructions[offset] = Instruction::LoadConstant(index);
    assert!(load.verify().is_err());
    let mut wrong = body.clone();
    wrong.constants[index as usize] = Constant::Number(1.0);
    assert!(wrong.verify().is_err());
    let mut expression = body.clone();
    let Constant::ObjectLiteral { definition, .. } = &mut expression.constants[index as usize]
    else {
        unreachable!()
    };
    *definition = std::rc::Rc::new(temiya::parser::parse_expression("42").unwrap());
    assert!(expression.verify().is_err());
    let mut outside = body.clone();
    outside.instructions[offset] = Instruction::CreateObjectLiteral(u32::MAX);
    assert!(outside.verify().is_err());
    assert!(
        Bytecode {
            constants: vec![body.constants[index as usize].clone()],
            instructions: vec![Instruction::CreateObjectLiteral(0), Instruction::Return],
            local_count: 0
        }
        .verify()
        .is_err()
    );
}

#[test]
fn object_initializer_bindings_and_home_objects_survive_reentrant_collection() {
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
            let script=engine.compile(&realm,"function make(value){return {nested:{__proto__:(gcNow(),{marker:42}),[(gcNow(),'read')](){gcNow();return value;},get value(){gcNow();return value;}}};}let marker={};let object=make(marker).nested;gcNow();object.read()===marker && object.value===marker && object.marker===42;").unwrap();
            assert!(matches!(
                engine.evaluate(&script).unwrap().value(),
                Value::Boolean(true)
            ));
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn super_reference_bytecode_checks_scope_stack_and_runtime_function_operands() {
    let mut body = function_body("function(){return 1;}").into_bytecode();
    let enter = body.instructions[0];
    let undefined = body.constants.len() as u32;
    body.constants.push(Constant::Undefined);
    let invalid_function = body.constants.len() as u32;
    body.constants.push(Constant::Number(42.0));
    let key = body.constants.len() as u32;
    body.constants.push(Constant::String("value".into()));
    body.instructions = vec![
        enter,
        Instruction::LoadSuperBinding,
        Instruction::LoadConstant(key),
        Instruction::ResolveSuperProperty { strict: true },
        Instruction::LoadReference,
        Instruction::DiscardReference,
        Instruction::Return,
    ];
    let code = body.clone().verify().unwrap();
    let realm = Realm::ordinary();
    let frame = Environment::function(
        Some(realm.global_environment()),
        realm.intrinsic(IntrinsicId::Object),
        None,
        ThisBindingStatus::Uninitialized,
    );
    assert_eq!(
        VirtualMachine::execute_in_function(&code, &realm, &frame)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
    let mut no_scope = body.clone();
    no_scope.instructions.remove(0);
    assert!(no_scope.verify().is_err());
    let mut underflow = body.clone();
    underflow.instructions.remove(1);
    assert!(underflow.verify().is_err());
    body.instructions.splice(
        1..2,
        [
            Instruction::LoadConstant(undefined),
            Instruction::LoadConstant(invalid_function),
        ],
    );
    let invalid = body.verify().unwrap();
    assert_eq!(
        VirtualMachine::execute_in_function(&invalid, &realm, &frame)
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::TypeError
    );
}

#[test]
fn object_loop_reuse_keeps_body_bindings_and_nested_captures_distinct() {
    for source in [
        "function make(){let values=[];for(let i=0;i<3;i++){let value=i;let object={read(){return value;},self(){return object;}};values.push(object);}return values;}let values=make();values[0]!==values[1] && values[0].read()===0 && values[1].read()===1 && values[2].read()===2 && values[0].self()===values[0] && values[1].self()===values[1];",
        "function make(){let values=[];for(let i=0;i<3;i++){values.push({read(value=()=>i){return value();}});}return values;}let values=make();values[0].read()===0 && values[1].read()===1 && values[2].read()===2;",
        "function make(){let values=[];for(let i=0;i<3;i++){values.push({read(){return ()=>()=>i;}});}return values;}let values=make();values[0].read()()()===0 && values[1].read()()()===1 && values[2].read()()()===2;",
        "function make(){let values=[];for(let i=0;i<3;i++){values.push({...{get read(){return ()=>i;}}});}return values;}let values=make();values[0].read()===0 && values[1].read()===1 && values[2].read()===2;",
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        assert!(
            matches!(
                VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
                Completion::Normal(Some(Value::Boolean(true)))
            ),
            "{source}"
        );
    }
}
