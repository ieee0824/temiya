use temiya::{
    Engine,
    bytecode::{
        Bytecode, BytecodeCompiler, Constant, Instruction, VerifyErrorKind, VirtualMachine,
    },
    embedding::ScriptBackend,
    runtime::{Completion, Object, Realm, RuntimeErrorKind, Value},
};

fn check(source: &str) {
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(matches!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
        Completion::Normal(Some(Value::Boolean(true)))
    ));
}

#[test]
fn declaration_patterns_keep_function_loops_in_verified_bytecode() {
    let source = "function read(n){let total=0;for(let i=1;i<=n;i++){let {value,nested:{number}}={value:i,nested:{number:i}};if(number!==i)throw 42;total+=value;}return total;}read(3)===6;";
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(code.bytecode().constants.iter().any(|c| matches!(c,Constant::Scope(scope) if scope.functions.iter().any(|f| f.template.body.bytecode().instructions.iter().any(|op| matches!(op,Instruction::InitializePattern(_)))))));
    check(source);
    check(
        "function F(value){let {a=this.seed,b=new.target,c=arguments[0]}={};this.a=a;this.b=b;this.c=c;}F.prototype.seed=20;let object=new F(42);object.a===20&&object.b===F&&object.c===42;",
    );
    check("var {x,y=x}={x:20};const {nested:{z}}={nested:{z:22}};x+y+z===62;");
}

#[test]
fn defaults_keep_distinct_per_iteration_closure_cells() {
    check("function read(n){let {value=n?read(0):42}={};return value;}read(1)===42;");
    check(
        "function make(n){let readers=[];for(let i=0;i<n;i++){let {value=()=>i}={};readers.push(value);}return readers;}let readers=make(3);readers[0]()===0&&readers[1]()===1&&readers[2]()===2;",
    );
}

#[test]
fn proxy_rest_keeps_exclusion_descriptor_and_get_order() {
    check(
        "function run(){let trace='';let source=new Proxy({drop:20,keep:22},{ownKeys(target){trace+='K';return ['drop','keep'];},getOwnPropertyDescriptor(target,key){trace+='D';return Reflect.getOwnPropertyDescriptor(target,key);},get(target,key,receiver){trace+=key==='drop'?'T':'G';return Reflect.get(target,key,receiver);}});let {drop,...rest}=source;return drop+rest.keep===42&&trace==='TKDG';}run();",
    );
}

#[test]
fn iterator_close_tdz_and_const_errors_keep_runtime_semantics() {
    check(
        "let closed=0;function make(){return {[Symbol.iterator](){return {next(){return {value:42,done:false};},return(){closed++;return {};}};}};}let [value]=make();value===42&&closed===1;",
    );
    for (source, kind) in [
        (
            "function read(input){let {x=x}=input;return x;}read({});",
            RuntimeErrorKind::ReferenceError,
        ),
        (
            "const {value}={value:42};value=0;",
            RuntimeErrorKind::TypeError,
        ),
    ] {
        let code = BytecodeCompiler::compile(source).unwrap();
        assert_eq!(
            VirtualMachine::execute_in_realm(&code, &Realm::ordinary())
                .unwrap_err()
                .kind(),
            kind
        );
    }
}

#[test]
fn pattern_metadata_cannot_be_loaded_or_used_without_declared_bindings() {
    let code = BytecodeCompiler::compile("let {x}={x:42};x;").unwrap();
    let index = code
        .bytecode()
        .constants
        .iter()
        .position(|c| matches!(c, Constant::BindingPattern { .. }))
        .unwrap();
    let pattern = code.bytecode().constants[index].clone();
    let bad = Bytecode {
        constants: vec![pattern.clone()],
        instructions: vec![Instruction::LoadConstant(0), Instruction::Complete],
        local_count: 0,
    };
    assert_eq!(
        bad.verify().unwrap_err().kind(),
        VerifyErrorKind::InvalidConstantKind
    );
    let bad = Bytecode {
        constants: vec![pattern, Constant::Undefined],
        instructions: vec![
            Instruction::LoadConstant(1),
            Instruction::InitializePattern(0),
            Instruction::LoadConstant(1),
            Instruction::Complete,
        ],
        local_count: 0,
    };
    assert_eq!(
        bad.verify().unwrap_err().kind(),
        VerifyErrorKind::ScopeMismatch
    );
    let mut bad = code.bytecode().clone();
    for constant in &mut bad.constants {
        if let Constant::Scope(scope) = constant {
            scope.bindings.clear();
        }
    }
    assert_eq!(
        bad.verify().unwrap_err().kind(),
        VerifyErrorKind::BindingOutOfBounds
    );
    let mut bad = code.bytecode().clone();
    bad.constants[index] = Constant::Number(0.0);
    assert_eq!(
        bad.verify().unwrap_err().kind(),
        VerifyErrorKind::InvalidConstantKind
    );
}

#[test]
fn bytecode_pattern_callbacks_survive_gc_and_release_everything() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Bytecode)
        .build();
    {
        let realm = engine.create_realm();
        let gc =
            Object::native_function_with_context(engine.allocation_context(), |context, _, _| {
                context.collect_garbage()?;
                Ok(Value::Undefined)
            });
        engine
            .set_global(
                &realm,
                "gcNow",
                engine.value(&realm, Value::Object(gc)).unwrap(),
            )
            .unwrap();
        let script=engine.compile(&realm,"function run(){let source={get first(){gcNow();return undefined;},get last(){gcNow();return {answer:22};}};let {first=(gcNow(),{answer:20}),...rest}=source;gcNow();return first.answer+rest.last.answer;}run()===42;").unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn prepared_parameter_body_preserves_escaped_cells_and_frame_observers() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        let realm = engine.create_realm();
        for source in [
            "function f({a=1},read=()=>a){a=42;return read();}f({})===42;",
            "function f({a},b=arguments[0].a){var a;return a+b;}f({a:21})===42;",
            "function f({a},read=()=>a){function g(){return read();}a=42;return g();}f({a:1})===42;",
            "function F({a=this.seed},target=new.target){this.a=a;this.target=target;}F.prototype.seed=42;let x=new F({});x.a===42&&x.target===F;",
            "function sum({a},n){let total=0;for(let i=0;i<n;i++)total+=a;return total;}sum({a:14},3)===42;",
            "function f({a}){const {b=a}={};return b;}f({a:42})===42;",
        ] {
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
}
