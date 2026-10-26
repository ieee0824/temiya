//! Fundamental Object, Function, and Reflect built-ins.

use std::{cell::RefCell, rc::Rc};

use super::{
    ArrayBuffer, ArrayObject, DataView, DateObject, HostHooks, IntrinsicId, IntrinsicRegistry,
    JsString, Json, MapObject, Math, NumberObject, Object, PropertyDescriptor, PropertyKey,
    ProxyObject, RegExp, RuntimeError, RuntimeResult, SetObject, StringObject, TypedArray, Value,
    WeakMapObject, WeakSetObject, parse_float, parse_int,
};

pub(super) fn initialize(
    intrinsics: &IntrinsicRegistry,
    host_hooks: HostHooks,
) -> RuntimeResult<()> {
    install_object(intrinsics)?;
    install_function(intrinsics)?;
    install_reflect(intrinsics)
        .and_then(|()| install_indexed(intrinsics))
        .and_then(|()| install_string_regexp(intrinsics))
        .and_then(|()| install_numeric(intrinsics))
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
        Ok(ProxyObject::new(target, handler).object())
    });
    method(&constructor, "revocable", |_, arguments| {
        let target = object_arg(arguments.first(), "Proxy target")?;
        let handler = object_arg(arguments.get(1), "Proxy handler")?;
        let proxy = ProxyObject::new(target, handler);
        let revocable = Object::new();
        revocable.create_data_property("proxy".into(), Value::Object(proxy.object()))?;
        revocable.create_data_property(
            "revoke".into(),
            Value::Object(Object::native_function(move |_, _| {
                proxy.revoke();
                Ok(Value::Undefined)
            })),
        )?;
        Ok(Value::Object(revocable))
    })
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
    method(&date, "now", move |_, _| Ok(Value::Number(host.now())))?;
    let date_prototype = intrinsics.get(IntrinsicId::DatePrototype);
    method(&date_prototype, "valueOf", |this, _| {
        Ok(Value::Number(date_receiver(this)?.value()))
    })?;
    method(&date_prototype, "toISOString", |this, _| {
        date_receiver(this)?.to_iso_string().map(Value::String)
    })?;

    let json = intrinsics.get(IntrinsicId::Json);
    method(&json, "parse", |_, arguments| {
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
    method(&json, "stringify", |_, arguments| {
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
    method(&map_prototype, "set", |this, arguments| {
        let map = map_receiver(this)?;
        map.set(value_arg(arguments.first()), value_arg(arguments.get(1)));
        Ok(Value::Object(map.object()))
    })?;
    method(&map_prototype, "get", |this, arguments| {
        Ok(map_receiver(this)?.get(&value_arg(arguments.first())))
    })?;
    method(&map_prototype, "has", |this, arguments| {
        Ok(Value::Boolean(
            map_receiver(this)?.has(&value_arg(arguments.first())),
        ))
    })?;
    method(&map_prototype, "delete", |this, arguments| {
        Ok(Value::Boolean(
            map_receiver(this)?.delete(&value_arg(arguments.first())),
        ))
    })?;

    let set = intrinsics.get(IntrinsicId::Set);
    let set_prototype = intrinsics.get(IntrinsicId::SetPrototype);
    set.clear_native_call();
    set.set_native_construct(move |_, _| {
        let object = SetObject::new().object();
        object.set_prototype(Some(set_prototype.clone()));
        Ok(object)
    });
    let set_prototype = intrinsics.get(IntrinsicId::SetPrototype);
    method(&set_prototype, "add", |this, arguments| {
        let set = set_receiver(this)?;
        set.add(value_arg(arguments.first()));
        Ok(Value::Object(set.object()))
    })?;
    method(&set_prototype, "has", |this, arguments| {
        Ok(Value::Boolean(
            set_receiver(this)?.has(&value_arg(arguments.first())),
        ))
    })?;
    method(&set_prototype, "delete", |this, arguments| {
        Ok(Value::Boolean(
            set_receiver(this)?.delete(&value_arg(arguments.first())),
        ))
    })?;

    let weak_map = intrinsics.get(IntrinsicId::WeakMap);
    let weak_map_prototype = intrinsics.get(IntrinsicId::WeakMapPrototype);
    weak_map.clear_native_call();
    weak_map.set_native_construct(move |_, _| {
        let object = WeakMapObject::new().object();
        object.set_prototype(Some(weak_map_prototype.clone()));
        Ok(object)
    });
    let weak_map_prototype = intrinsics.get(IntrinsicId::WeakMapPrototype);
    method(&weak_map_prototype, "set", |this, arguments| {
        let map = weak_map_receiver(this)?;
        map.set(&value_arg(arguments.first()), value_arg(arguments.get(1)))?;
        Ok(Value::Object(map.object()))
    })?;
    method(&weak_map_prototype, "get", |this, arguments| {
        let map = weak_map_receiver(this)?;
        match arguments.first() {
            Some(Value::Object(key)) => Ok(map.get(key)),
            _ => Ok(Value::Undefined),
        }
    })?;

    let weak_set = intrinsics.get(IntrinsicId::WeakSet);
    let weak_set_prototype = intrinsics.get(IntrinsicId::WeakSetPrototype);
    weak_set.clear_native_call();
    weak_set.set_native_construct(move |_, _| {
        let object = WeakSetObject::new().object();
        object.set_prototype(Some(weak_set_prototype.clone()));
        Ok(object)
    });
    let weak_set_prototype = intrinsics.get(IntrinsicId::WeakSetPrototype);
    method(&weak_set_prototype, "add", |this, arguments| {
        let set = weak_set_receiver(this)?;
        set.add(&value_arg(arguments.first()))?;
        Ok(Value::Object(set.object()))
    })
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

fn install_numeric(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let number = intrinsics.get(IntrinsicId::Number);
    let number_prototype = intrinsics.get(IntrinsicId::NumberPrototype);
    number.set_native_call(|_, arguments| match arguments.first() {
        None => Ok(Value::Number(0.0)),
        Some(value) => value.to_number().map(Value::Number),
    });
    number.set_native_construct(move |arguments, _| {
        let value = arguments.first().map_or(Ok(0.0), Value::to_number)?;
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
    ] {
        require_define(
            &number,
            name.into(),
            PropertyDescriptor::data(Value::Number(value), false, false, false),
        )?;
    }
    method(&number, "isNaN", |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_nan()),
        ))
    })?;
    method(&number, "isFinite", |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_finite()),
        ))
    })?;
    method(&number, "isInteger", |_, arguments| {
        Ok(Value::Boolean(
            matches!(arguments.first(), Some(Value::Number(value)) if value.is_finite() && value.fract() == 0.0),
        ))
    })?;
    method(&number, "parseFloat", |_, arguments| {
        Ok(Value::Number(parse_float(
            &value_arg(arguments.first()).to_js_string()?,
        )))
    })?;
    method(&number, "parseInt", |_, arguments| {
        let input = value_arg(arguments.first()).to_js_string()?;
        let radix = match arguments.get(1) {
            None | Some(Value::Undefined) => None,
            Some(value) => Some(value.to_number()? as u32),
        };
        Ok(Value::Number(parse_int(&input, radix)))
    })?;
    method(
        &intrinsics.get(IntrinsicId::NumberPrototype),
        "valueOf",
        |this, _| numeric_receiver(&this, false),
    )?;
    method(
        &intrinsics.get(IntrinsicId::NumberPrototype),
        "toFixed",
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
    bigint.set_native_call(|_, arguments| {
        value_arg(arguments.first()).to_bigint().map(Value::BigInt)
    });
    bigint.clear_native_construct();
    method(&bigint, "asUintN", |_, arguments| {
        let bits = bit_count(arguments.first())?;
        let value = value_arg(arguments.get(1)).to_bigint()?;
        Math::bigint_as_uint_n(bits, &value).map(Value::BigInt)
    })?;
    method(&bigint, "asIntN", |_, arguments| {
        let bits = bit_count(arguments.first())?;
        let value = value_arg(arguments.get(1)).to_bigint()?;
        Math::bigint_as_int_n(bits, &value).map(Value::BigInt)
    })?;

    let math = intrinsics.get(IntrinsicId::Math);
    unary_math(&math, "abs", f64::abs)?;
    unary_math(&math, "floor", f64::floor)?;
    unary_math(&math, "ceil", f64::ceil)?;
    unary_math(&math, "sqrt", f64::sqrt)?;
    unary_math(&math, "round", Math::round)?;
    method(&math, "pow", |_, arguments| {
        Ok(Value::Number(
            value_arg(arguments.first())
                .to_number()?
                .powf(value_arg(arguments.get(1)).to_number()?),
        ))
    })?;
    method(&math, "max", |_, arguments| {
        arguments
            .iter()
            .map(Value::to_number)
            .collect::<RuntimeResult<Vec<_>>>()
            .map(|values| Value::Number(Math::max(&values)))
    })?;
    method(&math, "min", |_, arguments| {
        arguments
            .iter()
            .map(Value::to_number)
            .collect::<RuntimeResult<Vec<_>>>()
            .map(|values| Value::Number(Math::min(&values)))
    })
}

fn unary_math(object: &Object, name: &str, operation: fn(f64) -> f64) -> RuntimeResult<()> {
    method(object, name, move |_, arguments| {
        Ok(Value::Number(operation(
            value_arg(arguments.first()).to_number()?,
        )))
    })
}

fn numeric_receiver(value: &Value, bigint: bool) -> RuntimeResult<Value> {
    match value {
        Value::Number(_) if !bigint => Ok(value.clone()),
        Value::BigInt(_) if bigint => Ok(value.clone()),
        Value::Object(object) => match object.0.state.borrow().numeric_data.as_ref() {
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

fn install_string_regexp(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let string = intrinsics.get(IntrinsicId::String);
    let string_prototype = intrinsics.get(IntrinsicId::StringPrototype);
    string.set_native_call(|_, arguments| {
        value_arg(arguments.first())
            .to_js_string()
            .map(Value::String)
    });
    string.set_native_construct(move |arguments, _| {
        let value = value_arg(arguments.first()).to_js_string()?;
        let object = StringObject::new(value).object();
        object.set_prototype(Some(string_prototype.clone()));
        Ok(object)
    });

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
        &intrinsics.get(IntrinsicId::RegExpPrototype),
        "exec",
        |this, arguments| {
            let object = object_value(&this, "RegExp receiver")?;
            let regexp = RegExp::from_object(object)
                .ok_or_else(|| RuntimeError::type_error("receiver is not a RegExp"))?;
            let input = value_arg(arguments.first()).to_js_string()?;
            let Some(found) = regexp.exec(&input)? else {
                return Ok(Value::Null);
            };
            let values = found
                .captures
                .into_iter()
                .map(|capture| capture.map_or(Value::Undefined, Value::String))
                .collect();
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
    method(&array, "isArray", |_, arguments| {
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
    method(&array_buffer, "isView", |_, arguments| {
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
        ArrayObject::new(*length as u32)?
    } else {
        let array = ArrayObject::new(arguments.len() as u32)?;
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

fn install_object(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let constructor = intrinsics.get(IntrinsicId::Object);
    let object_prototype = intrinsics.get(IntrinsicId::ObjectPrototype);
    method(&constructor, "create", move |_, arguments| {
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
    })?;
    method(&constructor, "defineProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.defineProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let descriptor = descriptor_arg(arguments.get(2))?;
        require_define(&object, key, descriptor)?;
        Ok(Value::Object(object))
    })?;
    method(&constructor, "getOwnPropertyDescriptor", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.getOwnPropertyDescriptor target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object
            .try_get_own_property(&key)?
            .map_or(Ok(Value::Undefined), |descriptor| {
                descriptor.to_object().map(Value::Object)
            })
    })?;
    method(&constructor, "getPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.getPrototypeOf target")?;
        Ok(object.try_prototype()?.map_or(Value::Null, Value::Object))
    })?;
    method(&constructor, "setPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.setPrototypeOf target")?;
        let prototype = nullable_object(arguments.get(1))?;
        if !object.try_set_prototype(prototype)? {
            return Err(RuntimeError::type_error(
                "object prototype cannot be changed",
            ));
        }
        Ok(Value::Object(object))
    })?;
    method(&constructor, "preventExtensions", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.preventExtensions target")?;
        object.try_prevent_extensions()?;
        Ok(Value::Object(object))
    })?;
    method(&constructor, "isExtensible", |_, arguments| {
        let object = object_arg(arguments.first(), "Object.isExtensible target")?;
        Ok(Value::Boolean(object.try_is_extensible()?))
    })?;
    method(&object_prototype, "hasOwnProperty", |this, arguments| {
        let object = object_value(&this, "Object.prototype.hasOwnProperty receiver")?;
        let key = value_arg(arguments.first()).to_property_key()?;
        Ok(Value::Boolean(object.get_own_property(&key).is_some()))
    })
}

fn install_function(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let prototype = intrinsics.get(IntrinsicId::FunctionPrototype);
    method(&prototype, "call", |this, arguments| {
        let target = callable_value(&this)?;
        target.call(
            arguments.first().cloned().unwrap_or(Value::Undefined),
            arguments.get(1..).unwrap_or_default(),
        )
    })?;
    method(&prototype, "apply", |this, arguments| {
        let target = callable_value(&this)?;
        let this_argument = arguments.first().cloned().unwrap_or(Value::Undefined);
        let list = argument_list(arguments.get(1))?;
        target.call(this_argument, &list)
    })?;
    method(&prototype, "bind", |this, arguments| {
        let target = callable_value(&this)?;
        let bound_this = arguments.first().cloned().unwrap_or(Value::Undefined);
        let prefix = arguments.get(1..).unwrap_or_default().to_vec();
        Ok(Value::Object(bound_function(target, bound_this, prefix)))
    })
}

fn install_reflect(intrinsics: &IntrinsicRegistry) -> RuntimeResult<()> {
    let reflect = intrinsics.get(IntrinsicId::Reflect);
    method(&reflect, "apply", |_, arguments| {
        let target = callable_arg(arguments.first())?;
        let this_argument = arguments.get(1).cloned().unwrap_or(Value::Undefined);
        target.call(this_argument, &argument_list(arguments.get(2))?)
    })?;
    method(&reflect, "construct", |_, arguments| {
        let target = constructor_arg(arguments.first())?;
        let list = argument_list(arguments.get(1))?;
        let new_target = match arguments.get(2) {
            None => target.clone(),
            Some(value) => constructor_arg(Some(value))?,
        };
        target.construct(&list, new_target).map(Value::Object)
    })?;
    method(&reflect, "defineProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.defineProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let descriptor = descriptor_arg(arguments.get(2))?;
        object
            .define_own_property(key, descriptor)
            .map(Value::Boolean)
    })?;
    method(&reflect, "getOwnPropertyDescriptor", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.getOwnPropertyDescriptor target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object
            .try_get_own_property(&key)?
            .map_or(Ok(Value::Undefined), |descriptor| {
                descriptor.to_object().map(Value::Object)
            })
    })?;
    method(&reflect, "get", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.get target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        let receiver = arguments
            .get(2)
            .cloned()
            .unwrap_or_else(|| Value::Object(object.clone()));
        object.get_with_receiver(&key, receiver)
    })?;
    method(&reflect, "set", |_, arguments| {
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
    method(&reflect, "has", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.has target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        object.has_property(&key).map(Value::Boolean)
    })?;
    method(&reflect, "deleteProperty", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.deleteProperty target")?;
        let key = value_arg(arguments.get(1)).to_property_key()?;
        Ok(Value::Boolean(object.try_delete(&key)?))
    })?;
    method(&reflect, "ownKeys", |_, arguments| {
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
    method(&reflect, "getPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.getPrototypeOf target")?;
        Ok(object.try_prototype()?.map_or(Value::Null, Value::Object))
    })?;
    method(&reflect, "setPrototypeOf", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.setPrototypeOf target")?;
        let prototype = nullable_object(arguments.get(1))?;
        Ok(Value::Boolean(object.try_set_prototype(prototype)?))
    })?;
    method(&reflect, "isExtensible", |_, arguments| {
        Ok(Value::Boolean(
            object_arg(arguments.first(), "Reflect.isExtensible target")?.try_is_extensible()?,
        ))
    })?;
    method(&reflect, "preventExtensions", |_, arguments| {
        let object = object_arg(arguments.first(), "Reflect.preventExtensions target")?;
        Ok(Value::Boolean(object.try_prevent_extensions()?))
    })
}

fn method(
    holder: &Object,
    name: &str,
    callback: impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static,
) -> RuntimeResult<()> {
    require_define(
        holder,
        name.into(),
        PropertyDescriptor::data(
            Value::Object(Object::native_function(callback)),
            true,
            false,
            true,
        ),
    )
}

fn bound_function(target: Object, bound_this: Value, prefix: Vec<Value>) -> Object {
    let call_target = target.clone();
    let call_prefix = prefix.clone();
    let call = move |_: Value, suffix: &[Value]| {
        let mut arguments = call_prefix.clone();
        arguments.extend_from_slice(suffix);
        call_target.call(bound_this.clone(), &arguments)
    };
    if target.is_constructor() {
        let slot = Rc::new(RefCell::new(None::<Object>));
        let construct_slot = slot.clone();
        let construct_target = target.clone();
        let construct_prefix = prefix;
        let function = Object::native_callable_constructor(call, move |suffix, new_target| {
            let mut arguments = construct_prefix.clone();
            arguments.extend_from_slice(suffix);
            let actual_new_target = if construct_slot.borrow().as_ref() == Some(&new_target) {
                construct_target.clone()
            } else {
                new_target
            };
            construct_target.construct(&arguments, actual_new_target)
        });
        *slot.borrow_mut() = Some(function.clone());
        function
    } else {
        Object::native_function(call)
    }
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
