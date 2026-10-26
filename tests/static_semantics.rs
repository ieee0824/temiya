use temiya::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use temiya::lexer::Span;
use temiya::parser::{parse_expression, parse_module, parse_script};
use temiya::semantics::{
    DeclarationScope, SemanticOptions, SyntaxFeature, bound_names, contains,
    lexically_declared_names, validate_expression, validate_program, var_declared_names,
};

#[test]
fn name_queries_observe_patterns_and_scope_boundaries() {
    let program=parse_script("let {a:x,b:y=outside,...rest}=object;var z;function f(){var inner;} {let block;var hoisted;}").unwrap();
    let lexical = lexically_declared_names(&program.body, DeclarationScope::Script);
    assert_eq!(
        lexical.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
        ["x", "y", "rest"]
    );
    let vars = var_declared_names(&program.body, DeclarationScope::Script);
    assert_eq!(
        vars.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
        ["z", "f", "hoisted"]
    );
    let StmtKind::Variable { declarations, .. } = &program.body[0].kind else {
        panic!()
    };
    let names = bound_names(&declarations[0].binding);
    assert_eq!(
        names.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
        ["x", "y", "rest"]
    );
    let module = parse_module("import x from 'm';export function f(){};export const y=1;").unwrap();
    assert_eq!(
        lexically_declared_names(&module.body, DeclarationScope::Module)
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["x", "f", "y"]
    );
}

#[test]
fn contains_queries_search_nested_syntax_explicitly() {
    let expr = parse_expression("async function* g(x=arguments){yield await x;return new.target;}")
        .unwrap();
    for feature in [
        SyntaxFeature::Arguments,
        SyntaxFeature::Await,
        SyntaxFeature::Yield,
        SyntaxFeature::NewTarget,
    ] {
        assert!(contains(&expr, feature));
    }
    assert!(!contains(&expr, SyntaxFeature::SuperCall));
    let expr = parse_expression("class extends Base{constructor(){super();}m(){return super.x;}}")
        .unwrap();
    assert!(contains(&expr, SyntaxFeature::SuperCall));
    assert!(contains(&expr, SyntaxFeature::SuperProperty));
}

#[test]
fn shadowing_and_var_redeclarations_remain_valid() {
    for source in [
        "var x;var x;function x(){}",
        "let x;{let x;}",
        "{let x;}var x;",
        "function f(x){var x;function x(){}}",
        "function f(x){{let x;}}",
        "try{}catch(e){var e;}",
        "try{}catch({e}){{let e;}}",
        "for(let i=0;i<1;i++){let i;}",
        "switch(x){case 0:{let x;}case 1:{let x;}}",
        "class C{m(){return this.#x;}#x;}",
        "class C{#x;m(){return class{m(o){return o.#x;}};}}",
        "'use strict';obj.eval=1;obj.arguments++;",
        "({__proto__:x,__proto__(){},['__proto__']:y})",
    ] {
        assert!(parse_script(source).is_ok(), "{source}");
    }
}

#[test]
fn duplicate_and_conflicting_declarations_report_source_ranges() {
    for source in [
        "let x;let x;",
        "let x;var x;",
        "let x;{var x;}",
        "{let x;var x;}",
        "const {a:x,b:x}=o;",
        "{function f(){}function f(){}}",
        "let f;function f(){}",
        "function f(a){let a;}",
        "function f({a}){const a=1;}",
        "for(let i=0;;){var i;}",
        "try{}catch(e){let e;}",
        "try{}catch({e}){var e;}",
        "switch(x){case 0:let a;break;case 1:let a;}",
    ] {
        let error = parse_script(source).unwrap_err();
        assert!(error.span.start <= error.span.end, "{source}");
        assert!(error.span.end <= source.len(), "{source}");
    }
    let error = parse_script("let 雪;var 雪;").unwrap_err();
    assert_eq!(&"let 雪;var 雪;"[error.span.start..error.span.end], "雪");
}

#[test]
fn module_export_and_import_conflicts_are_checked() {
    for source in [
        "export {missing};",
        "const x=1;export {x};export {x};",
        "export default 1;export default 2;",
        "export default function f(){}export default class G{}",
        "import x from 'm';let x;",
        "function f(){}function f(){}",
        "export const x=1;export {x};",
        "export * as ns from 'm';export * as ns from 'n';",
    ] {
        assert!(parse_module(source).is_err(), "{source}");
    }
    assert!(parse_module("export {x};const x=1;").is_ok());
    assert!(parse_module("export * from 'm';export * from 'n';").is_ok());
    assert!(parse_module("export {missing as x} from 'm';").is_ok());
}

#[test]
fn undeclared_private_names_and_strict_targets_are_rejected() {
    for source in [
        "class C{m(){return this.#missing;}}",
        "class C{m(o){return #missing in o;}}",
        "class C extends obj.#x{#x;}",
        "class C{#x;}class D{m(o){return o.#x;}}",
        "'use strict';eval=1;",
        "'use strict';arguments++;",
        "'use strict';[eval]=xs;",
        "'use strict';({x:arguments}=obj);",
        "({__proto__:a,'__proto__':b})",
    ] {
        assert!(parse_script(source).is_err(), "{source}");
    }
    let error = parse_script("class C{m(){return this.#missing;}}").unwrap_err();
    assert_eq!(error.message, "undeclared private name");
}

#[test]
fn public_validator_checks_context_of_manually_built_nodes() {
    let span = Span { start: 0, end: 1 };
    let identifier = Expr::new(span, ExprKind::Identifier("x".to_owned()));
    let program = Program {
        span,
        strict: false,
        body: vec![Stmt {
            span,
            kind: StmtKind::Return(Some(identifier.clone())),
        }],
    };
    assert!(validate_program(&program, SemanticOptions::default()).is_err());
    assert!(
        validate_program(
            &program,
            SemanticOptions {
                in_function: true,
                ..SemanticOptions::default()
            }
        )
        .is_ok()
    );
    let await_expr = Expr::new(
        span,
        ExprKind::Unary {
            operator: "await".to_owned(),
            argument: Box::new(identifier),
        },
    );
    assert!(validate_expression(&await_expr, SemanticOptions::default()).is_err());
    assert!(
        validate_expression(
            &await_expr,
            SemanticOptions {
                asynchronous: true,
                ..SemanticOptions::default()
            }
        )
        .is_ok()
    );
    let private = Expr::new(span, ExprKind::PrivateIdentifier("x".to_owned()));
    assert!(validate_expression(&private, SemanticOptions::default()).is_err());
}

#[test]
fn cover_initialized_names_and_rest_commas_are_context_sensitive() {
    for source in [
        "({x=1}=object);",
        "for({x=1,...rest} of values);",
        "[x=1,...rest]=values;",
        "({...object.rest}=value);",
        "({__proto__:a,__proto__:b}=object);",
    ] {
        assert!(parse_script(source).is_ok(), "{source}");
    }
    for source in [
        "({x=1});",
        "const object={x=1};",
        "([...rest,]=values);",
        "({...rest,}=object);",
        "([...rest=1]=values);",
    ] {
        assert!(parse_script(source).is_err(), "{source}");
    }
    assert!(parse_expression("[...values,]").is_ok());
    assert!(parse_expression("({...object,})").is_ok());
}

#[test]
fn validator_rejects_invalid_continue_and_yield_fragments() {
    let span = Span { start: 0, end: 1 };
    let program = Program {
        span,
        strict: false,
        body: vec![Stmt {
            span,
            kind: StmtKind::Continue(None),
        }],
    };
    assert!(validate_program(&program, SemanticOptions::default()).is_err());
    let expr = Expr::new(
        span,
        ExprKind::Yield {
            argument: None,
            delegate: false,
        },
    );
    assert!(validate_expression(&expr, SemanticOptions::default()).is_err());
    assert!(
        validate_expression(
            &expr,
            SemanticOptions {
                generator: true,
                ..SemanticOptions::default()
            }
        )
        .is_ok()
    );
}
