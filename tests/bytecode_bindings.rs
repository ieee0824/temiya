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

#[test]
fn try_bridge_preserves_completions_and_enclosing_vm_loop() {
    let source = "function run(n){let total=0;for(let i=1;i<=n;i++){let sentinel={};try{let {x}={get x(){throw sentinel;}};throw 42;}catch(error){if(error!==sentinel)throw error;}try{let {}=null;throw 42;}catch(error){if(!(error instanceof TypeError))throw error;}total+=i;}return total;}run(3)===6;";
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(code.bytecode().constants.iter().any(|c|matches!(c,Constant::Scope(scope) if scope.functions.iter().any(|f|f.template.body.bytecode().instructions.iter().any(|op|matches!(op,Instruction::ExecuteTry(_) | Instruction::BeginTryCatch(_)))))));
    check(source);
    for source in [
        "function f(){try{return 1;}finally{return 42;}}f()===42;",
        "function f(){try{throw 1;}catch(e){return e+41;}finally{let x=0;}}f()===42;",
        "function f(){let x=20;try{var y=22;let x=0;throw 1;}catch(e){x+=y;}finally{x+=0;}return x;}f()===42;",
        "function F(value){try{throw 1;}catch(e){this.x=arguments[0];this.target=new.target;}}let x=new F(42);x.x===42&&x.target===F;",
        "function f(){let read;for(let i=0;i<3;i++){try{if(i===1)read=()=>i;}finally{}}return read();}f()===1;",
        "function f(){try{try{return 1;}finally{throw 2;}}catch(e){return e+40;}}f()===42;",
    ] {
        check(source);
    }
}

#[test]
fn try_metadata_rejects_unhandled_jumps_and_missing_function_frame() {
    assert!(
        BytecodeCompiler::compile("function f(){while(true){try{break;}finally{}}}f();").is_err()
    );
    let code = BytecodeCompiler::compile("function f(){try{var x=42;}finally{}return x;}f()===42;")
        .unwrap();
    let statement = code
        .bytecode()
        .constants
        .iter()
        .find_map(|c| match c {
            Constant::Scope(scope) => scope.functions.first().and_then(|f| {
                f.template
                    .body
                    .bytecode()
                    .constants
                    .iter()
                    .find_map(|c| match c {
                        Constant::TryStatement { .. } => Some(c.clone()),
                        _ => None,
                    })
            }),
            _ => None,
        })
        .unwrap();
    let mut invalid = Bytecode {
        instructions: vec![
            Instruction::ExecuteTry(0),
            Instruction::LoadConstant(1),
            Instruction::Complete,
        ],
        constants: vec![statement, Constant::Undefined],
        local_count: 0,
    };
    assert!(invalid.clone().verify().is_err());
    invalid.instructions[0] = Instruction::LoadConstant(0);
    assert_eq!(
        invalid.verify().unwrap_err().kind(),
        VerifyErrorKind::InvalidConstantKind
    );
}

#[test]
fn bytecode_try_callbacks_survive_gc_and_release_everything() {
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
        let script=engine.compile(&realm,"function run(){try{throw {get answer(){gcNow();return 42;}};}catch({answer}){gcNow();return answer;}finally{gcNow();}}run()===42;").unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn direct_catches_unwind_calls_references_and_nested_handlers() {
    for source in [
        "function id(x){return x;}function f(){let source={get x(){throw 42;}};try{id(source.x);}catch(e){return id(e);}}f()===42;",
        "function id(x){return x;}function f(){let target={set x(v){throw 42;}};try{target.x=id(1);}catch(e){return id(e);}}f()===42;",
        "function f(){try{try{throw 20;}catch(e){throw e+22;}}catch(e){return e;}}f()===42;",
        "function f(){try{try{throw {};}catch({a=b,b=1}){return 0;}}catch(e){return e instanceof ReferenceError;}}f();",
        "function f(){let n=0;outer:for(let i=0;i<4;i++){try{if(i===0)continue outer;if(i===2)break outer;n+=40;}catch(e){throw e;}}return n+2;}f()===42;",
        "function f(){try{throw 1;}catch(e){var e=2;}return typeof e==='undefined';}f();",
        "function f(){try{throw {a:20,b:22};}catch({a,...rest}){return a+rest.b;}}f()===42;",
    ] {
        check(source);
    }
}

#[test]
fn direct_handler_verifier_rejects_entry_jumps_and_lost_frames() {
    let code =
        BytecodeCompiler::compile("function f(){try{throw 1;}catch{return 2;}}f();").unwrap();
    let body = code
        .bytecode()
        .constants
        .iter()
        .find_map(|c| match c {
            Constant::Scope(scope) => scope
                .functions
                .first()
                .map(|f| f.template.body.bytecode().clone()),
            _ => None,
        })
        .unwrap();
    let begin = body
        .instructions
        .iter()
        .position(|op| matches!(op, Instruction::BeginTryCatch(_)))
        .unwrap();
    for replacement in [
        Instruction::ExitScope,
        Instruction::Jump((begin + 2) as u32),
    ] {
        let mut invalid = body.clone();
        invalid.instructions.insert(begin, replacement);
        for op in &mut invalid.instructions {
            if let Instruction::BeginTryCatch(target) = op {
                *target += 1;
            }
        }
        if matches!(replacement, Instruction::ExitScope) {
            invalid.instructions.swap(begin, begin + 1);
        }
        assert_eq!(
            invalid.verify().unwrap_err().kind(),
            VerifyErrorKind::ScopeMismatch
        );
    }
}

#[test]
fn bytecode_direct_catch_callbacks_survive_gc_and_release_everything() {
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
        let script=engine.compile(&realm,"function run(){try{throw {get answer(){gcNow();return 42;}};}catch({answer}){gcNow();return answer;}}run()===42;").unwrap();
        assert_eq!(script.backend(), ScriptBackend::Bytecode);
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn direct_catches_do_not_swallow_internal_host_failures() {
    let context = temiya::runtime::AllocationContext::new();
    let realm = Realm::ordinary_in(&context);
    realm
        .global_object()
        .create_data_property(
            "hostFail".into(),
            Value::Object(Object::native_function_in(&context, |_, _| {
                Err(temiya::runtime::RuntimeError::internal_error(
                    "host invariant",
                ))
            })),
        )
        .unwrap();
    let code =
        BytecodeCompiler::compile("function run(){try{hostFail();}catch(e){return 42;}}run();")
            .unwrap();
    let error = VirtualMachine::execute_in_realm(&code, &realm).unwrap_err();
    assert_eq!(error.kind(), RuntimeErrorKind::InternalError);
    assert_eq!(error.message(), "host invariant");
}

#[test]
fn pattern_defaults_and_caught_getters_keep_each_iteration_cells() {
    for source in [
        "function f(){let reads=[];for(let i=0;i<3;i++){let {read=(value=>()=>value)(i)}={};reads.push(read);}return reads[0]()===0&&reads[1]()===1&&reads[2]()===2;}f();",
        "function f(){let total=0;for(let i=0;i<3;i++){let {value=i}={};total+=value;}return total===3;}f();",
        "function f(){let reads=[];for(let i=0;i<3;i++){let {read=()=>i}={};try{throw read;}catch(read){reads.push(read);}}return reads[0]()===0&&reads[1]()===1&&reads[2]()===2;}f();",
        "function f(){let reads=[];for(let i=0;i<3;i++){try{throw {get value(){return i;}};}catch(value){reads.push(value);}}return reads[0].value===0&&reads[1].value===1&&reads[2].value===2;}f();",
        "function f(){let reads=[];for(let i=0;i<2;i++){for(let j=0;j<2;j++){let {read=()=>i*2+j}={};reads.push(read);}}return reads[0]()===0&&reads[1]()===1&&reads[2]()===2&&reads[3]()===3;}f();",
        "function f(){let total=0;for(let i=0;i<3;i++){try{let {value}={value:14};total+=value;}catch(e){throw e;}}return total===42;}f();",
    ] {
        check(source);
    }
}

#[test]
fn prepared_parameter_cells_preserve_tdz_arguments_and_default_order() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        for source in [
            "var b=99;function f(a=b,b=42){return a;}function run(){try{f();return false;}catch(e){return e instanceof ReferenceError;}}run();",
            "function f(a=arguments,arguments=42){return false;}function run(){try{f();return false;}catch(e){return e instanceof ReferenceError;}}run();",
            "function f({arguments},next=arguments){return next;}f({arguments:42})===42;",
            "let order='';function f({a=(order+='a',20)},b=(order+='b',22)){return a+b;}f({})===42&&order==='ab';",
            "function f(a=()=>b,b=42){return a();}f()===42;",
            "function f({a},...rest){return a+rest[0];}f({a:20},22)===42;",
        ] {
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
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

#[test]
fn prepared_parameter_cells_trace_values_during_getters_and_defaults() {
    let engine = Engine::builder()
        .script_backend(ScriptBackend::Auto)
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
        let script=engine.compile(&realm,"function read({first=(gcNow(),{answer:20}),last},next=(gcNow(),first)){gcNow();return next.answer+last.answer;}read({get first(){gcNow();return undefined;},get last(){gcNow();return {answer:22};}})===42;").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
    assert_eq!(engine.collect_garbage().unwrap().retained, 0);
}

#[test]
fn public_name_instructions_preserve_block_shadowing() {
    use temiya::runtime::{Environment, ThisBindingStatus};
    let script =
        BytecodeCompiler::compile("function f(){let unused=7;{let answer=42;return answer;}}f();")
            .unwrap();
    let template = script
        .bytecode()
        .constants
        .iter()
        .find_map(|constant| {
            if let Constant::Scope(plan) = constant {
                plan.functions.first().map(|f| &f.template)
            } else {
                None
            }
        })
        .unwrap();
    for legacy in [false, true] {
        let mut body = template.body.bytecode().clone();
        let name = body.constants.len() as u32;
        body.constants.push(Constant::String("answer".into()));
        let mut replaced = false;
        for offset in 0..body.instructions.len() {
            if let Instruction::LoadBinding { scope, slot } = body.instructions[offset] {
                let Instruction::EnterScope(index) = body.instructions[scope as usize] else {
                    panic!("scope")
                };
                if matches!(&body.constants[index as usize], Constant::Scope(plan) if plan.bindings[slot as usize].name == "answer".into())
                {
                    body.instructions[offset] = if legacy {
                        Instruction::LoadName(name)
                    } else {
                        Instruction::GetName { name, strict: true }
                    };
                    replaced = true;
                }
            }
        }
        assert!(replaced);
        let body = body.verify().unwrap();
        let realm = Realm::ordinary();
        let frame = Environment::function(
            Some(realm.global_environment()),
            Object::new_in(realm.allocation_context()),
            None,
            ThisBindingStatus::Initialized,
        );
        frame
            .create_mutable_binding("answer".into(), false)
            .unwrap();
        frame
            .initialize_binding(&"answer".into(), Value::Number(99.0))
            .unwrap();
        assert!(matches!(
            VirtualMachine::execute_in_function(&body, &realm, &frame).unwrap(),
            Completion::Return(Value::Number(42.0))
        ));
    }
}

#[test]
fn free_names_keep_catch_shadowing_and_outer_with_receivers() {
    check(
        "let outside=42;function f(){try{throw 0;}catch(error){{let local=1;return outside+local===43;}}}f();",
    );
    check("let outside=99;function f(){try{throw 42;}catch(outside){return outside===42;}}f();");
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
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
        let script = engine.compile(&realm, "let scope={get answer(){if(this!==scope)throw 1;gcNow();return 42;}},read;with(scope){read=function(){try{throw 0;}catch(error){return answer;}};}read()===42;").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
    }
}

#[test]
fn shared_definition_context_keeps_lazy_this_and_per_call_new_target() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        for source in [
            "class B{constructor(){this.answer=42;}}class D extends B{constructor(){let {read=()=>this}={};super();let {value=this.answer,target=new.target}={};this.ok=read()===this&&value===42&&target===D;}}new D().ok;",
            "function F(){let {first=new.target}={};for(let i=0;i<2;i++){let {next=new.target}={};if(next!==first)throw 42;}this.target=first;}function G(){}let a=new F();let b=Reflect.construct(F,[],G);a.target===F&&b.target===G;",
            "function F(){let {read=()=>this}={};return read();}let a={answer:20},b={answer:22};F.call(a)===a&&F.call(b)===b;",
        ] {
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
}

#[test]
fn public_scope_root_switch_does_not_reuse_function_this_context() {
    use std::rc::Rc;
    use temiya::{
        ast::StmtKind,
        bytecode::{ScopeDescriptor, ScopeKind},
        parser::parse_script,
        runtime::{Environment, ThisBindingStatus},
    };
    let parsed = parse_script("let object={value:this};").unwrap();
    let StmtKind::Variable { declarations, .. } = &parsed.body[0].kind else {
        panic!("variable")
    };
    let definition = Rc::new(declarations[0].init.as_ref().unwrap().clone());
    let code = Bytecode {
        constants: vec![
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Function,
                strict: false,
                bindings: vec![],
                functions: vec![],
            }),
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Global,
                strict: false,
                bindings: vec![],
                functions: vec![],
            }),
            Constant::ObjectLiteral {
                definition,
                strict: false,
            },
        ],
        instructions: vec![
            Instruction::EnterScope(0),
            Instruction::CreateObjectLiteral(2),
            Instruction::Pop,
            Instruction::ExitScope,
            Instruction::EnterScope(1),
            Instruction::CreateObjectLiteral(2),
            Instruction::Return,
        ],
        local_count: 0,
    }
    .verify()
    .unwrap();
    let realm = Realm::ordinary();
    let frame = Environment::function(
        Some(realm.global_environment()),
        Object::new_in(realm.allocation_context()),
        None,
        ThisBindingStatus::Uninitialized,
    );
    frame
        .bind_this_value(Value::Object(Object::new_in(realm.allocation_context())))
        .unwrap();
    let Completion::Return(Value::Object(result)) =
        VirtualMachine::execute_in_function(&code, &realm, &frame).unwrap()
    else {
        panic!("result")
    };
    assert!(
        result
            .get(&"value".into())
            .unwrap()
            .same_value(&realm.global_environment().global_this_value().unwrap())
    );
}

#[test]
fn ast_arrow_defaults_keep_lexical_new_target() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        for source in [
            "function F(){this.answer=42;let read=()=>{let {answer=this.answer,target=new.target}={};return answer===42&&target===F;};this.ok=read();}new F().ok;",
            "function F(){let read=(target=new.target)=>target;this.ok=read()===F;}new F().ok;",
            "function F(){let read=()=>()=>new.target;this.ok=read()()===F;}new F().ok;",
            "function F(){let read=()=>function(){return new.target;};this.ok=read()()===undefined;}new F().ok;",
            "function F(){return (()=>new.target)();}F()===undefined;",
            "function F(){this.ok=((target=new.target)=>target)()===G;}function G(){}Reflect.construct(F,[],G).ok;",
        ] {
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
}

#[test]
fn lexical_object_lowering_preserves_order_primitives_and_tdz() {
    let source = "function f(){let trace='';let key={toString(){trace+='k';return 'value';}};let {[key]:a=(trace+='d',20),nested:{b}={b:22}}={get value(){trace+='g';return undefined;}};return a+b===42&&trace==='kgd';}f();";
    let code = BytecodeCompiler::compile(source).unwrap();
    let template = code
        .bytecode()
        .constants
        .iter()
        .find_map(|constant| {
            if let Constant::Scope(plan) = constant {
                plan.functions.first().map(|f| &f.template)
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        template
            .body
            .bytecode()
            .instructions
            .iter()
            .any(|op| matches!(op, Instruction::GetProperty { .. }))
    );
    // Only the coercibility check and the nested static `{b}` remain in the
    // shared instruction. The computed key/default use verified VM branches.
    let patterns: Vec<_> = template
        .body
        .bytecode()
        .constants
        .iter()
        .filter_map(|constant| match constant {
            Constant::BindingPattern { definition, .. } => Some(&definition.kind),
            _ => None,
        })
        .collect();
    assert_eq!(patterns.len(), 2);
    assert!(
        matches!(patterns[0], temiya::ast::ExprKind::Object(properties) if properties.is_empty())
    );
    assert!(
        matches!(patterns[1], temiya::ast::ExprKind::Object(properties)
        if properties.len() == 1 && matches!(&properties[0].key.kind, temiya::ast::ExprKind::Identifier(name) if name == "b"))
    );
    check(source);
    for source in [
        "function f(){try{throw {value:undefined};}catch({value=42}){return value===42;}}f();",
        "function f(){let keys=0;try{try{throw null;}catch({[++keys]:x}){return false;}}catch(e){return e instanceof TypeError&&keys===0;}}f();",
        "let {0:first,length:n}='ab';first==='a'&&n===2;",
        "function f(){let keys=0;try{let {[++keys]:value}=null;}catch(e){return e instanceof TypeError&&keys===0;}return false;}f();",
        "function f(){let caught=false;try{let {a=a}={};}catch(e){caught=e instanceof ReferenceError;}return caught;}f();",
        "function f(){let {a=42,b=21}={a:null,b:undefined};return a===null&&b===21;}f();",
        "function f(){let marker={},seen=false;try{let {x:{y}}={get x(){throw marker;}};}catch(e){seen=e===marker;}return seen;}f();",
        "function f(){let {C=class{}}={};return C.name==='C';}f();",
    ] {
        check(source);
    }
}

#[test]
fn unsupported_object_defaults_roll_back_before_shared_initialization() {
    for source in [
        "function f(){let fn;let {a=(fn=function(){return 42;})}={};return a===fn&&fn.name==='fn';}f();",
        "function f(){let fn;let {[((fn=function(){return 'x';}),fn())]:a}={x:42};return a===42&&fn.name==='fn';}f();",
        "function f(){let trace='';let {a=(trace+='a',20),fn=function(){return 22;}}={};return a+fn()===42&&fn.name==='fn'&&trace==='a';}f();",
        "function f(){let trace='';let {a=(trace+='a',20),read=()=>22}={};return a+read()===42&&read.name==='read'&&trace==='a';}f();",
        "function f(){let trace='';let {a=(trace+='a',20),...rest}={b:22};return a+rest.b===42&&trace==='a';}f();",
        "function f(){let closes=0;let input={[Symbol.iterator](){return {next(){return {done:false,value:42};},return(){closes++;return {};}};}};let {values:[x]}={values:input};return x===42&&closes===1;}f();",
    ] {
        check(source);
    }
    let code =
        BytecodeCompiler::compile("function f(){let {value=eval('42')}={};return value;}f();")
            .unwrap();
    let template = code
        .bytecode()
        .constants
        .iter()
        .find_map(|constant| {
            if let Constant::Scope(plan) = constant {
                plan.functions.first().map(|f| &f.template)
            } else {
                None
            }
        })
        .unwrap();
    assert!(template.body.bytecode().constants.iter().any(|constant|
        matches!(constant, Constant::BindingPattern {definition, ..}
            if matches!(&definition.kind, temiya::ast::ExprKind::Object(properties) if !properties.is_empty()))));
}

#[test]
fn lowered_object_source_and_initialized_cells_survive_reentrant_gc() {
    for backend in [ScriptBackend::Ast, ScriptBackend::Auto] {
        let engine = Engine::builder().script_backend(backend).build();
        {
            let realm = engine.create_realm();
            let gc = Object::native_function_with_context(
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
            let script = engine.compile(&realm, "function f(){let {first=(gcNow(),{answer:20}),last}={get first(){gcNow();return undefined;},get last(){gcNow();return {answer:22};}};gcNow();return first.answer+last.answer===42;}f();").unwrap();
            assert!(
                matches!(
                    engine.evaluate(&script).unwrap().value(),
                    Value::Boolean(true)
                ),
                "{backend:?}"
            );
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}
