use temiya::ast::{ExportKind, ExprKind, ImportName, StmtKind};
use temiya::lexer::{LexerOptions, SourceGoal};
use temiya::parser::{ParserOptions, parse_expression, parse_module, parse_program, parse_script};
use temiya::source::SourceText;

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn module_forms_and_top_level_await_have_owned_ranges() {
    for source in [
        "",
        "import 'side';",
        "import value from 'm';",
        "import * as ns from 'm';",
        "import value,{a,b as c,'x-y' as x,default as d,} from 'm';",
        "import value,* as ns from 'm';",
        "import {} from 'm' with {};",
        "import from from 'm';",
        "import {as as as,yield as y,await as a} from 'm';",
        "import data from 'm' with {type:'json','extra':'yes',};",
        "import 'm'\nwith {type:'json'};",
        "export * from 'm';",
        "export * as ns from 'm';",
        "export * as 'x-y' from 'm' with {type:'json'};",
        "export {default as x,'x-y' as 'other'} from 'm';",
        "const x=1;export {x,x as 'public-x'};",
        "export {};",
        "export var a=1; export let b=2;export const {x}=obj;",
        "export function f(){} export async function* g(){yield await value;}",
        "export class C{}",
        "export default function(){}",
        "export default async function*(){}",
        "export default class extends Base{}",
        "export default (x=>x);",
        "const result=await import('m',{with:{type:'json'}});",
        "for await(const x of items)use(x);",
        "function f(){return import.meta.url;}",
        "export default await load();",
        "export default async function named(){}",
    ] {
        let program = parse_module(source).unwrap_or_else(|e| panic!("{source}: {e}"));
        assert!(program.strict);
        program
            .validate(&SourceText::new(source))
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(program.span.end, source.len());
    }
}

#[test]
fn import_and_export_ast_records_preserve_decoded_names() {
    let source = "import d,{a as local,'x-y' as odd} from 'm';export {local as 'public'};";
    let program = parse_module(source).unwrap();
    let StmtKind::ImportDeclaration(import) = &program.body[0].kind else {
        panic!()
    };
    assert_eq!(import.specifiers.len(), 3);
    assert_eq!(
        import.specifiers[0].imported,
        ImportName::Name(utf16("default"))
    );
    assert_eq!(
        import.specifiers[2].imported,
        ImportName::Name(utf16("x-y"))
    );
    assert!(matches!(&import.specifiers[1].local.kind,ExprKind::Identifier(name) if name=="local"));
    let StmtKind::ExportDeclaration(export) = &program.body[1].kind else {
        panic!()
    };
    let ExportKind::Named {
        specifiers,
        request,
    } = &export.kind
    else {
        panic!()
    };
    assert!(request.is_none());
    assert_eq!(specifiers[0].exported, utf16("public"));
    assert_eq!(specifiers[0].local, utf16("local"));
}

#[test]
fn requests_are_unique_and_attribute_order_is_not_identity() {
    let source = "import a from 'm' with {type:'json',mode:'one'};export {default as b} from 'm' with {mode:'one',type:'json'};import 'm' with {type:'text',mode:'one'};import('dynamic');export * from 'n';";
    let program = parse_module(source).unwrap();
    let requests = program.module_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].specifier, utf16("m"));
    assert_eq!(requests[0].attributes[0].key, utf16("mode"));
    assert_eq!(requests[0].attributes[1].value, utf16("json"));
    assert_eq!(requests[1].attributes[1].value, utf16("text"));
    assert_eq!(requests[2].specifier, utf16("n"));
    let StmtKind::ImportDeclaration(import) = &program.body[0].kind else {
        panic!()
    };
    assert_eq!(import.request.attributes[0].key, utf16("type"));
    assert_eq!(requests[0].span, import.request.span);
}

#[test]
fn dynamic_import_and_import_meta_are_distinct_from_static_requests() {
    for source in [
        "import('m')",
        "import('m',)",
        "import('m',{with:{type:'json'}},)",
        "import('m').then(use)",
    ] {
        let expr = parse_expression(source).unwrap();
        expr.validate(&SourceText::new(source)).unwrap();
        assert!(parse_script(source).unwrap().module_requests().is_empty());
    }
    let expr = parse_expression("import('m')").unwrap();
    assert!(matches!(
        expr.kind,
        ExprKind::ImportCall { options: None, .. }
    ));
    let program = parse_module("import.meta;").unwrap();
    assert!(
        matches!(&program.body[0].kind,StmtKind::Expression(expr) if matches!(&expr.kind,ExprKind::MetaProperty {meta,property} if meta=="import" && property=="meta"))
    );
    assert!(parse_script("import.meta").is_err());
    for source in [
        "import()",
        "import(...x)",
        "import('m',{},1)",
        "import?.('m')",
        "new import('m')",
        "import.foo",
    ] {
        assert!(parse_expression(source).is_err(), "{source}");
    }
}

#[test]
fn invalid_module_positions_names_and_attributes_have_spans() {
    for source in [
        "{import 'm';}",
        "function f(){import 'm';}",
        "{export {};}",
        "import d, from 'm';",
        "import {default} from 'm';",
        "import {'x'} from 'm';",
        "import {a as x,b as x} from 'm';",
        "import await from 'm';",
        "import {x as eval} from 'm';",
        "import 'm' with {type:1};",
        "import 'm' with {type:'json','type':'text'};",
        "import 'm' with {ty\\u0070e:'json',type:'text'};",
        "import 'm' with {#x:'a'};",
        "import {#x as x} from 'm';",
        "import {a} 'm';",
        "export {default};",
        "export {'x'};",
        "export {await};",
        "export * 'm';",
        "export default;",
        "export if(x);",
        "export {'\\uD800' as x} from 'm';",
        "import {'\\uD800' as x} from 'm';",
        "<!--comment\nx;",
        "import '雪' with {type:",
        "export {x as",
        "export default function(",
    ] {
        let error = parse_module(source).unwrap_err();
        assert!(error.span.start <= error.span.end, "{source}");
        assert!(error.span.end <= source.len(), "{source}");
        assert!(source.is_char_boundary(error.span.start));
        assert!(source.is_char_boundary(error.span.end));
    }
    assert!(parse_script("import 'm';").is_err());
    assert!(parse_script("export {};").is_err());
    let options = ParserOptions {
        lexer: LexerOptions {
            source_goal: SourceGoal::Module,
            html_comments: true,
        },
        ..ParserOptions::default()
    };
    assert!(parse_program("<!--comment", options).is_err());
}

#[test]
fn module_nodes_validate_auxiliary_spans() {
    let source = "import 'm' with {type:'json'};";
    let mut program = parse_module(source).unwrap();
    let StmtKind::ImportDeclaration(import) = &mut program.body[0].kind else {
        panic!()
    };
    import.request.attributes[0].span.end = source.len() + 1;
    assert!(program.validate(&SourceText::new(source)).is_err());
}
