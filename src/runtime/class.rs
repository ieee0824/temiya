//! Class definitions and constructor inheritance. Fields have separate gates.
use std::borrow::Cow;

use crate::ast::{
    Class, ClassElementKind, ExprKind, Function, FunctionBody, MethodKind, Stmt, StmtKind,
};

use super::{
    Environment, ExpressionEvaluator, FunctionKind, IntrinsicId, JsString, Object,
    PropertyDescriptor, PropertyKey, RuntimeError, RuntimeResult, Value,
    function::{ConstructorKind, create_function_with_constructor},
};

pub(super) fn evaluate(
    class: &Class,
    outer: &ExpressionEvaluator,
    inferred_name: Option<JsString>,
) -> RuntimeResult<Object> {
    enum StaticInitializer<'a> {
        Field(PropertyKey, Option<Object>),
        PrivateField(super::private_fields::Name, Option<Object>),
        DirectField(PropertyKey, &'a crate::ast::Expr),
        Block(Object),
    }
    let binding = match class.name.as_ref().map(|name| &name.kind) {
        Some(ExprKind::Identifier(name)) => Some(JsString::from(name.as_str())),
        None => None,
        _ => return Err(RuntimeError::syntax_error("invalid class binding")),
    };
    // An anonymous class introduces no lexical name. Its empty declarative
    // record adds no binding or this behavior; methods can capture the outer
    // environment directly. Named classes still need their own TDZ/immutable
    // binding, and the evaluator below remains strict in both cases.
    let has_private = class.elements.iter().any(|element| {
        element
            .key
            .as_ref()
            .is_some_and(|key| matches!(key.kind, ExprKind::PrivateIdentifier(_)))
    });
    let environment = if binding.is_none() && !has_private {
        outer.environment.clone()
    } else {
        Environment::declarative(outer.environment.clone())
    };
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
    .with_this_environment(outer.this_environment.clone())
    .with_function_cache(outer.function_cache.clone());
    let (prototype_parent, constructor_parent) = if let Some(heritage) = &class.super_class {
        match evaluator.evaluate(heritage)? {
            Value::Null => (None, outer.realm.intrinsic(IntrinsicId::FunctionPrototype)),
            Value::Object(parent) if parent.is_constructor() => {
                let prototype = match parent.get(&PropertyKey::from("prototype"))? {
                    Value::Object(object) => Some(object),
                    Value::Null => None,
                    _ => {
                        return Err(RuntimeError::type_error(
                            "superclass prototype is not an object or null",
                        ));
                    }
                };
                (prototype, parent)
            }
            _ => {
                return Err(RuntimeError::type_error(
                    "class heritage is not a constructor or null",
                ));
            }
        }
    } else {
        (
            Some(outer.realm.intrinsic(IntrinsicId::ObjectPrototype)),
            outer.realm.intrinsic(IntrinsicId::FunctionPrototype),
        )
    };
    for element in &class.elements {
        if let Some(crate::ast::Expr {
            kind: ExprKind::PrivateIdentifier(name),
            ..
        }) = &element.key
        {
            let kind = match &element.kind {
                ClassElementKind::Field(_) => super::private_fields::Kind::Field,
                ClassElementKind::Method {
                    kind: MethodKind::Get | MethodKind::Set,
                    ..
                } => super::private_fields::Kind::Accessor,
                _ => super::private_fields::Kind::Method,
            };
            environment.declare_private(JsString::from(name.as_str()), kind);
        }
    }
    let prototype = Object::new_for(&constructor_parent);
    if let Some(parent) = prototype_parent {
        prototype.initialize_prototype(Some(parent));
    }
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
    let constructor_kind = if class.super_class.is_none() {
        ConstructorKind::Base
    } else if explicit.is_some() {
        ConstructorKind::Derived
    } else {
        ConstructorKind::DefaultDerived
    };
    let constructor = create_function_with_constructor(
        &code,
        FunctionKind::Method(MethodKind::Constructor),
        &evaluator,
        Some(prototype.clone()),
        constructor_kind,
        Some(constructor_parent),
        Some(binding.clone().or(inferred_name).unwrap_or_default()),
    )?;
    constructor.define_own_property(
        "prototype".into(),
        PropertyDescriptor::data(Value::Object(prototype.clone()), false, false, false),
    )?;
    prototype.define_own_property(
        "constructor".into(),
        PropertyDescriptor::data(Value::Object(constructor.clone()), true, false, true),
    )?;
    let mut static_initializers = Vec::new();
    let mut private_methods: Vec<(super::private_fields::Name, bool, Object)> = vec![];
    for element in &class.elements {
        if let ClassElementKind::StaticBlock(block) = &element.kind {
            let code = Function {
                span: element.span,
                name: None,
                parameters: vec![],
                body: FunctionBody::Block(block.clone()),
                asynchronous: false,
                generator: false,
                strict: true,
            };
            let function = create_function_with_constructor(
                &code,
                FunctionKind::Method(MethodKind::Method),
                &evaluator,
                Some(constructor.clone()),
                ConstructorKind::Base,
                None,
                None,
            )?;
            static_initializers.push(StaticInitializer::Block(function));
            continue;
        }
        let key = element
            .key
            .as_ref()
            .ok_or_else(|| RuntimeError::syntax_error("class method has no key"))?;
        let private_text = match &key.kind {
            ExprKind::PrivateIdentifier(name) => Some(name.as_str()),
            _ => None,
        };
        let private = private_text
            .map(|name| environment.resolve_private(name))
            .transpose()?;
        let key = if let Some(name) = private_text {
            PropertyKey::from(format!("#{name}").as_str())
        } else if !element.computed {
            if let ExprKind::Identifier(name) = &key.kind {
                PropertyKey::from(name.as_str())
            } else {
                super::object_literal::property_key(&outer.realm, evaluator.evaluate(key)?)?
            }
        } else {
            super::object_literal::property_key(&outer.realm, evaluator.evaluate(key)?)?
        };
        if let ClassElementKind::Field(expression) = &element.kind {
            if let Some(name) = private {
                if element.is_static {
                    let initializer = expression
                        .as_ref()
                        .map(|expression| {
                            super::class_fields::initializer(
                                &constructor,
                                &evaluator,
                                &key,
                                expression,
                            )
                        })
                        .transpose()?;
                    static_initializers.push(StaticInitializer::PrivateField(name, initializer));
                } else {
                    super::class_fields::add_private(
                        &constructor,
                        &prototype,
                        &evaluator,
                        private_text.expect("private name"),
                        expression.as_ref(),
                    )?;
                }
                continue;
            }
            if element.is_static {
                if let Some(expression) = expression.as_ref().filter(|expression| {
                    super::class_fields::can_evaluate_without_function_environment(expression)
                }) {
                    static_initializers.push(StaticInitializer::DirectField(key, expression));
                    continue;
                }
                let initializer = expression
                    .as_ref()
                    .map(|expression| {
                        super::class_fields::initializer(&constructor, &evaluator, &key, expression)
                    })
                    .transpose()?;
                static_initializers.push(StaticInitializer::Field(key, initializer));
                continue;
            }
            super::class_fields::add(
                &constructor,
                &prototype,
                &evaluator,
                key,
                expression.as_ref(),
            )?;
            continue;
        }
        let ClassElementKind::Method { function, kind } = &element.kind else {
            unreachable!()
        };
        if *kind == MethodKind::Constructor && !element.is_static {
            continue;
        }
        let home = if element.is_static {
            &constructor
        } else {
            &prototype
        };
        let code = strict_code(function);
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
        let method = create_function_with_constructor(
            &code,
            FunctionKind::Method(*kind),
            &evaluator,
            Some(home.clone()),
            ConstructorKind::Base,
            None,
            Some(JsString::from_code_units_slice(&name)),
        )?;
        if let Some(name) = private {
            if matches!(kind, MethodKind::Get | MethodKind::Set) {
                // Shared hidden pair, never exposed as an ordinary JS value.
                let pair = if let Some((_, _, pair)) = private_methods
                    .iter()
                    .find(|(key, is_static, _)| key.same(&name) && *is_static == element.is_static)
                {
                    pair.clone()
                } else {
                    let pair = Object::new_for(&constructor);
                    private_methods.push((name.clone(), element.is_static, pair.clone()));
                    pair
                };
                let key = if *kind == MethodKind::Get {
                    "get"
                } else {
                    "set"
                };
                if !pair.create_data_property(key.into(), Value::Object(method))? {
                    return Err(RuntimeError::internal_error(
                        "cannot define private accessor",
                    ));
                }
            } else {
                private_methods.push((name, element.is_static, method));
            }
            continue;
        }
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
    for (name, is_static, value) in private_methods {
        if is_static {
            constructor.add_private(name, Value::Object(value))?;
        } else {
            super::class_fields::add_private_method(&constructor, name, &value)?;
        }
    }
    for initializer in static_initializers {
        match initializer {
            StaticInitializer::PrivateField(name, initializer) => {
                let value = match initializer {
                    Some(function) => function.call(Value::Object(constructor.clone()), &[])?,
                    None => Value::Undefined,
                };
                constructor.add_private(name, value)?;
            }
            StaticInitializer::Field(key, initializer) => {
                let value = match initializer {
                    Some(function) => function.call(Value::Object(constructor.clone()), &[])?,
                    None => Value::Undefined,
                };
                define_static_field(&constructor, key, value)?;
            }
            StaticInitializer::DirectField(key, expression) => {
                let direct = ExpressionEvaluator::with_environment(
                    outer.realm.clone(),
                    environment.clone(),
                    Value::Object(constructor.clone()),
                    true,
                );
                let value = direct
                    .evaluate(expression)
                    .map_err(|error| outer.realm.bind_exception(error))?;
                define_static_field(&constructor, key, value)?;
            }
            StaticInitializer::Block(function) => {
                function.call(Value::Object(constructor.clone()), &[])?;
            }
        }
    }
    Ok(constructor)
}

fn define_static_field(constructor: &Object, key: PropertyKey, value: Value) -> RuntimeResult<()> {
    if !constructor.define_own_property(key, PropertyDescriptor::data(value, true, true, true))? {
        return Err(RuntimeError::type_error("cannot define static field"));
    }
    Ok(())
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
