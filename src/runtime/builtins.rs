//! Fundamental Object, Function, and Reflect built-ins.

use super::{
    ArrayBuffer, ArrayObject, DataView, DateObject, HostHooks, IntrinsicId, IntrinsicRegistry,
    JsString, Json, MapObject, Math, NumberObject, Object, PropertyDescriptor, PropertyKey,
    ProxyObject, RegExp, RuntimeError, RuntimeResult, SetObject, TypedArray, Value, WeakMapObject,
    WeakSetObject, parse_float, parse_int,
};

pub(super) fn initialize(
    intrinsics: &IntrinsicRegistry,
    host_hooks: HostHooks,
    symbols: &super::WellKnownSymbols,
) -> RuntimeResult<()> {
    install_object(intrinsics, symbols)?;
    install_function(intrinsics)?;
    install_reflect(intrinsics)
        .and_then(|()| install_indexed(intrinsics))
        .and_then(|()| install_regexp(intrinsics))
        .and_then(|()| install_numeric(intrinsics, symbols))
        .and_then(|()| install_collections(intrinsics))
        .and_then(|()| install_date_json(intrinsics, host_hooks))
        .and_then(|()| install_proxy(intrinsics))
}

fn install_proxy(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let constructor = intrinsics.get(IntrinsicId::Proxy);
    constructor.clear_native_call();
    constructor.set_native_construct(|arguments, _| {
        let target = object_arg(arguments.first(), "Proxy target")?;
        let handler = object_arg(arguments.get(1), "Proxy handler")?;
        Ok(ProxyObject::try_new(target, handler)?.object())
    });
    let prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    method(
        intrinsics,
        &constructor,
        "revocable",
        2,
        move |_, arguments| {
            let target = object_arg(arguments.first(), "Proxy target")?;
            let handler = object_arg(arguments.get(1), "Proxy handler")?;
            let proxy = ProxyObject::try_new(target, handler)?;
            let revocable = Object::new_for(&prototype);
            revocable.create_data_property("proxy".into(), Value::Object(proxy.object()))?;
            revocable.create_data_property(
                "revoke".into(),
                Value::Object(super::native_function::create(
                    &prototype,
                    "",
                    0,
                    move |_, _| {
                        proxy.revoke();
                        Ok(Value::Undefined)
                    },
                )?),
            )?;
            Ok(Value::Object(revocable))
        },
    )
}

fn install_date_json(intrinsics: &IntrinsicRegistry, host: HostHooks) -> RuntimeResult<()> {
    let date = intrinsics.get(IntrinsicId::Date);
    let date_prototype = intrinsics.get(IntrinsicId::DatePrototype);
    let call_host = host.clone();
    date.set_native_call(move |_, _| {
        DateObject::now(&call_host)
            .to_iso_string()
            .map(Value::String)
    });
    let construct_host = host.clone();
    date.set_native_construct(move |arguments, _| {
        let value = match arguments.first() {
            Some(value) => value.to_number()?,
            None => construct_host.now(),
        };
        let object = DateObject::new(value).object();
        object.set_prototype(Some(date_prototype.clone()));
        Ok(object)
    });
    method(intrinsics, &date, "now", 0, move |_, _| {
        Ok(Value::Number(host.now()))
    })?;
    let date_prototype = intrinsics.get(IntrinsicId::DatePrototype);
    method(intrinsics, &date_prototype, "valueOf", 0, |this, _| {
        Ok(Value::Number(date_receiver(this)?.value()))
    })?;
    method(intrinsics, &date_prototype, "toISOString", 0, |this, _| {
        date_receiver(this)?.to_iso_string().map(Value::String)
    })?;

    let json = intrinsics.get(IntrinsicId::Json);
    method(intrinsics, &json, "parse", 2, |_, arguments| {
        let source = value_arg(arguments.first()).to_js_string()?;
        let reviver = callable_arg(arguments.get(1)).ok();
        let mut callback = |key: &str, value: Value| match &reviver {
            Some(reviver) => reviver.call(
                Value::Undefined,
                &[Value::String(JsString::from(key)), value],
            ),
            None => Ok(value),
        };
        let callback = reviver
            .as_ref()
            .map(|_| &mut callback as &mut dyn FnMut(&str, Value) -> RuntimeResult<Value>);
        Json::parse(&source, callback)
    })?;
    method(intrinsics, &json, "stringify", 3, |_, arguments| {
        let replacer = callable_arg(arguments.get(1)).ok();
        let mut callback = |key: &str, value: Value| match &replacer {
            Some(replacer) => replacer.call(
                Value::Undefined,
                &[Value::String(JsString::from(key)), value],
            ),
            None => Ok(value),
        };
        let callback = replacer
            .as_ref()
            .map(|_| &mut callback as &mut dyn FnMut(&str, Value) -> RuntimeResult<Value>);
        Ok(Json::stringify(value_arg(arguments.first()), callback)?
            .map_or(Value::Undefined, Value::String))
    })
}

fn date_receiver(value: Value) -> RuntimeResult<DateObject> {
    let object = object_value(&value, "Date receiver")?;
    DateObject::from_object(object)
        .ok_or_else(|| RuntimeError::type_error("receiver is not a Date"))
}

fn install_collections(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let map = intrinsics.get(IntrinsicId::Map);
    let map_prototype = intrinsics.get(IntrinsicId::MapPrototype);
    map.clear_native_call();
    map.set_native_construct(move |_, _| {
        let object = MapObject::new().object();
        object.set_prototype(Some(map_prototype.clone()));
        Ok(object)
    });
    let map_prototype = intrinsics.get(IntrinsicId::MapPrototype);
    method(intrinsics, &map_prototype, "set", 2, |this, arguments| {
        let map = map_receiver(this)?;
        map.set(value_arg(arguments.first()), value_arg(arguments.get(1)));
        Ok(Value::Object(map.object()))
    })?;
    method(intrinsics, &map_prototype, "get", 1, |this, arguments| {
        Ok(map_receiver(this)?.get(&value_arg(arguments.first())))
    })?;
    method(intrinsics, &map_prototype, "has", 1, |this, arguments| {
        Ok(Value::Boolean(
            map_receiver(this)?.has(&value_arg(arguments.first())),
        ))
    })?;
    method(
        intrinsics,
        &map_prototype,
        "delete",
        1,
        |this, arguments| {
            Ok(Value::Boolean(
                map_receiver(this)?.delete(&value_arg(arguments.first())),
            ))
        },
    )?;

    let set = intrinsics.get(IntrinsicId::Set);
    let set_prototype = intrinsics.get(IntrinsicId::SetPrototype);
    set.clear_native_call();
    set.set_native_construct(move |_, _| {
        let object = SetObject::new().object();
        object.set_prototype(Some(set_prototype.clone()));
        Ok(object)
    });
    let set_prototype = intrinsics.get(IntrinsicId::SetPrototype);
    method(intrinsics, &set_prototype, "add", 1, |this, arguments| {
        let set = set_receiver(this)?;
        set.add(value_arg(arguments.first()));
        Ok(Value::Object(set.object()))
    })?;
    method(intrinsics, &set_prototype, "has", 1, |this, arguments| {
        Ok(Value::Boolean(
            set_receiver(this)?.has(&value_arg(arguments.first())),
        ))
    })?;
    method(
        intrinsics,
        &set_prototype,
        "delete",
        1,
        |this, arguments| {
            Ok(Value::Boolean(
                set_receiver(this)?.delete(&value_arg(arguments.first())),
            ))
        },
    )?;

    let weak_map = intrinsics.get(IntrinsicId::WeakMap);
    let weak_map_prototype = intrinsics.get(IntrinsicId::WeakMapPrototype);
    weak_map.clear_native_call();
    weak_map.set_native_construct(move |_, _| {
        let object = WeakMapObject::new().object();
        object.set_prototype(Some(weak_map_prototype.clone()));
        Ok(object)
    });
    let weak_map_prototype = intrinsics.get(IntrinsicId::WeakMapPrototype);
    method(
        intrinsics,
        &weak_map_prototype,
        "set",
        2,
        |this, arguments| {
            let map = weak_map_receiver(this)?;
            map.set(&value_arg(arguments.first()), value_arg(arguments.get(1)))?;
            Ok(Value::Object(map.object()))
        },
    )?;
    method(
        intrinsics,
        &weak_map_prototype,
        "get",
        1,
        |this, arguments| {
            let map = weak_map_receiver(this)?;
            match arguments.first() {
                Some(Value::Object(key)) => Ok(map.get(key)),
                _ => Ok(Value::Undefined),
            }
        },
    )?;

    let weak_set = intrinsics.get(IntrinsicId::WeakSet);
    let weak_set_prototype = intrinsics.get(IntrinsicId::WeakSetPrototype);
    weak_set.clear_native_call();
    weak_set.set_native_construct(move |_, _| {
        let object = WeakSetObject::new().object();
        object.set_prototype(Some(weak_set_prototype.clone()));
        Ok(object)
    });
    let weak_set_prototype = intrinsics.get(IntrinsicId::WeakSetPrototype);
    method(
        intrinsics,
        &weak_set_prototype,
        "add",
        1,
        |this, arguments| {
            let set = weak_set_receiver(this)?;
            set.add(&value_arg(arguments.first()))?;
            Ok(Value::Object(set.object()))
        },
    )
}

fn map_receiver(value: Value) -> RuntimeResult<MapObject> {
    let object = object_value(&value, "Map receiver")?;
    MapObject::from_object(object).ok_or_else(|| RuntimeError::type_error("receiver is not a Map"))
}
fn set_receiver(value: Value) -> RuntimeResult<SetObject> {
    let object = object_value(&value, "Set receiver")?;
    SetObject::from_object(object).ok_or_else(|| RuntimeError::type_error("receiver is not a Set"))
}
fn weak_map_receiver(value: Value) -> RuntimeResult<WeakMapObject> {
    let object = object_value(&value, "WeakMap receiver")?;
    WeakMapObject::from_object(object)
        .ok_or_else(|| RuntimeError::type_error("receiver is not a WeakMap"))
}
fn weak_set_receiver(value: Value) -> RuntimeResult<WeakSetObject> {
    let object = object_value(&value, "WeakSet receiver")?;
    WeakSetObject::from_object(object)
        .ok_or_else(|| RuntimeError::type_error("receiver is not a WeakSet"))
}

fn install_numeric(
    intrinsics: &IntrinsicRegistry,
    symbols: &super::WellKnownSymbols,
) -> RuntimeResult<()> {
    let number = intrinsics.get(IntrinsicId::Number);
    let number_prototype = intrinsics.get(IntrinsicId::NumberPrototype);
    let number_key: PropertyKey = symbols.get(super::WellKnownSymbol::ToPrimitive).into();
    let construct_number_key = number_key.clone();
    number.set_native_call(move |_, arguments| {
        number_argument(arguments.first(), &number_key).map(Value::Number)
    });
    number.set_native_construct(move |arguments, _| {
        let value = number_argument(arguments.first(), &construct_number_key)?;
        let object = NumberObject::new(value).object();
        object.set_prototype(Some(number_prototype.clone()));
        Ok(object)
    });
    for (name, value) in [
        ("NaN", f64::NAN),
        ("POSITIVE_INFINITY", f64::INFINITY),
        ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
        ("MAX_VALUE", f64::MAX),
        ("MIN_VALUE", f64::from_bits(1)),
        ("EPSILON", f64::EPSILON),
        ("MAX_SAFE_INTEGER", 9_007_199_254_740_991.0),
        ("MIN_SAFE_INTEGER", -9_007_199_254_740_991.0),
    ] {
        require_define(
            &number,
            name.into(),
            PropertyDescriptor::data(Value::Number(value), false, false, false),
        )?;
    }
    method(intrinsics, &number, "isNaN", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_nan()),
        ))
    })?;
    method(intrinsics, &number, "isFinite", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_finite()),
        ))
    })?;
    method(intrinsics, &number, "isInteger", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_finite() && value.fract() == 0.0),
        ))
    })?;
    numeric_method(intrinsics, &number, "isSafeInteger", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0),
        ))
    })?;
    method(intrinsics, &number, "parseFloat", 1, |_, arguments| {
        Ok(Value::Number(parse_float(
            &value_arg(arguments.first()).to_js_string()?,
        )))
    })?;
    method(intrinsics, &number, "parseInt", 2, |_, arguments| {
        let input = value_arg(arguments.first()).to_js_string()?;
        let radix = match arguments.get(1) {
            None | Some(Value::Undefined) => None,
            Some(value) => Some(value.to_number()? as u32),
        };
        Ok(Value::Number(parse_int(&input, radix)))
    })?;
    method(
        intrinsics,
        &intrinsics.get(IntrinsicId::NumberPrototype),
        "valueOf",
        0,
        |this, _| numeric_receiver(&this, false),
    )?;
    method(
        intrinsics,
        &intrinsics.get(IntrinsicId::NumberPrototype),
        "toFixed",
        1,
        |this, arguments| {
            let Value::Number(value) = numeric_receiver(&this, false)? else {
                unreachable!()
            };
            let digits = length_argument(arguments.first())?;
            if digits > 100 {
                return Err(RuntimeError::range_error(
                    "fraction digits are out of range",
                ));
            }
            Ok(Value::String(JsString::from(format!("{value:.digits$}"))))
        },
    )?;

    let bigint = intrinsics.get(IntrinsicId::BigInt);
    let bigint_key: PropertyKey = symbols.get(super::WellKnownSymbol::ToPrimitive).into();
    bigint.set_native_call(move |_, arguments| {
        let value = super::coercion::primitive(&value_arg(arguments.first()), &bigint_key, false)?;
        match value {
            Value::Number(number) => super::numeric::number_to_bigint(number).map(Value::BigInt),
            value => value.to_bigint().map(Value::BigInt),
        }
    });
    bigint.clear_native_construct();
    method(intrinsics, &bigint, "asUintN", 2, |_, arguments| {
        let bits = bit_count(arguments.first())?;
        let value = value_arg(arguments.get(1)).to_bigint()?;
        Math::bigint_as_uint_n(bits, &value).map(Value::BigInt)
    })?;
    method(intrinsics, &bigint, "asIntN", 2, |_, arguments| {
        let bits = bit_count(arguments.first())?;
        let value = value_arg(arguments.get(1)).to_bigint()?;
        Math::bigint_as_int_n(bits, &value).map(Value::BigInt)
    })?;

    let prototype = intrinsics.get(IntrinsicId::BigIntPrototype);
    numeric_method(intrinsics, &prototype, "valueOf", 0, |this, _| {
        numeric_receiver(&this, true)
    })?;
    let key: PropertyKey = symbols.get(super::WellKnownSymbol::ToPrimitive).into();
    numeric_method(
        intrinsics,
        &prototype,
        "toString",
        0,
        move |this, arguments| {
            let Value::BigInt(value) = numeric_receiver(&this, true)? else {
                unreachable!()
            };
            let radix = match arguments.first() {
                None | Some(Value::Undefined) => 10,
                Some(value) => {
                    let number = super::coercion::primitive(value, &key, false)?.to_number()?;
                    let integer = if number.is_nan() { 0.0 } else { number.trunc() };
                    if !(2.0..=36.0).contains(&integer) {
                        return Err(RuntimeError::range_error(
                            "BigInt radix must be between 2 and 36",
                        ));
                    }
                    integer as u32
                }
            };
            Ok(Value::String(value.to_str_radix(radix).into()))
        },
    )?;
    prototype.define_own_property(
        symbols.get(super::WellKnownSymbol::ToStringTag).into(),
        PropertyDescriptor::data(Value::String("BigInt".into()), false, false, true),
    )?;

    let math = intrinsics.get(IntrinsicId::Math);
    unary_math(intrinsics, &math, "abs", f64::abs)?;
    unary_math(intrinsics, &math, "floor", f64::floor)?;
    unary_math(intrinsics, &math, "ceil", f64::ceil)?;
    unary_math(intrinsics, &math, "sqrt", f64::sqrt)?;
    unary_math(intrinsics, &math, "round", Math::round)?;
    method(intrinsics, &math, "pow", 2, |_, arguments| {
        Ok(Value::Number(
            value_arg(arguments.first())
                .to_number()?
                .powf(value_arg(arguments.get(1)).to_number()?),
        ))
    })?;
    method(intrinsics, &math, "max", 2, |_, arguments| {
        arguments
            .iter()
            .map(Value::to_number)
            .collect::<RuntimeResult<Vec<_>>>()
            .map(|values| Value::Number(Math::max(&values)))
    })?;
    method(intrinsics, &math, "min", 2, |_, arguments| {
        arguments
            .iter()
            .map(Value::to_number)
            .collect::<RuntimeResult<Vec<_>>>()
            .map(|values| Value::Number(Math::min(&values)))
    })
}

fn unary_math(
    intrinsics: &IntrinsicRegistry,
    object: &Object,
    name: &str,
    operation: fn(f64) -> f64,
) -> RuntimeResult<()> {
    method(intrinsics, object, name, 1, move |_, arguments| {
        Ok(Value::Number(operation(
            value_arg(arguments.first()).to_number()?,
        )))
    })
}

fn numeric_receiver(value: &Value, bigint: bool) -> RuntimeResult<Value> {
    match value {
        Value::Number(_) if !bigint => Ok(value.clone()),
        Value::BigInt(_) if bigint => Ok(value.clone()),
        Value::Object(object) => match object.0.state.borrow().primitive_data.as_ref() {
            Some(value @ Value::Number(_)) if !bigint => Ok(value.clone()),
            Some(value @ Value::BigInt(_)) if bigint => Ok(value.clone()),
            _ => Err(RuntimeError::type_error("incompatible numeric receiver")),
        },
        _ => Err(RuntimeError::type_error("incompatible numeric receiver")),
    }
}

fn bit_count(value: Option<&Value>) -> RuntimeResult<u64> {
    let value = value_arg(value).to_number()?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 1_000_000.0 {
        return Err(RuntimeError::range_error("bit count is out of range"));
    }
    Ok(value as u64)
}

fn install_regexp(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let regexp = intrinsics.get(IntrinsicId::RegExp);
    let regexp_prototype = intrinsics.get(IntrinsicId::RegExpPrototype);
    let create = move |arguments: &[Value]| {
        let pattern = match arguments.first() {
            Some(Value::Object(object)) => RegExp::from_object(object.clone()).map_or_else(
                || value_arg(arguments.first()).to_js_string(),
                |value| Ok(value.source()),
            )?,
            value => value_arg(value).to_js_string()?,
        };
        let flags = match arguments.get(1) {
            None | Some(Value::Undefined) => String::new(),
            Some(value) => value.to_js_string()?.to_string_lossy(),
        };
        let object = RegExp::new(pattern, &flags)?.object();
        object.set_prototype(Some(regexp_prototype.clone()));
        Ok(object)
    };
    let create = std::rc::Rc::new(create);
    let call_create = create.clone();
    regexp.set_native_call(move |_, arguments| call_create(arguments).map(Value::Object));
    regexp.set_native_construct(move |arguments, _| create(arguments));
    method(
        intrinsics,
        &intrinsics.get(IntrinsicId::RegExpPrototype),
        "exec",
        1,
        |this, arguments| {
            let object = object_value(&this, "RegExp receiver")?;
            let regexp = RegExp::from_object(object)
                .ok_or_else(|| RuntimeError::type_error("receiver is not a RegExp"))?;
            let input = value_arg(arguments.first()).to_js_string()?;
            let Some(values) = regexp.exec_values(&input)? else {
                return Ok(Value::Null);
            };
            array_value(values)
        },
    )
}

fn install_indexed(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let array = intrinsics.get(IntrinsicId::Array);
    let array_prototype = intrinsics.get(IntrinsicId::ArrayPrototype);
    let prototype_for_call = array_prototype.clone();
    array.set_native_call(move |_, arguments| {
        create_array(arguments, &prototype_for_call).map(Value::Object)
    });
    let prototype_for_construct = array_prototype;
    array.set_native_construct(move |arguments, _| {
        create_array(arguments, &prototype_for_construct)
    });
    method(intrinsics, &array, "isArray", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Object(object)) if ArrayObject::from_object(object.clone()).is_some()),
        ))
    })?;

    let array_buffer = intrinsics.get(IntrinsicId::ArrayBuffer);
    let buffer_prototype = intrinsics.get(IntrinsicId::ArrayBufferPrototype);
    array_buffer.set_native_construct(move |arguments, _| {
        let length = length_argument(arguments.first())?;
        let buffer = ArrayBuffer::new(length)?;
        buffer
            .object()
            .set_prototype(Some(buffer_prototype.clone()));
        Ok(buffer.object())
    });
    method(intrinsics, &array_buffer, "isView", 1, |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Object(object)) if TypedArray::from_object(object.clone()).is_some()),
        ))
    })?;

    let typed_array = intrinsics.get(IntrinsicId::TypedArray);
    let typed_prototype = intrinsics.get(IntrinsicId::TypedArrayPrototype);
    typed_array.set_native_construct(move |arguments, _| {
        let view = match arguments.first() {
            Some(Value::Object(object)) => {
                let buffer = ArrayBuffer::from_object(object.clone()).ok_or_else(|| {
                    RuntimeError::type_error("TypedArray requires an ArrayBuffer or length")
                })?;
                let offset = length_argument(arguments.get(1))?;
                let length = match arguments.get(2) {
                    Some(value) => length_argument(Some(value))?,
                    None => buffer
                        .byte_length()
                        .checked_sub(offset)
                        .ok_or_else(|| RuntimeError::range_error("view offset is out of bounds"))?,
                };
                TypedArray::new(&buffer, offset, length)?
            }
            value => {
                let buffer = ArrayBuffer::new(length_argument(value)?)?;
                TypedArray::new(&buffer, 0, buffer.byte_length())?
            }
        };
        view.object().set_prototype(Some(typed_prototype.clone()));
        Ok(view.object())
    });

    let data_view = intrinsics.get(IntrinsicId::DataView);
    let data_prototype = intrinsics.get(IntrinsicId::DataViewPrototype);
    data_view.set_native_construct(move |arguments, _| {
        let buffer = object_arg(arguments.first(), "DataView buffer")?;
        let buffer = ArrayBuffer::from_object(buffer)
            .ok_or_else(|| RuntimeError::type_error("DataView requires an ArrayBuffer"))?;
        let offset = length_argument(arguments.get(1))?;
        let length = match arguments.get(2) {
            Some(value) => length_argument(Some(value))?,
            None => buffer
                .byte_length()
                .checked_sub(offset)
                .ok_or_else(|| RuntimeError::range_error("view offset is out of bounds"))?,
        };
        let view = DataView::new(&buffer, offset, length)?;
        let object = view.object();
        object.set_prototype(Some(data_prototype.clone()));
        Ok(object)
    });
    Ok(())
}

fn create_array(arguments: &[Value], prototype: &Object) -> RuntimeResult<Object> {
    let array = if let [Value::Number(length)] = arguments {
        if !length.is_finite()
            || *length < 0.0
            || length.fract() != 0.0
            || *length > f64::from(u32::MAX)
        {
            return Err(RuntimeError::range_error("invalid array length"));
        }
        ArrayObject::new_for(prototype, *length as u32)?
    } else {
        let array = ArrayObject::new_for(prototype, arguments.len() as u32)?;
        for (index, value) in arguments.iter().cloned().enumerate() {
            array
                .object()
                .create_data_property(JsString::from(index.to_string()).into(), value)?;
        }
        array
    };
    array.object().set_prototype(Some(prototype.clone()));
    Ok(array.object())
}

fn length_argument(value: Option<&Value>) -> RuntimeResult<usize> {
    let number = value_arg(value).to_number()?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > 100_000_000.0 {
        return Err(RuntimeError::range_error("length is out of range"));
    }
    Ok(number as usize)
}

fn install_object(
    intrinsics: &IntrinsicRegistry,
    symbols: &super::WellKnownSymbols,
) -> RuntimeResult<()> {
    let constructor = intrinsics.get(IntrinsicId::Object);
    let object_prototype = intrinsics.get(IntrinsicId::ObjectPrototype);
    for (name, symbols) in [
        ("getOwnPropertyNames", false),
        ("getOwnPropertySymbols", true),
    ] {
        let prototype = intrinsics.get(IntrinsicId::ArrayPrototype);
        method(intrinsics, &constructor, name, 1, move |_, arguments| {
            let source = property_source(value_arg(arguments.first()))?;
            let keys: Vec<Value> = source
                .try_own_property_keys()?
                .into_iter()
                .filter_map(|key| match key {
                    PropertyKey::String(name) if !symbols => Some(Value::String(name)),
                    PropertyKey::Symbol(symbol) if symbols => Some(Value::Symbol(symbol)),
                    _ => None,
                })
                .collect();
            let length = u32::try_from(keys.len())
                .map_err(|_| RuntimeError::range_error("too many own property keys"))?;
            let array = ArrayObject::new_for(&prototype, length)?.object();
            array.initialize_prototype(Some(prototype.clone()));
            for (index, key) in keys.into_iter().enumerate() {
                array.create_data_property(JsString::from(index.to_string()).into(), key)?;
            }
            Ok(Value::Object(array))
        })?;
    }
    method(
        intrinsics,
        &constructor,
        "create",
        2,
        move |_, arguments| {
            let prototype = nullable_object(arguments.first())?;
            let object = Object::new();
            object.set_prototype(prototype);
            match arguments.get(1) {
                None | Some(Value::Undefined) => {}
                Some(Value::Object(descriptors)) => define_properties(&object, descriptors)?,
                Some(_) => {
                    return Err(RuntimeError::type_error(
                        "Object.create descriptors must be an object",
                    ));
                }
            }
            Ok(Value::Object(object))
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "defineProperty",
        3,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.defineProperty target")?;
            let key = value_arg(arguments.get(1)).to_property_key()?;
            let descriptor = descriptor_arg(arguments.get(2))?;
            require_define(&object, key, descriptor)?;
            Ok(Value::Object(object))
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "getOwnPropertyDescriptor",
        2,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.getOwnPropertyDescriptor target")?;
            let key = value_arg(arguments.get(1)).to_property_key()?;
            object
                .try_get_own_property(&key)?
                .map_or(Ok(Value::Undefined), |descriptor| {
                    descriptor.to_object().map(Value::Object)
                })
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "getPrototypeOf",
        1,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.getPrototypeOf target")?;
            Ok(object.try_prototype()?.map_or(Value::Null, Value::Object))
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "setPrototypeOf",
        2,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.setPrototypeOf target")?;
            let prototype = nullable_object(arguments.get(1))?;
            if !object.try_set_prototype(prototype)? {
                return Err(RuntimeError::type_error(
                    "object prototype cannot be changed",
                ));
            }
            Ok(Value::Object(object))
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "preventExtensions",
        1,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.preventExtensions target")?;
            object.try_prevent_extensions()?;
            Ok(Value::Object(object))
        },
    )?;
    method(
        intrinsics,
        &constructor,
        "isExtensible",
        1,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Object.isExtensible target")?;
            Ok(Value::Boolean(object.try_is_extensible()?))
        },
    )?;
    for (name, enumerable) in [("hasOwnProperty", false), ("propertyIsEnumerable", true)] {
        let to_primitive: PropertyKey = symbols.get(super::WellKnownSymbol::ToPrimitive).into();
        method(
            intrinsics,
            &object_prototype,
            name,
            1,
            move |this, arguments| {
                // Key conversion precedes ToObject(this), including a null receiver.
                let key =
                    super::coercion::primitive(&value_arg(arguments.first()), &to_primitive, true)?
                        .to_property_key()?;
                let object = property_source(this)?;
                let descriptor = object.try_get_own_property(&key)?;
                Ok(Value::Boolean(descriptor.is_some_and(|descriptor| {
                    !enumerable || descriptor.enumerable == Some(true)
                })))
            },
        )?;
    }
    Ok(())
}

fn install_function(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    method(intrinsics, &prototype, "call", 1, |this, arguments| {
        let target = callable_value(&this)?;
        target.call(
            arguments.first().cloned().unwrap_or(Value::Undefined),
            arguments.get(1..).unwrap_or_default(),
        )
    })?;
    method(intrinsics, &prototype, "apply", 2, |this, arguments| {
        let target = callable_value(&this)?;
        let this_argument = arguments.first().cloned().unwrap_or(Value::Undefined);
        let list = argument_list(arguments.get(1))?;
        target.call(this_argument, &list)
    })?;
    method(intrinsics, &prototype, "bind", 1, |this, arguments| {
        let target = callable_value(&this)?;
        let bound_this = arguments.first().cloned().unwrap_or(Value::Undefined);
        let prefix = arguments.get(1..).unwrap_or_default().to_vec();
        bound_function(target, bound_this, prefix).map(Value::Object)
    })
}

fn install_reflect(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let reflect = intrinsics.get(IntrinsicId::Reflect);
    method(intrinsics, &reflect, "apply", 3, |_, arguments| {
        let target = callable_arg(arguments.first())?;
        let this_argument = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        target.call(this_argument, &argument_list(arguments.get(2))?)
    })?;
    method(intrinsics, &reflect, "construct", 2, |_, arguments| {
        let target = constructor_arg(arguments.first())?;
        let list = argument_list(arguments.get(1))?;
        let new_target = match arguments.get(2) {
            None => target.clone(),
            Some(value) => constructor_arg(Some(value))?,
        };
        target.construct(&list, new_target).map(Value::Object)
    })?;
    method(intrinsics, &reflect, "defineProperty", 3, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.defineProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let descriptor = descriptor_arg(arguments.get(2))?;
        object
            .define_own_property(key, descriptor)
            .map(Value::Boolean)
    })?;
    method(
        intrinsics,
        &reflect,
        "getOwnPropertyDescriptor",
        2,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Reflect.getOwnPropertyDescriptor target")?;
            let key = value_arg(arguments.get(1)).to_property_key()?;
            object
                .try_get_own_property(&key)?
                .map_or(Ok(Value::Undefined), |descriptor| {
                    descriptor.to_object().map(Value::Object)
                })
        },
    )?;
    method(intrinsics, &reflect, "get", 2, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.get target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let receiver = arguments
            .get(2)
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()));
        object.get_with_receiver(&key, receiver)
    })?;
    method(intrinsics, &reflect, "set", 3, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.set target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let value = value_arg(arguments.get(2));
        let receiver = arguments
            .get(3)
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()));
        object
            .set_with_receiver(&key, value, receiver)
            .map(Value::Boolean)
    })?;
    method(intrinsics, &reflect, "has", 2, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.has target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object.has_property(&key).map(Value::Boolean)
    })?;
    method(intrinsics, &reflect, "deleteProperty", 2, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.deleteProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        Ok(Value::Boolean(object.try_delete(&key)?))
    })?;
    method(intrinsics, &reflect, "ownKeys", 1, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.ownKeys target")?;
        let values = object
            .try_own_property_keys()?
            .into_iter()
            .map(|key| match key {
                PropertyKey::String(value) => Value::String(value),
                PropertyKey::Symbol(value) => Value::Symbol(value),
            })
            .collect::<Vec<_>>();
        array_value(values)
    })?;
    method(intrinsics, &reflect, "getPrototypeOf", 1, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.getPrototypeOf target")?;
        Ok(object.try_prototype()?.map_or(Value::Null, Value::Object))
    })?;
    method(intrinsics, &reflect, "setPrototypeOf", 2, |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.setPrototypeOf target")?;
        let prototype = nullable_object(arguments.get(1))?;
        Ok(Value::Boolean(object.try_set_prototype(prototype)?))
    })?;
    method(intrinsics, &reflect, "isExtensible", 1, |_, arguments| {
        Ok(Value::Boolean(
            object_arg(arguments.first(), "Reflect.isExtensible target")?.try_is_extensible()?,
        ))
    })?;
    method(
        intrinsics,
        &reflect,
        "preventExtensions",
        1,
        |_, arguments| {
            let object = object_arg(arguments.first(), "Reflect.preventExtensions target")?;
            Ok(Value::Boolean(object.try_prevent_extensions()?))
        },
    )
}

fn method(
    intrinsics: &IntrinsicRegistry,
    holder: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    super::array_methods::method(
        holder,
        &intrinsics.get(IntrinsicId::FunctionPrototype),
        name,
        length,
        callback,
    )
}

fn bound_function(target: Object, bound_this: Value, prefix: Vec<Value>) -> RuntimeResult<Object> {
    let prototype = target.try_prototype()?;
    let function = Object::new_bound(&target, &bound_this, &prefix)?;
    function.initialize_prototype(prototype);
    Ok(function)
}

fn define_properties(target: &Object, descriptors: &Object) -> RuntimeResult<()> {
    for key in descriptors.own_property_keys() {
        let Value::Object(descriptor) = descriptors.get(&key)? else {
            return Err(RuntimeError::type_error(
                "property descriptor must be an object",
            ));
        };
        require_define(target, key, PropertyDescriptor::from_object(&descriptor)?)?;
    }
    Ok(())
}

fn descriptor_arg(value: Option<&Value>) -> RuntimeResult<PropertyDescriptor> {
    let object = object_arg(value, "property descriptor")?;
    PropertyDescriptor::from_object(&object)
}

fn require_define(
    object: &Object,
    key: PropertyKey,
    descriptor: PropertyDescriptor,
) -> RuntimeResult<()> {
    if object.define_own_property(key, descriptor)? {
        Ok(())
    } else {
        Err(RuntimeError::type_error("property cannot be defined"))
    }
}

fn value_arg(value: Option<&Value>) -> Value {
    value.cloned().unwrap_or(Value::Undefined)
}

fn object_arg(value: Option<&Value>, name: &str) -> RuntimeResult<Object> {
    object_value(&value_arg(value), name)
}

fn object_value(value: &Value, name: &str) -> RuntimeResult<Object> {
    match value {
        Value::Object(object) => Ok(object.clone()),
        _ => Err(RuntimeError::type_error(format!(
            "{name} must be an object"
        ))),
    }
}

fn callable_arg(value: Option<&Value>) -> RuntimeResult<Object> {
    callable_value(&value_arg(value))
}

fn callable_value(value: &Value) -> RuntimeResult<Object> {
    let object = object_value(value, "function target")?;
    if object.is_callable() {
        Ok(object)
    } else {
        Err(RuntimeError::type_error("function target is not callable"))
    }
}

fn constructor_arg(value: Option<&Value>) -> RuntimeResult<Object> {
    let object = object_arg(value, "constructor target")?;
    if object.is_constructor() {
        Ok(object)
    } else {
        Err(RuntimeError::type_error("target is not a constructor"))
    }
}

fn nullable_object(value: Option<&Value>) -> RuntimeResult<Option<Object>> {
    match value_arg(value) {
        Value::Null => Ok(None),
        Value::Object(object) => Ok(Some(object)),
        _ => Err(RuntimeError::type_error(
            "prototype must be an object or null",
        )),
    }
}

fn argument_list(value: Option<&Value>) -> RuntimeResult<Vec<Value>> {
    match value_arg(value) {
        Value::Undefined | Value::Null => Ok(Vec::new()),
        Value::Object(object) => {
            let length = object.get(&"length".into())?.to_number()?;
            if !length.is_finite() || length < 0.0 || length > 100_000.0 {
                return Err(RuntimeError::range_error("argument list length is invalid"));
            }
            (0..length.floor() as u32)
                .map(|index| object.get(&JsString::from(index.to_string()).into()))
                .collect()
        }
        _ => Err(RuntimeError::type_error("argument list must be an object")),
    }
}

fn array_value(values: Vec<Value>) -> RuntimeResult<Value> {
    let array = Object::new();
    let length = values.len();
    for (index, value) in values.into_iter().enumerate() {
        array.create_data_property(JsString::from(index.to_string()).into(), value)?;
    }
    array.define_own_property(
        "length".into(),
        PropertyDescriptor::data(Value::Number(length as f64), true, false, false),
    )?;
    Ok(Value::Object(array))
}

// These new numeric methods expose ordinary builtin metadata and Function.prototype.
fn numeric_method(
    intrinsics: &IntrinsicRegistry,
    holder: &Object,
    name: &str,
    length: u32,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    method(intrinsics, holder, name, length, callback)
}

fn number_argument(value: Option<&Value>, key: &PropertyKey) -> RuntimeResult<f64> {
    let Some(value) = value else { return Ok(0.0) };
    match super::coercion::primitive(value, key, false)? {
        Value::BigInt(value) => Ok(value
            .to_string()
            .parse::<f64>()
            .expect("BigInt decimal is a valid Number spelling")),
        value => value.to_number(),
    }
}

// Primitive wrapper identity/prototype cannot be observed by an own-descriptor
// query; only string wrappers contribute own properties.
fn property_source(value: Value) -> RuntimeResult<Object> {
    match value {
        Value::Object(object) => Ok(object),
        Value::String(value) => Ok(super::StringObject::new(value).object()),
        Value::Null | Value::Undefined => {
            Err(RuntimeError::type_error("cannot inspect null or undefined"))
        }
        _ => Ok(Object::new()),
    }
}

pub(super) fn install_has_instance(
    intrinsics: &IntrinsicRegistry,
    symbols: &super::WellKnownSymbols,
) -> RuntimeResult<()> {
    let prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    let key: PropertyKey = symbols.get(super::WellKnownSymbol::HasInstance).into();
    let call_key = key.clone();
    let method = Object::native_function_for(&prototype, move |this, arguments| {
        super::operators::ordinary_has_instance(this, value_arg(arguments.first()), &call_key)
            .map(Value::Boolean)
    });
    method.set_prototype(Some(prototype.clone()));
    method.define_own_property(
        "name".into(),
        PropertyDescriptor::data(
            Value::String("[Symbol.hasInstance]".into()),
            false,
            false,
            true,
        ),
    )?;
    method.define_own_property(
        "length".into(),
        PropertyDescriptor::data(Value::Number(1.0), false, false, true),
    )?;
    prototype.define_own_property(
        key,
        PropertyDescriptor::data(Value::Object(method), false, false, false),
    )?;
    Ok(())
}
