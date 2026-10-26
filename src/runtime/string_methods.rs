//! String indexing and slicing operate on UTF-16 code units, without UTF-8 conversion.

use super::{
    IntrinsicId, IntrinsicRegistry, JsString, PropertyKey, RuntimeError, RuntimeResult, Value,
    WellKnownSymbol, WellKnownSymbols, array_methods::method, coercion::primitive,
};

#[derive(Clone, Copy)]
enum Operation {
    CharAt,
    CharCodeAt,
    CodePointAt,
    At,
    Slice,
    Substring,
}

pub(super) fn install(
    intrinsics: &IntrinsicRegistry,
    symbols: &WellKnownSymbols,
) -> RuntimeResult<()> {
    let prototype = intrinsics.get(IntrinsicId::StringPrototype);
    let function_prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    let constructor = intrinsics.get(IntrinsicId::String);
    let to_primitive: PropertyKey = symbols.get(WellKnownSymbol::ToPrimitive).into();
    let call_key = to_primitive.clone();
    constructor.set_native_call(move |_, args| {
        // String(symbol) alone permits SymbolDescriptiveString. A Symbol
        // returned by an object's coercion still throws, as does construction.
        if let Some(Value::Symbol(symbol)) = args.first() {
            let mut units = JsString::from("Symbol(").code_units().to_vec();
            if let Some(description) = symbol.description() {
                units.extend_from_slice(description.code_units());
            }
            units.push(u16::from(b')'));
            return Ok(Value::String(JsString::from_code_units(units)));
        }
        constructor_string(args, &call_key).map(Value::String)
    });
    let instance_prototype = prototype.clone();
    constructor.set_native_construct(move |args, _| {
        let object = super::StringObject::new(constructor_string(args, &to_primitive)?).object();
        object.set_prototype(Some(instance_prototype.clone()));
        Ok(object)
    });
    for name in ["toString", "valueOf"] {
        method(
            &prototype,
            &function_prototype,
            name,
            0,
            |this, _| match this {
                Value::String(_) => Ok(this),
                Value::Object(object) => object
                    .0
                    .state
                    .borrow()
                    .string_data
                    .clone()
                    .map(Value::String)
                    .ok_or_else(|| RuntimeError::type_error("receiver has no String data")),
                _ => Err(RuntimeError::type_error("receiver has no String data")),
            },
        )?;
    }
    for (name, arity, operation) in [
        ("charAt", 1, Operation::CharAt),
        ("charCodeAt", 1, Operation::CharCodeAt),
        ("codePointAt", 1, Operation::CodePointAt),
        ("at", 1, Operation::At),
        ("slice", 2, Operation::Slice),
        ("substring", 2, Operation::Substring),
    ] {
        let to_primitive = symbols.get(WellKnownSymbol::ToPrimitive).into();
        method(
            &prototype,
            &function_prototype,
            name,
            arity,
            move |this, args| {
                let string = match this {
                    Value::Null | Value::Undefined => {
                        return Err(RuntimeError::type_error(
                            "string method receiver is null or undefined",
                        ));
                    }
                    // Move the existing string instead of copying its buffer again.
                    Value::String(string) => string,
                    _ => primitive(&this, &to_primitive, true)?.to_js_string()?,
                };
                evaluate(operation, &string, args, &to_primitive)
            },
        )?;
    }
    Ok(())
}

fn constructor_string(args: &[Value], to_primitive: &PropertyKey) -> RuntimeResult<JsString> {
    match args.first() {
        None => Ok(JsString::default()),
        Some(value) => primitive(value, to_primitive, true)?.to_js_string(),
    }
}

fn integer(value: Option<&Value>, to_primitive: &PropertyKey) -> RuntimeResult<f64> {
    let value = value.unwrap_or(&Value::Undefined);
    let number = primitive(value, to_primitive, false)?.to_number()?;
    Ok(if number.is_nan() { 0.0 } else { number.trunc() })
}

fn evaluate(
    operation: Operation,
    string: &JsString,
    args: &[Value],
    to_primitive: &PropertyKey,
) -> RuntimeResult<Value> {
    let units = string.code_units();
    let length = units.len() as f64;
    let start = integer(args.first(), to_primitive)?;
    if matches!(operation, Operation::Slice | Operation::Substring) {
        // Convert end even when start would make the result empty.
        let end = match args.get(1) {
            None | Some(Value::Undefined) => length,
            value => integer(value, to_primitive)?,
        };
        let clamp = |index: f64| index.max(0.0).min(length) as usize;
        let (start, end) = if matches!(operation, Operation::Slice) {
            let relative = |index: f64| clamp(if index < 0.0 { length + index } else { index });
            let start = relative(start);
            (start, relative(end).max(start))
        } else {
            let (start, end) = (clamp(start), clamp(end));
            (start.min(end), start.max(end))
        };
        return Ok(Value::String(JsString::from_code_units(
            units[start..end].to_vec(),
        )));
    }
    let index = if matches!(operation, Operation::At) && start < 0.0 {
        length + start
    } else {
        start
    };
    if index < 0.0 || index >= length {
        return Ok(match operation {
            Operation::CharAt => Value::String(JsString::default()),
            Operation::CharCodeAt => Value::Number(f64::NAN),
            _ => Value::Undefined,
        });
    }
    let index = index as usize;
    let unit = units[index];
    match operation {
        Operation::CharAt | Operation::At => {
            Ok(Value::String(JsString::from_code_units(vec![unit])))
        }
        Operation::CodePointAt if (0xD800..=0xDBFF).contains(&unit) => {
            let point = match units.get(index + 1) {
                Some(&low) if (0xDC00..=0xDFFF).contains(&low) => {
                    0x10000 + ((u32::from(unit) - 0xD800) << 10) + u32::from(low) - 0xDC00
                }
                _ => u32::from(unit),
            };
            Ok(Value::Number(f64::from(point)))
        }
        _ => Ok(Value::Number(f64::from(unit))),
    }
}
