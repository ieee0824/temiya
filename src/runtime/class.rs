//! Base class definitions. Derived construction and fields have separate gates.
use std::borrow::Cow;

use crate::ast::{
    Class, ClassElementKind, ExprKind, Function, FunctionBody, MethodKind, Stmt, StmtKind,
};

use super::{
    Environment, ExpressionEvaluator, FunctionKind, IntrinsicId, JsString, Object,
    PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult, Value, function::create_function,
};

pub(super) fn evaluate(
    class: &Class,
    outer: &ExpressionEvaluator,
    inferred_name: Option<JsString>,
) -> RuntimeResult<Object> {
    if class.super_class.is_some() {
        return Err(RuntimeError::internal_error(
            "derived class evaluation is not yet implemented",
        ));
    }
    // Never execute a partial class while silently discarding unsupported elements.
    for element in &class.elements {
        if !matches!(element.kind, ClassElementKind::Method { .. })
            || element
                .key
                .as_ref()
                .is_some_and(|key| matches!(key.kind, ExprKind::PrivateIdentifier(_)))
        {
            return Err(RuntimeError::internal_error(
                "class fields and private elements are not yet implemented",
            ));
        }
    }
    let binding = match class.name.as_ref().map(|name| &name.kind) {
        Some(ExprKind::Identifier(name)) => Some(JsString::from(name.as_str())),
        None => None,
        _ => return Err(RuntimeError::syntax_error("invalid class binding")),
    };
    let environment = Environment::declarative(outer.environment.clone());
    if let Some(name) = &binding {
        environment.create_immutable_binding(name.clone(), true)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        outer.realm.clone(),
        environment.clone(),
        outer.this_value.clone(),
        true,
    )
    .with_new_target(outer.new_target.clone())
    .with_function_cache(outer.function_cache.clone());
    let prototype = Object::new_with_prototype(outer.realm.intrinsic(IntrinsicId::ObjectPrototype));
    let explicit = class
        .elements
        .iter()
        .find_map(|element| match &element.kind {
            ClassElementKind::Method {
                function,
                kind: MethodKind::Constructor,
            } if !element.is_static => Some(function.as_ref()),
            _ => None,
        });
    let code = if let Some(function) = explicit {
        strict_code(function)
    } else {
        Cow::Owned(Function {
            span: class.span,
            name: None,
            parameters: vec![],
            body: FunctionBody::Block(Box::new(Stmt {
                span: class.span,
                kind: StmtKind::Block(vec![]),
            })),
            asynchronous: false,
            generator: false,
            strict: true,
        })
    };
    let constructor = create_function(
        &code,
        FunctionKind::Method(MethodKind::Constructor),
        &evaluator,
        Some(prototype.clone()),
    )?;
    constructor.define_own_property(
        "name".into(),
        PropertyDescriptor::data(
            Value::String(binding.clone().or(inferred_name).unwrap_or_default()),
            false,
            false,
            true,
        ),
    )?;
    constructor.define_own_property(
        "prototype".into(),
        PropertyDescriptor::data(Value::Object(prototype.clone()), false, false, false),
    )?;
    prototype.define_own_property(
        "constructor".into(),
        PropertyDescriptor::data(Value::Object(constructor.clone()), true, false, true),
    )?;
    for element in &class.elements {
        let ClassElementKind::Method { function, kind } = &element.kind else {
            unreachable!()
        };
        if *kind == MethodKind::Constructor && !element.is_static {
            continue;
        }
        let key = element
            .key
            .as_ref()
            .ok_or_else(|| RuntimeError::syntax_error("class method has no key"))?;
        let key = if !element.computed {
            if let ExprKind::Identifier(name) = &key.kind {
                PropertyKey::from(name.as_str())
            } else {
                super::object_literal::property_key(&outer.realm, evaluator.evaluate(key)?)?
            }
        } else {
            super::object_literal::property_key(&outer.realm, evaluator.evaluate(key)?)?
        };
        let home = if element.is_static {
            &constructor
        } else {
            &prototype
        };
        let code = strict_code(function);
        let method = create_function(
            &code,
            FunctionKind::Method(*kind),
            &evaluator,
            Some(home.clone()),
        )?;
        let name = match &key {
            PropertyKey::String(name) => name.code_units().to_vec(),
            PropertyKey::Symbol(symbol) => {
                symbol.description().map_or_else(Vec::new, |description| {
                    let mut name = vec![u16::from(b'[')];
                    name.extend_from_slice(description.code_units());
                    name.push(u16::from(b']'));
                    name
                })
            }
        };
        let prefix = match kind {
            MethodKind::Get => "get ",
            MethodKind::Set => "set ",
            _ => "",
        };
        let name = prefix.encode_utf16().chain(name).collect::<Vec<_>>();
        method.define_own_property(
            "name".into(),
            PropertyDescriptor::data(
                Value::String(JsString::from_code_units_slice(&name)),
                false,
                false,
                true,
            ),
        )?;
        let descriptor = match kind {
            MethodKind::Get => PropertyDescriptor {
                get: Some(Some(method)),
                enumerable: Some(false),
                configurable: Some(true),
                ..PropertyDescriptor::default()
            },
            MethodKind::Set => PropertyDescriptor {
                set: Some(Some(method)),
                enumerable: Some(false),
                configurable: Some(true),
                ..PropertyDescriptor::default()
            },
            _ => PropertyDescriptor::data(Value::Object(method), true, false, true),
        };
        if !home.define_own_property(key, descriptor)? {
            return Err(RuntimeError::type_error("cannot define class method"));
        }
    }
    if let Some(name) = binding {
        environment.initialize_binding(&name, Value::Object(constructor.clone()))?;
    }
    Ok(constructor)
}

fn strict_code(function: &Function) -> Cow<'_, Function> {
    if function.strict {
        Cow::Borrowed(function)
    } else {
        let mut code = function.clone();
        code.strict = true;
        Cow::Owned(code)
    }
}
