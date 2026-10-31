use temiya::ast::{ClassElementKind, ExprKind, FunctionBody, MethodKind, StmtKind};
use temiya::lexer::{Lexer, LexicalGoal, TokenKind};
use temiya::parser::{parse_expression, parse_script};
use temiya::source::SourceText;

#[test]
fn declarations_patterns_and_function_forms_are_owned() {
    for source in [
        "var [a,,b=1,...rest]=xs; let {x,y:z=2,...other}=obj; const {[key]:value}=obj;",
        "for(const [a,b] of pairs){} for({x} of values);",
        "function f(a,b=2,...rest){return a+b;} function f(a,a){}",
        "function* yield(){} async function await(){} function* g(a,a){} async function f(a,a){}",
        "async function f({x}={}){return await x;} function* g(){yield;yield 1;yield* items;}",
        "async function* g(){yield await value;}",
        "let f=x=>x+1; const g=(a,b=2,...rest)=>({a,b}); let h=async (x)=>await x;",
        "let f=async=>async; let g=({x=1},[y,...z])=>x+y;",
        "let f=function named(){return new.target;}; let g=function*(){yield 1;};",
        "let o={method(x){return super.method(x)},get x(){return 1},set x(v){},async f(){await x},*g(){yield x},async *h(){yield await x}};",
        "label:while(x){function f(){while(y){break;}}break label;}",
        "async function outer(){let f=()=>await;function inner(){let await=1;}}",
        "async function outer(){class C{x=await;}let f=function await(){};}",
        "function* g(){yield\n+1;yield\n/regex/;}",
    ] {
        let program = parse_script(source).unwrap_or_else(|e| panic!("{source}: {e}"));
        program
            .validate(&SourceText::new(source))
            .unwrap_or_else(|e| panic!("{source}: {e}"));
    }
    let function = {
        let source = String::from("(x=1)=>x");
        parse_expression(&source).unwrap()
    };
    assert!(matches!(function.kind, ExprKind::Arrow(_)));
}

#[test]
fn class_methods_fields_and_private_syntax() {
    for source in [
        "class Base{} class C extends Base{constructor(x){super(x);}method(){return super.x;}}",
        "class C{#x=1; static #y; get #value(){return this.#x;}set #value(v){this.#x=v;} has(o){return #x in o;}}",
        "class C{field=1\nother;['x']=2;static count=0;static{let x=1;this.count=x;}async f(){await x}*g(){yield x}async *h(){yield await x}}",
        "let C=class extends Base{constructor(){let f=()=>super();f();}m(){return ()=>super.x;}};",
        "class C{static(){} async(){} get(){} set(){} *constructorName(){} static constructor(){}}",
        "class C{x=new.target;y=await;z=function(){return arguments;}; m(){class Inner{#x;f(o){return o.#x;}}}}",
        "class C{get #x(){}set #x(v){} static get y(){return 1}static set y(v){}}",
    ] {
        let program = parse_script(source).unwrap_or_else(|e| panic!("{source}: {e}"));
        program
            .validate(&SourceText::new(source))
            .unwrap_or_else(|e| panic!("{source}: {e}"));
    }
}

#[test]
fn ast_preserves_function_and_class_categories() {
    let program = parse_script(
        "async function* g(x=1,...rest){yield x;} class C{get x(){return 1}static y=2}",
    )
    .unwrap();
    let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
        panic!()
    };
    assert!(function.asynchronous && function.generator);
    assert_eq!(function.parameters.len(), 2);
    assert!(matches!(function.body, FunctionBody::Block(_)));
    let StmtKind::ClassDeclaration(class) = &program.body[1].kind else {
        panic!()
    };
    assert!(matches!(
        class.elements[0].kind,
        ClassElementKind::Method {
            kind: MethodKind::Get,
            ..
        }
    ));
    assert!(class.elements[1].is_static);
    let expr = parse_expression("a=>b=>a+b").unwrap();
    let ExprKind::Arrow(function) = expr.kind else {
        panic!()
    };
    assert!(
        matches!(&function.body, FunctionBody::Expression(body) if matches!(body.kind, ExprKind::Arrow(_)))
    );
}

#[test]
fn invalid_parameters_and_context_leaks_are_rejected() {
    for source in [
        "function(){}",
        "function f(...a,){}",
        "function f(...a=1){}",
        "function f(a=1,a){}",
        "function f(a,a){'use strict';}",
        "function f(a=1){'use strict';}",
        "async function f(a=await x){}",
        "function* f(a=yield x){}",
        "const f=(a,a)=>1;",
        "let f=x\n=>x;",
        "let f=async x\n=>x;",
        "async function f(){let g=(await)=>1;}",
        "function* f(){let g=(yield)=>1;}",
        "var [a];",
        "for(var [a];;);",
        "for(var [a]=xs in obj);",
        "let {...{a}}=o;",
        "let [a,...rest,]=xs;",
        "let {...a,b}=o;",
        "while(x){function f(){break;}}",
        "label:while(x){function f(){break label;}}",
        "let o={get x(a){},set x(){}};",
        "let o={set x(...a){}};",
        "let o={f(a,a){}};",
        "if(x)function f(){}",
        "if(x)class C{}",
        "class C{m(){function f(){return super.x;}}}",
    ] {
        assert!(parse_script(source).is_err(), "{source}");
    }
    for source in [
        "a+ b=>b",
        "!a=>a",
        "a=>{}()",
        "a=>{}+1",
        "new.target",
        "super.x",
        "obj.#x",
        "#x in obj",
    ] {
        assert!(parse_expression(source).is_err(), "{source}");
    }
}

#[test]
fn invalid_class_elements_and_super_uses_are_rejected() {
    for source in [
        "class{}",
        "class C{constructor(){}constructor(){}}",
        "class C{async constructor(){}}",
        "class C{get constructor(){}}",
        "class C{constructor;}",
        "class C{static constructor;}",
        "class C{static prototype(){}}",
        "class C{#constructor;}",
        "class C{#x;#x;}",
        "class C{static #x;#x;}",
        "class C{get #x(){}get #x(){}}",
        "class C{get #x(){}set #x(v){}set #x(v){}}",
        "class C{m(){super();}}",
        "class C{constructor(){super();}}",
        "class C extends B{constructor(){super.#x;}}",
        "class C{static{return 1;}}",
        "class C{static{await x;}}",
        "class C{x=arguments;}",
        "class C{x=()=>arguments;}",
        "class C{x=super();}",
        "class C{#x;m(){delete (this.#x);}}",
        "let o={#x:1};",
        "class C{m(){return #x;}}",
    ] {
        assert!(parse_script(source).is_err(), "{source}");
    }
}

#[test]
fn private_identifier_lexing_preserves_raw_spelling() {
    let source = r"#\u0078";
    let token = Lexer::new(source).next_token(LexicalGoal::Div).unwrap();
    assert_eq!(token.kind, TokenKind::PrivateIdentifier);
    assert_eq!(token.text, source);
    assert_eq!(token.identifier_name().as_deref(), Some("x"));
    assert_eq!(token.span.end, source.len());
    for source in ["#", "#1", "#\\u0030"] {
        assert!(Lexer::new(source).next_token(LexicalGoal::Div).is_err());
    }
}

#[test]
fn declaration_errors_and_ast_ranges_remain_checked() {
    for source in [
        "function 雪(",
        "class 雪{",
        "const {[雪]:x",
        "let f=([a,...b,",
    ] {
        let error = parse_script(source).unwrap_err();
        assert!(error.span.start <= error.span.end);
        assert!(error.span.end <= source.len());
        assert!(source.is_char_boundary(error.span.start));
        assert!(source.is_char_boundary(error.span.end));
    }
    let source = "function f(x){return x;}";
    let mut program = parse_script(source).unwrap();
    let StmtKind::FunctionDeclaration(function) = &mut program.body[0].kind else {
        panic!()
    };
    function.parameters[0].span.end = source.len() + 1;
    assert!(program.validate(&SourceText::new(source)).is_err());
    let expr = parse_expression("function f(x){return x;}").unwrap();
    let mut names = vec![];
    expr.visit(&mut |node| {
        if let ExprKind::Identifier(name) = &node.kind {
            names.push(name.clone());
        }
    });
    assert_eq!(names, ["f", "x", "x"]);
}

#[test]
fn module_await_reservations_apply_to_nested_syntax() {
    use temiya::lexer::{LexerOptions, SourceGoal};
    use temiya::parser::{ParserOptions, parse_program};
    let options = ParserOptions {
        lexer: LexerOptions {
            source_goal: SourceGoal::Module,
            html_comments: false,
        },
        ..ParserOptions::default()
    };
    for source in [
        "function await(){}",
        "const f=function await(){}",
        "class C{x=await}",
        "function f(){return await;}",
    ] {
        assert!(parse_program(source, options).is_err(), "{source}");
    }
    assert!(parse_program("async function f(){return await g();}", options).is_ok());
}

#[test]
fn accessor_named_fields_use_asi_before_generator_methods() {
    for prefix in ["get", "set"] {
        for static_prefix in ["", "static "] {
            let source =
                format!("class C{{{static_prefix}{prefix}\n*method(value){{return value;}}}}");
            let program = parse_script(&source).unwrap();
            program.validate(&SourceText::new(&source)).unwrap();
            let StmtKind::ClassDeclaration(class) = &program.body[0].kind else {
                panic!("class")
            };
            assert_eq!(class.elements.len(), 2);
            assert!(matches!(
                class.elements[0].kind,
                ClassElementKind::Field(None)
            ));
            assert_eq!(class.elements[0].is_static, !static_prefix.is_empty());
            assert!(!class.elements[1].is_static);
            assert!(
                matches!(&class.elements[1].kind,ClassElementKind::Method{function,kind:MethodKind::Method} if function.generator)
            );
        }
    }
    for source in [
        "class C{get *method(){}}",
        "class C{set *method(value){}}",
        "({get\n*method(){}})",
        "class C{get\nvalue=1}",
    ] {
        assert!(parse_script(source).is_err(), "{source}");
    }
    let program = parse_script("class C{get\nvalue(){return 42;}set\nvalue(value){}}").unwrap();
    let StmtKind::ClassDeclaration(class) = &program.body[0].kind else {
        panic!("class")
    };
    assert!(matches!(
        class.elements[0].kind,
        ClassElementKind::Method {
            kind: MethodKind::Get,
            ..
        }
    ));
    assert!(matches!(
        class.elements[1].kind,
        ClassElementKind::Method {
            kind: MethodKind::Set,
            ..
        }
    ));
}
