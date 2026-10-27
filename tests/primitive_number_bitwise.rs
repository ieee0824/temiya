use num_bigint::BigInt;
use temiya::{
    bytecode::{BytecodeCompiler, VirtualMachine},
    parser::parse_expression,
    runtime::{Completion, ExpressionEvaluator, Realm, Value},
};

// Independent exact-integer oracle from IEEE-754 bits; no native floating-point
// integer cast, truncation or floating remainder is used to obtain the result.
fn expected(value: f64) -> u32 {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    if exponent == 0x7ff {
        return 0;
    }
    let fraction = bits & ((1_u64 << 52) - 1);
    let significand = if exponent == 0 {
        fraction
    } else {
        fraction | (1_u64 << 52)
    };
    let shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut integer = BigInt::from(significand);
    if shift >= 0 {
        integer <<= shift as usize;
    } else {
        integer >>= (-shift) as usize;
    }
    if bits >> 63 != 0 {
        integer = -integer;
    }
    let (_, digits) = (integer & BigInt::from(u32::MAX)).to_u32_digits();
    digits.first().copied().unwrap_or(0)
}

#[test]
fn primitive_number_bitwise_operations_match_exact_bits_in_ast_and_vm() {
    let realm = Realm::ordinary();
    let environment = realm.global_environment();
    let name = temiya::runtime::JsString::from("input");
    environment
        .create_mutable_binding(name.clone(), false)
        .unwrap();
    environment
        .initialize_binding(&name, Value::Number(0.0))
        .unwrap();
    let evaluator = ExpressionEvaluator::new(realm.clone());
    let sources = [
        "input >>> 0",
        "input >> 0",
        "input | 0",
        "input & 2",
        "input ^ 42",
        "input << 33",
        "input >> 33",
        "input >>> 33",
        "1 << input",
        "-42 >> input",
        "4294967295 >>> input",
    ];
    let ast = sources.map(|s| parse_expression(s).unwrap());
    let vm = sources.map(|s| BytecodeCompiler::compile(s).unwrap());
    let mut inputs = vec![
        0.0,
        -0.0,
        1.9,
        -1.9,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
    ];
    for boundary in [
        2_f64.powi(31),
        2_f64.powi(32),
        2_f64.powi(53),
        2_f64.powi(63),
        2_f64.powi(64),
    ] {
        for bits in [
            boundary.to_bits() - 1,
            boundary.to_bits(),
            boundary.to_bits() + 1,
        ] {
            inputs.push(f64::from_bits(bits));
            inputs.push(-f64::from_bits(bits));
        }
    }
    let mut state = 0x28ed_f642_f73b_19a1_u64;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        inputs.push(f64::from_bits(state));
    }
    for input in inputs {
        environment
            .set_mutable_binding(&name, Value::Number(input), true)
            .unwrap();
        let unsigned = expected(input);
        let signed = unsigned as i32;
        let results = [
            f64::from(unsigned),
            f64::from(signed),
            f64::from(signed),
            f64::from(signed & 2),
            f64::from(signed ^ 42),
            f64::from(signed.wrapping_shl(1)),
            f64::from(signed.wrapping_shr(1)),
            f64::from(unsigned.wrapping_shr(1)),
            f64::from(1_i32.wrapping_shl(unsigned & 31)),
            f64::from((-42_i32).wrapping_shr(unsigned & 31)),
            f64::from(u32::MAX.wrapping_shr(unsigned & 31)),
        ];
        for (i, value) in results.into_iter().enumerate() {
            let expected = Value::Number(value);
            assert!(
                evaluator.evaluate(&ast[i]).unwrap().same_value(&expected),
                "AST {} input bits {:016x}",
                sources[i],
                input.to_bits()
            );
            let Completion::Normal(Some(actual)) =
                VirtualMachine::execute_in_realm(&vm[i], &realm).unwrap()
            else {
                panic!()
            };
            assert!(
                actual.same_value(&expected),
                "VM {} input bits {:016x}",
                sources[i],
                input.to_bits()
            );
        }
    }
}
