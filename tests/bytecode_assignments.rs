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
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks/destructuring-assignment/workloads-v1");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    for input in manifest["workloads"].as_array().unwrap() {
        let data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(input["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
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
            input["name"]
        );
        assert!(
            matches!(
                VirtualMachine::execute_in_realm(&code, &Realm::ordinary()).unwrap(),
                Completion::Normal(Some(Value::Boolean(true)))
            ),
            "{}",
            input["name"]
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
