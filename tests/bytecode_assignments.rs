use std::rc::Rc;
use temiya::{
    bytecode::{
        Bytecode, BytecodeCompiler, Constant, Instruction, ScopeDescriptor, ScopeKind,
        VerifyErrorKind, VirtualMachine,
    },
    parser::parse_expression,
    runtime::{Completion, Realm, RuntimeErrorKind, Value},
};

fn check(source: &str) {
    let code = BytecodeCompiler::compile(source).unwrap();
    assert!(
        matches!(
            VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
            Completion::Normal(Some(Value::Boolean(true)))
        ),
        "{source}"
    );
}

#[test]
fn frozen_assignment_functions_are_verified_vm_bodies() {
    // Embed the original frozen inputs so strict Miri needs no host filesystem access.
    for input in [
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-array-members.json"
        ),
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-object-members.json"
        ),
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-computed-default.json"
        ),
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-rest-symbol.json"
        ),
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-close-priority.json"
        ),
        include_str!(
            "../benchmarks/destructuring-assignment/workloads-v1/assignment-const-failure.json"
        ),
    ] {
        let data: serde_json::Value = serde_json::from_str(input).unwrap();
        let source = format!("{}bench(3)===6;", data["setup"].as_str().unwrap());
        let code = BytecodeCompiler::compile(&source).unwrap();
        let template = code
            .bytecode()
            .constants
            .iter()
            .find_map(|c| {
                if let Constant::Scope(scope) = c {
                    scope.functions.first().map(|f| &f.template)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(
            template
                .body
                .bytecode()
                .constants
                .iter()
                .enumerate()
                .any(|(index, c)| matches!(c, Constant::AssignmentPattern { .. })
                    && template
                        .body
                        .bytecode()
                        .instructions
                        .contains(&Instruction::InitializePattern(index as u32))),
            "{}",
            data["name"]
        );
        assert!(
            matches!(
                VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
                Completion::Normal(Some(Value::Boolean(true)))
            ),
            "{}",
            data["name"]
        );
    }
}

#[test]
fn assignment_vm_preserves_rhs_cells_arguments_and_definition_context() {
    for source in [
        "let rhs={a:42};let result=({a:created}=rhs);result===rhs&&created===42;",
        "function make(){let readers=[],value;for(let i=0;i<3;i++){[value=()=>i]=[];readers.push(value);}return readers;}let readers=make();readers[0]()===0&&readers[1]()===1&&readers[2]()===2;",
        "function f(x){'use strict';[arguments[0]]=[42];return x===1&&arguments[0]===42;}f(1);",
        "function F(){[this.value=new.target]=[];}let object=new F();object.value===F;",
        "let value;({value=function(){}}={});value.name==='value';",
    ] {
        check(source);
    }
    let code = BytecodeCompiler::compile("'use strict';({value:missing}={value:42});").unwrap();
    assert_eq!(
        VirtualMachine::execute_in_realm(&code, &Realm::ordinary())
            .unwrap_err()
            .kind(),
        RuntimeErrorKind::ReferenceError
    );
}

fn fixture(pattern: &str, assignment: bool, instructions: Vec<Instruction>) -> Bytecode {
    let definition = Rc::new(parse_expression(pattern).unwrap());
    Bytecode {
        constants: vec![
            Constant::Scope(ScopeDescriptor {
                kind: ScopeKind::Global,
                strict: false,
                bindings: vec![],
                functions: vec![],
            }),
            if assignment {
                Constant::AssignmentPattern {
                    definition,
                    strict: false,
                }
            } else {
                Constant::BindingPattern {
                    definition,
                    initialize: false,
                    strict: false,
                }
            },
            Constant::Undefined,
        ],
        instructions,
        local_count: 0,
    }
}
#[test]
fn assignment_metadata_keeps_binding_validation_and_rejects_runtime_loading() {
    let body = vec![
        Instruction::EnterScope(0),
        Instruction::LoadConstant(2),
        Instruction::InitializePattern(1),
        Instruction::LoadConstant(2),
        Instruction::ExitScope,
        Instruction::Complete,
    ];
    assert!(fixture("[obj.value]", true, body.clone()).verify().is_ok());
    assert!(fixture("[unbound]", true, body.clone()).verify().is_ok());
    assert_eq!(
        fixture("[obj.value]", false, body.clone())
            .verify()
            .unwrap_err()
            .kind(),
        VerifyErrorKind::InvalidConstantKind
    );
    assert_eq!(
        fixture("[unbound]", false, body.clone())
            .verify()
            .unwrap_err()
            .kind(),
        VerifyErrorKind::BindingOutOfBounds
    );
    assert_eq!(
        fixture("[42]", true, body.clone())
            .verify()
            .unwrap_err()
            .kind(),
        VerifyErrorKind::InvalidConstantKind
    );
    assert_eq!(
        fixture(
            "[obj.value]",
            true,
            vec![
                Instruction::LoadConstant(2),
                Instruction::InitializePattern(1),
                Instruction::LoadConstant(2),
                Instruction::Complete
            ]
        )
        .verify()
        .unwrap_err()
        .kind(),
        VerifyErrorKind::ScopeMismatch
    );
    assert_eq!(
        fixture(
            "[obj.value]",
            true,
            vec![Instruction::LoadConstant(1), Instruction::Complete]
        )
        .verify()
        .unwrap_err()
        .kind(),
        VerifyErrorKind::InvalidConstantKind
    );
}

#[test]
fn lexical_block_functions_match_ast_without_claiming_annex_b_aliases() {
    // Isolate lexical block declarations from the separate AST for-let #173.
    let source = "function run(){let readers=[],i=0;while(i<3){let value=i;function read(){return value;}readers.push(read);i++;}return typeof read==='undefined'&&readers[0]()===0&&readers[1]()===1&&readers[2]()===2;}run();";
    check(source);
    let realm = Realm::ordinary();
    let ast = temiya::runtime::StatementEvaluator::new(realm)
        .evaluate_program(&temiya::parser::parse_script(source).unwrap())
        .unwrap();
    assert!(matches!(
        ast,
        Completion::Normal(Some(Value::Boolean(true)))
    ));
}

#[test]
fn static_assignment_references_precede_getters_and_preserve_abrupt_order() {
    for source in [
        "function run(){let target={},old=target,other=0;let source={get x(){target={};return 42;},get y(){return 1;}};let result=({x:target.value,y:other}=source);return result===source&&old.value===42&&target.value===undefined&&other===1;}run();",
        "function run(){'use strict';let first=0,after=0,reads='',caught=false;const last=1;try{({a:first,b:last,c:after}={get a(){reads+='A';return 42;},get b(){reads+='B';return 99;},get c(){reads+='C';return 1;}});}catch(e){caught=e instanceof TypeError;}return first===42&&last===1&&after===0&&reads==='AB'&&caught;}run();",
        "function run(){'use strict';let calls=0,caught=false;try{({x:missing}={get x(){calls++;return 42;}});}catch(e){caught=e instanceof ReferenceError;}return calls===1&&caught;}run();",
        "function run(){let calls=0,caught=false;try{({x:missing.value}={get x(){calls++;return 42;}});}catch(e){caught=e instanceof ReferenceError;}return calls===0&&caught;}run();",
        "function run(){let caught=false;try{({x:missing.value}=null);}catch(e){caught=e instanceof TypeError;}return caught;}run();",
        "function run(){let caught=false;try{({}=undefined);}catch(e){caught=e instanceof TypeError;}return caught;}run();",
        "function run(){Object.defineProperty(Number.prototype,'answer',{get:function(){'use strict';return typeof this==='number'&&this===20?42:0;},configurable:true});let result;({answer:result}=20);return result===42;}run();",
    ] {
        check(source);
    }
}
