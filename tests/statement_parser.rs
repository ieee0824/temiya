use temiya::ast::{ExprKind, ForInit, StmtKind, VariableKind};
use temiya::parser::{
    ParserOptions, parse_async_function_body, parse_function_body, parse_program, parse_script,
};
use temiya::source::SourceText;

#[test]
fn control_statements_and_for_variants_have_valid_ranges() {
    for source in [
        "",
        ";",
        "{a;{b}}",
        "if(a)if(b)x;else y;",
        "while(a){if(b)break;continue;}",
        "do x++;while(x<10) y;",
        "for(;;);",
        "for(i=0;i<3;i++)x(i);",
        "for(let i=0;i<3;i++){continue;}",
        "for(const key in object)use(key);",
        "for(var value of values)use(value)",
        "for(x in a,b);",
        "for(x of (a,b));",
        "for(var x=1 in object);",
        "for(x=(a in b);;);",
        "for(x=f(a in b);;);",
        "for(x=[a in b];;);",
        "for(x={key:a in b};;);",
        "for(x=`${a in b}`;;);",
        r"for(l\u0065t of xs);",
        r"for(\u0061sync of xs);",
        "for((let) of xs);",
        "outer:inner:for(;;){continue outer;break inner;}",
        "label:{break label;}",
        "switch(x){case 1: y();break;default: z();case 2:;}",
        "try{throw x;}catch(error){use(error)}finally{cleanup()}",
        "try{}catch{}",
        "try{}finally{}",
        "with(object){x}",
        "debugger\nx",
        "var x,y=1;let z;const k=3;",
        "let=1;",
        "label:debugger;",
        "if(x)/regexp/.test(s);",
        "a\nb",
        "a\n++b",
        "do{}while(x)foo()",
    ] {
        let program = parse_script(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        program
            .validate(&SourceText::new(source))
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        assert_eq!(program.span.end, source.len());
    }
}

#[test]
fn dangling_else_binds_to_nearest_if() {
    let program = parse_script("if(a)if(b)x;else y;").unwrap();
    let StmtKind::If {
        consequent,
        alternate,
        ..
    } = &program.body[0].kind
    else {
        panic!()
    };
    assert!(alternate.is_none());
    assert!(matches!(
        &consequent.kind,
        StmtKind::If {
            alternate: Some(_),
            ..
        }
    ));
}

#[test]
fn for_ast_distinguishes_headers() {
    let program = parse_script("for(let x=0;x<1;x++);for(const x of xs);for(x in obj);").unwrap();
    assert!(matches!(
        &program.body[0].kind,
        StmtKind::For {
            init: Some(ForInit::Variable {
                kind: VariableKind::Let,
                ..
            }),
            ..
        }
    ));
    assert!(matches!(
        &program.body[1].kind,
        StmtKind::ForEach {
            of: true,
            asynchronous: false,
            ..
        }
    ));
    assert!(matches!(
        &program.body[2].kind,
        StmtKind::ForEach { of: false, .. }
    ));
}

#[test]
fn restricted_newlines_and_automatic_semicolons() {
    for newline in ["\n", "\r", "\r\n", "\u{2028}", "\u{2029}", "/*\n*/"] {
        let source = format!("return{newline}value;");
        let program = parse_function_body(&source, ParserOptions::default()).unwrap();
        assert!(matches!(program.body[0].kind, StmtKind::Return(None)));
        assert_eq!(program.body.len(), 2);
        assert!(parse_script(&format!("throw{newline}value")).is_err());
        let source = format!("while(x){{break{newline}label;}}");
        assert!(parse_script(&source).is_ok());
    }
    let program = parse_function_body("return /x/;", ParserOptions::default()).unwrap();
    assert!(matches!(program.body[0].kind, StmtKind::Return(Some(_))));
    assert!(parse_script("return value;").is_err());
    assert!(parse_script("a b;").is_err());
    assert!(parse_script("if(x)\n").is_err());
}

#[test]
fn async_iteration_and_await_are_contextual() {
    let source = "for await(const item of items){if(await test(item))return item;}";
    assert!(parse_script(source).is_err());
    let program = parse_async_function_body(source, ParserOptions::default()).unwrap();
    program.validate(&SourceText::new(source)).unwrap();
    assert!(matches!(
        &program.body[0].kind,
        StmtKind::ForEach {
            of: true,
            asynchronous: true,
            ..
        }
    ));
    assert!(parse_async_function_body("for await(x in xs);", ParserOptions::default()).is_err());
    assert!(parse_async_function_body("for await(;;);", ParserOptions::default()).is_err());
    let program = parse_async_function_body("return await f();", ParserOptions::default()).unwrap();
    assert!(
        matches!(&program.body[0].kind, StmtKind::Return(Some(expr)) if matches!(&expr.kind, ExprKind::Unary { operator, .. } if operator == "await"))
    );
}

#[test]
fn directive_prologues_activate_strict_mode_only_in_bodies() {
    assert!(parse_script("'use strict';let x=1;").unwrap().strict);
    assert!(parse_script("'use strict';08;").is_err());
    assert!(parse_script("'use strict';with(x);").is_err());
    assert!(parse_script(r"'\1';'use strict';").is_err());
    assert!(!parse_script(r"'use\x20strict';08;").unwrap().strict);
    assert!(!parse_script("{'use strict';}08;").unwrap().strict);
    let strict = ParserOptions {
        strict: true,
        ..ParserOptions::default()
    };
    assert!(parse_program("for(var x=1 in xs);", strict).is_err());
    assert!(parse_program("var eval;", strict).is_err());
}

#[test]
fn invalid_controls_and_headers_produce_errors() {
    for source in [
        "{",
        "if(x)",
        "else x;",
        "while()x;",
        "do{}while(x",
        "break;",
        "continue;",
        "label:{continue label;}",
        "while(x){break unknown;}",
        "x:x:;",
        "switch(x){default:;default:;}",
        "switch(x){x;}",
        "try{}",
        "try x;finally{}",
        "try{}catch(){}",
        "throw;",
        "for(;;)",
        "for(x of a,b);",
        "for(a+b of xs);",
        "for(let x=1 of xs);",
        "for(let x,y of xs);",
        "for(const x;;);",
        "const x;",
        "if(x)let y;",
        "while(x)const y=1;",
        "label:let x;",
        "for(async of xs);",
        "for(let.x of xs);",
        "for(let of xs);",
        "let let=1;",
    ] {
        let error = parse_script(source).unwrap_err();
        assert!(error.span.start <= error.span.end, "{source}");
        assert!(error.span.end <= source.len(), "{source}");
    }
    let nested = format!("{};{}", "{".repeat(200), "}".repeat(200));
    assert_eq!(
        parse_script(&nested).unwrap_err().message,
        "statement nesting limit exceeded"
    );
}
