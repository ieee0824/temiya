//! Allocation requests during repeated evaluation of already parsed numeric literals.
use std::hint::black_box;
use temiya::{
    ast::{ExprKind, Literal, NumberKind},
    parser::parse_expression,
    runtime::{ExpressionEvaluator, Realm, Value},
};
#[path = "support/allocations.rs"]
mod allocations;

// Preserve the preceding interpreter's normalization/parse algorithm as the
// allocation reference. Fixtures are valid; this is not a conformance oracle.
fn reference(raw: &str, kind: NumberKind) -> Value {
    let cleaned = raw.replace('_', "");
    let spelling = cleaned.strip_suffix('n').unwrap_or(&cleaned);
    let radix = if spelling.starts_with("0x") || spelling.starts_with("0X") {
        16
    } else if spelling.starts_with("0o") || spelling.starts_with("0O") {
        8
    } else if spelling.starts_with("0b") || spelling.starts_with("0B") {
        2
    } else if kind == NumberKind::LegacyOctal {
        8
    } else {
        10
    };
    let digits = if radix == 10 || kind == NumberKind::LegacyOctal {
        spelling
    } else {
        &spelling[2..]
    };
    if kind == NumberKind::BigInt {
        Value::BigInt(num_bigint::BigInt::parse_bytes(digits.as_bytes(), radix).unwrap())
    } else {
        Value::Number(if radix == 10 {
            spelling.parse::<f64>().unwrap()
        } else {
            u128::from_str_radix(digits, radix).unwrap() as f64
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() > 1 {
        return Err("usage: numeric_allocation_probe [ITERATIONS=10000]".into());
    }
    let n = args.first().map_or(Ok(10_000), |s| s.parse::<usize>())?;
    if !(1..=10_000_000).contains(&n) {
        return Err("iterations must be 1..10000000".into());
    }
    let evaluator = ExpressionEvaluator::new(Realm::ordinary());
    let mut cases = Vec::new();
    for (name, source) in [
        ("decimal", "42"),
        ("separated-decimal", "4_2"),
        ("hexadecimal", "0x2a"),
        ("separated-hexadecimal", "0x2_a"),
        ("legacy-octal", "052"),
        ("bigint", "42n"),
        ("separated-bigint", "4_2n"),
    ] {
        let expr = parse_expression(source)?;
        let ExprKind::Literal(Literal::Number { raw, kind }) = &expr.kind else {
            unreachable!()
        };
        let expected = if *kind == NumberKind::BigInt {
            Value::BigInt(42.into())
        } else {
            Value::Number(42.0)
        };
        assert!(evaluator.evaluate(&expr)?.same_value(&expected));
        assert!(reference(raw, *kind).same_value(&expected));
        let old = allocations::count(n, || {
            black_box(reference(black_box(raw), *kind));
        });
        let current = allocations::count(n, || {
            black_box(evaluator.evaluate(black_box(&expr)).unwrap());
        });
        cases.push(serde_json::json!({"case":name,"source":source,"validated":true,"reference":old,"current":current}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "iterations":n,"warmup_iterations":128,"timing_instrumentation":false,
            "scope":"Single-thread standalone allocation requests, including realloc. Current path evaluates a pre-parsed literal via ExpressionEvaluator. Parsing, Realm setup and reporting are excluded; not Engine throughput or RSS.",
            "cases":cases
        }))?
    );
    Ok(())
}
