//! Direct expression evaluation and ECMAScript Reference Records.

use num_bigint::BigInt;

use crate::ast::{Expr, ExprKind, Literal, MethodKind, NumberKind};

use super::{
    Environment, FunctionKind, JsString, Numeric, Object, PropertyKey, Realm, RuntimeError,
    RuntimeResult, Value, function::create_function,
};

/// The name component of a Reference Record.
///
/// Computed names deliberately retain their pre-`ToPropertyKey` value so that
/// observable key coercion occurs at `GetValue`/`PutValue`, not while the base
/// expression is being evaluated.
#[derive(Clone, Debug)]
pub enum ReferencedName {
    /// A name whose property-key conversion is already complete.
    PropertyKey(PropertyKey),
    /// A computed name awaiting `ToPropertyKey`.
    Value(Value),
}

impl ReferencedName {
    fn property_key(&self) -> RuntimeResult<PropertyKey> {
        match self {
            Self::PropertyKey(key) => Ok(key.clone()),
            Self::Value(value) => value.to_property_key(),
        }
    }

    fn binding_name(&self) -> RuntimeResult<JsString> {
        match self.property_key()? {
            PropertyKey::String(name) => Ok(name),
            PropertyKey::Symbol(_) => Err(RuntimeError::reference_error(
                "an environment binding name cannot be a Symbol",
            )),
        }
    }
}

#[derive(Clone, Debug)]
enum ReferenceBase {
    Environment(Environment),
    Property(Value),
    Unresolvable(Object),
}

/// An ECMAScript Reference Record used by identifier and property expressions.
#[derive(Clone, Debug)]
pub struct Reference {
    base: ReferenceBase,
    name: ReferencedName,
    strict: bool,
    this_value: Option<Value>,
}

impl Reference {
    /// Creates a resolved environment reference.
    #[must_use]
    pub fn environment(environment: Environment, name: JsString, strict: bool) -> Self {
        Self {
            base: ReferenceBase::Environment(environment),
            name: ReferencedName::PropertyKey(PropertyKey::String(name)),
            strict,
            this_value: None,
        }
    }

    /// Creates a property reference, optionally carrying an explicit `this` value.
    #[must_use]
    pub fn property(
        base: Value,
        name: ReferencedName,
        strict: bool,
        this_value: Option<Value>,
    ) -> Self {
        Self {
            base: ReferenceBase::Property(base),
            name,
            strict,
            this_value,
        }
    }

    /// Creates an unresolvable reference associated with a Realm global object.
    #[must_use]
    pub fn unresolvable(global_object: Object, name: JsString, strict: bool) -> Self {
        Self {
            base: ReferenceBase::Unresolvable(global_object),
            name: ReferencedName::PropertyKey(PropertyKey::String(name)),
            strict,
            this_value: None,
        }
    }

    /// Returns whether this reference is unresolvable.
    #[must_use]
    pub fn is_unresolvable(&self) -> bool {
        matches!(self.base, ReferenceBase::Unresolvable(_))
    }

    /// Returns whether this is a property reference.
    #[must_use]
    pub fn is_property_reference(&self) -> bool {
        matches!(self.base, ReferenceBase::Property(_))
    }

    /// Implements `GetThisValue` for call evaluation.
    #[must_use]
    pub fn this_value(&self) -> Option<Value> {
        match &self.base {
            ReferenceBase::Property(base) => {
                Some(self.this_value.clone().unwrap_or_else(|| base.clone()))
            }
            ReferenceBase::Environment(environment) => {
                environment.with_base_object().map(Value::Object)
            }
            ReferenceBase::Unresolvable(_) => None,
        }
    }

    /// Implements `GetValue`.
    pub fn get_value(&self) -> RuntimeResult<Value> {
        match &self.base {
            ReferenceBase::Environment(environment) => {
                environment.get_binding_value(&self.name.binding_name()?, self.strict)
            }
            ReferenceBase::Unresolvable(_) => Err(RuntimeError::reference_error(format!(
                "{} is not defined",
                self.name.binding_name()?.to_string_lossy()
            ))),
            ReferenceBase::Property(base) => get_property(base, &self.name.property_key()?),
        }
    }

    /// Implements `PutValue`, including sloppy assignment to the global object.
    pub fn put_value(&self, value: Value) -> RuntimeResult<()> {
        match &self.base {
            ReferenceBase::Environment(environment) => {
                environment.set_mutable_binding(&self.name.binding_name()?, value, self.strict)
            }
            ReferenceBase::Unresolvable(global) if !self.strict => {
                let key = self.name.property_key()?;
                if global.set(&key, value)? {
                    Ok(())
                } else {
                    Err(RuntimeError::type_error(
                        "global property assignment was rejected",
                    ))
                }
            }
            ReferenceBase::Unresolvable(_) => Err(RuntimeError::reference_error(format!(
                "{} is not defined",
                self.name.binding_name()?.to_string_lossy()
            ))),
            ReferenceBase::Property(base) => {
                let key = self.name.property_key()?;
                let succeeded = set_property(base, &key, value)?;
                if !succeeded && self.strict {
                    Err(RuntimeError::type_error("property assignment was rejected"))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Implements deletion of the referenced binding or property.
    pub fn delete(&self) -> RuntimeResult<bool> {
        match &self.base {
            ReferenceBase::Unresolvable(_) => Ok(true),
            ReferenceBase::Environment(environment) => {
                if self.strict {
                    Err(RuntimeError::syntax_error(
                        "cannot delete an unqualified identifier in strict mode",
                    ))
                } else {
                    environment.delete_binding(&self.name.binding_name()?)
                }
            }
            ReferenceBase::Property(Value::Object(object)) => {
                Ok(object.delete(&self.name.property_key()?))
            }
            ReferenceBase::Property(Value::Undefined | Value::Null) => Err(
                RuntimeError::type_error("cannot delete a property of null or undefined"),
            ),
            ReferenceBase::Property(_) => Ok(true),
        }
    }
}

#[derive(Clone, Debug)]
enum Evaluation {
    Value(Value),
    Reference(Reference),
    ShortCircuit,
}

impl Evaluation {
    fn value(self) -> RuntimeResult<Value> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Reference(reference) => reference.get_value(),
            Self::ShortCircuit => Ok(Value::Undefined),
        }
    }
}

/// A direct, AST-based expression evaluator for a Realm and lexical environment.
#[derive(Clone, Debug)]
pub struct ExpressionEvaluator {
    pub(super) realm: Realm,
    pub(super) environment: Environment,
    pub(super) this_value: Value,
    pub(super) new_target: Option<Object>,
    pub(super) strict: bool,
}

impl ExpressionEvaluator {
    /// Creates an evaluator in the Realm's global environment.
    #[must_use]
    pub fn new(realm: Realm) -> Self {
        let environment = realm.global_environment();
        let this_value = Value::Object(realm.global_object());
        Self {
            realm,
            environment,
            this_value,
            new_target: None,
            strict: false,
        }
    }

    /// Creates an evaluator with explicit execution bindings.
    #[must_use]
    pub fn with_environment(
        realm: Realm,
        environment: Environment,
        this_value: Value,
        strict: bool,
    ) -> Self {
        Self {
            realm,
            environment,
            this_value,
            new_target: None,
            strict,
        }
    }

    /// Supplies the active constructor's `new.target` value.
    #[must_use]
    pub fn with_new_target(mut self, new_target: Option<Object>) -> Self {
        self.new_target = new_target;
        self
    }

    /// Evaluates an expression and applies `GetValue` to its result.
    pub fn evaluate(&self, expression: &Expr) -> RuntimeResult<Value> {
        self.eval(expression)?.value()
    }

    /// Evaluates an assignment target and returns its Reference Record.
    pub fn evaluate_reference(&self, expression: &Expr) -> RuntimeResult<Reference> {
        match self.eval(expression)? {
            Evaluation::Reference(reference) => Ok(reference),
            _ => Err(RuntimeError::reference_error(
                "expression does not produce a reference",
            )),
        }
    }

    fn eval(&self, expression: &Expr) -> RuntimeResult<Evaluation> {
        match &expression.kind {
            ExprKind::Identifier(name) => self.identifier(name).map(Evaluation::Reference),
            ExprKind::Literal(literal) => {
                literal_value(literal, self.strict).map(Evaluation::Value)
            }
            ExprKind::This => Ok(Evaluation::Value(self.this_value.clone())),
            ExprKind::Parenthesized(inner) => self.eval(inner)?.value().map(Evaluation::Value),
            ExprKind::MetaProperty { meta, property } if meta == "new" && property == "target" => {
                Ok(Evaluation::Value(
                    self.new_target
                        .clone()
                        .map_or(Value::Undefined, Value::Object),
                ))
            }
            ExprKind::Unary { operator, argument } => {
                self.unary(operator, argument).map(Evaluation::Value)
            }
            ExprKind::Update {
                operator,
                argument,
                prefix,
            } => self
                .update(operator, argument, *prefix)
                .map(Evaluation::Value),
            ExprKind::Binary {
                operator,
                left,
                right,
            } => self.binary(operator, left, right).map(Evaluation::Value),
            ExprKind::Conditional {
                test,
                consequent,
                alternate,
            } => {
                if self.evaluate(test)?.to_boolean() {
                    self.eval(consequent)
                } else {
                    self.eval(alternate)
                }
            }
            ExprKind::Assignment {
                operator,
                target,
                value,
            } => self
                .assignment(operator, target, value)
                .map(Evaluation::Value),
            ExprKind::Sequence(expressions) => {
                let mut result = Value::Undefined;
                for expression in expressions {
                    result = self.evaluate(expression)?;
                }
                Ok(Evaluation::Value(result))
            }
            ExprKind::Array { elements, .. } => self.array(elements).map(Evaluation::Value),
            ExprKind::Object(properties) => self.object(properties).map(Evaluation::Value),
            ExprKind::Member {
                object,
                property,
                computed,
                optional,
            } => self.member(object, property, *computed, *optional),
            ExprKind::Call {
                callee,
                arguments,
                optional,
            } => self.call(callee, arguments, *optional),
            ExprKind::New { callee, arguments } => {
                self.construct(callee, arguments).map(Evaluation::Value)
            }
            ExprKind::Template {
                tag: None,
                quasis,
                expressions,
            } => {
                let mut units = Vec::new();
                for (index, quasi) in quasis.iter().enumerate() {
                    units.extend(quasi.cooked.as_ref().ok_or_else(|| {
                        RuntimeError::syntax_error("invalid escape in untagged template")
                    })?);
                    if let Some(expression) = expressions.get(index) {
                        units.extend(self.evaluate(expression)?.to_js_string()?.code_units());
                    }
                }
                Ok(Evaluation::Value(Value::String(JsString::from_code_units(
                    units,
                ))))
            }
            ExprKind::Spread(_) => Err(RuntimeError::syntax_error(
                "spread is only valid in an array, object, or argument list",
            )),
            ExprKind::PrivateIdentifier(_) | ExprKind::Super => Err(RuntimeError::reference_error(
                "private and super references require a class execution context",
            )),
            ExprKind::Function(function) => {
                create_function(function, FunctionKind::Normal, self, None)
                    .map(|function| Evaluation::Value(Value::Object(function)))
            }
            ExprKind::Arrow(function) => create_function(function, FunctionKind::Arrow, self, None)
                .map(|function| Evaluation::Value(Value::Object(function))),
            ExprKind::Method { function, kind } => {
                create_function(function, FunctionKind::Method(*kind), self, None)
                    .map(|function| Evaluation::Value(Value::Object(function)))
            }
            ExprKind::Class(_) => Err(RuntimeError::internal_error(
                "class evaluation is not yet implemented",
            )),
            ExprKind::Yield { .. } => Err(RuntimeError::internal_error(
                "yield evaluation is implemented by milestone #28",
            )),
            ExprKind::ImportCall { .. } => Err(RuntimeError::internal_error(
                "dynamic import evaluation is implemented by milestone #30",
            )),
            ExprKind::Template { tag: Some(_), .. } => Err(RuntimeError::internal_error(
                "tagged template evaluation requires function evaluation from milestone #27",
            )),
            ExprKind::MetaProperty { .. } => {
                Err(RuntimeError::syntax_error("unsupported meta-property"))
            }
        }
    }

    fn identifier(&self, name: &str) -> RuntimeResult<Reference> {
        let name = JsString::from(name);
        Ok(match self.environment.resolve_binding(&name)? {
            Some(environment) => Reference::environment(environment, name, self.strict),
            None => Reference::unresolvable(self.realm.global_object(), name, self.strict),
        })
    }

    fn member(
        &self,
        object: &Expr,
        property: &Expr,
        computed: bool,
        optional: bool,
    ) -> RuntimeResult<Evaluation> {
        let base = match self.eval(object)? {
            Evaluation::ShortCircuit => return Ok(Evaluation::ShortCircuit),
            value => value.value()?,
        };
        if optional && matches!(base, Value::Undefined | Value::Null) {
            return Ok(Evaluation::ShortCircuit);
        }
        if matches!(base, Value::Undefined | Value::Null) {
            return Err(RuntimeError::type_error(
                "cannot access a property of null or undefined",
            ));
        }
        let name = if computed {
            ReferencedName::Value(self.evaluate(property)?)
        } else {
            ReferencedName::PropertyKey(PropertyKey::String(member_name(property)?))
        };
        Ok(Evaluation::Reference(Reference::property(
            base,
            name,
            self.strict,
            None,
        )))
    }

    fn call(&self, callee: &Expr, arguments: &[Expr], optional: bool) -> RuntimeResult<Evaluation> {
        let result = self.eval(callee)?;
        if matches!(result, Evaluation::ShortCircuit) {
            return Ok(Evaluation::ShortCircuit);
        }
        let (function, this_value) = match result {
            Evaluation::Reference(reference) => {
                let this_value = reference.this_value().unwrap_or(Value::Undefined);
                (reference.get_value()?, this_value)
            }
            Evaluation::Value(value) => (value, Value::Undefined),
            Evaluation::ShortCircuit => unreachable!(),
        };
        if optional && matches!(function, Value::Undefined | Value::Null) {
            return Ok(Evaluation::ShortCircuit);
        }
        let Value::Object(function) = function else {
            return Err(RuntimeError::type_error("value is not callable"));
        };
        if !function.is_callable() {
            return Err(RuntimeError::type_error("value is not callable"));
        }
        let arguments = self.arguments(arguments)?;
        function.call(this_value, &arguments).map(Evaluation::Value)
    }

    fn construct(&self, callee: &Expr, arguments: &[Expr]) -> RuntimeResult<Value> {
        let Value::Object(constructor) = self.evaluate(callee)? else {
            return Err(RuntimeError::type_error("value is not a constructor"));
        };
        if !constructor.is_constructor() {
            return Err(RuntimeError::type_error("value is not a constructor"));
        }
        Ok(Value::Object(constructor.construct(
            &self.arguments(arguments)?,
            constructor.clone(),
        )?))
    }

    fn arguments(&self, arguments: &[Expr]) -> RuntimeResult<Vec<Value>> {
        let mut values = Vec::new();
        for argument in arguments {
            if let ExprKind::Spread(value) = &argument.kind {
                values.extend(array_like_values(&self.evaluate(value)?)?);
            } else {
                values.push(self.evaluate(argument)?);
            }
        }
        Ok(values)
    }

    fn unary(&self, operator: &str, argument: &Expr) -> RuntimeResult<Value> {
        if operator == "typeof" {
            let result = self.eval(argument)?;
            if matches!(&result, Evaluation::Reference(reference) if reference.is_unresolvable()) {
                return Ok(Value::String(JsString::from("undefined")));
            }
            return Ok(Value::String(JsString::from(type_of(&result.value()?))));
        }
        if operator == "delete" {
            return Ok(Value::Boolean(match self.eval(argument)? {
                Evaluation::Reference(reference) => reference.delete()?,
                _ => true,
            }));
        }
        let value = self.evaluate(argument)?;
        match operator {
            "void" => Ok(Value::Undefined),
            "!" => Ok(Value::Boolean(!value.to_boolean())),
            "+" => Ok(Value::Number(value.to_number()?)),
            "-" => match value.to_numeric()? {
                Numeric::Number(value) => Ok(Value::Number(-value)),
                Numeric::BigInt(value) => Ok(Value::BigInt(-value)),
            },
            "~" => match value.to_numeric()? {
                Numeric::Number(value) => Ok(Value::Number(f64::from(!to_i32(value)))),
                Numeric::BigInt(value) => Ok(Value::BigInt(!value)),
            },
            "await" => Err(RuntimeError::internal_error(
                "await evaluation is implemented by milestone #28",
            )),
            _ => Err(RuntimeError::syntax_error("unknown unary operator")),
        }
    }

    fn update(&self, operator: &str, argument: &Expr, prefix: bool) -> RuntimeResult<Value> {
        let reference = self.evaluate_reference(argument)?;
        let old = reference.get_value()?.to_numeric()?;
        let (old_value, new_value) = match old {
            Numeric::Number(value) => (
                Value::Number(value),
                Value::Number(if operator == "++" {
                    value + 1.0
                } else {
                    value - 1.0
                }),
            ),
            Numeric::BigInt(value) => {
                let next = if operator == "++" {
                    &value + 1
                } else {
                    &value - 1
                };
                (Value::BigInt(value), Value::BigInt(next))
            }
        };
        reference.put_value(new_value.clone())?;
        Ok(if prefix { new_value } else { old_value })
    }

    fn binary(&self, operator: &str, left: &Expr, right: &Expr) -> RuntimeResult<Value> {
        let left = self.evaluate(left)?;
        match operator {
            "&&" if !left.to_boolean() => return Ok(left),
            "||" if left.to_boolean() => return Ok(left),
            "??" if !matches!(left, Value::Undefined | Value::Null) => return Ok(left),
            _ => {}
        }
        let right = self.evaluate(right)?;
        binary_values(operator, left, right)
    }

    fn assignment(&self, operator: &str, target: &Expr, value: &Expr) -> RuntimeResult<Value> {
        let reference = self.evaluate_reference(target)?;
        let assigned = if operator == "=" {
            self.evaluate(value)?
        } else {
            let left = reference.get_value()?;
            if (operator == "&&=" && !left.to_boolean())
                || (operator == "||=" && left.to_boolean())
                || (operator == "??=" && !matches!(left, Value::Undefined | Value::Null))
            {
                return Ok(left);
            }
            let right = self.evaluate(value)?;
            binary_values(operator.strip_suffix('=').unwrap_or(operator), left, right)?
        };
        reference.put_value(assigned.clone())?;
        Ok(assigned)
    }

    fn array(&self, elements: &[Option<Expr>]) -> RuntimeResult<Value> {
        let object = Object::new();
        let mut index = 0_u32;
        for element in elements {
            if let Some(element) = element {
                if let ExprKind::Spread(value) = &element.kind {
                    for value in array_like_values(&self.evaluate(value)?)? {
                        object.create_data_property(index_key(index), value)?;
                        index += 1;
                    }
                } else {
                    object.create_data_property(index_key(index), self.evaluate(element)?)?;
                    index += 1;
                }
            } else {
                index += 1;
            }
        }
        object.create_data_property(
            PropertyKey::String(JsString::from("length")),
            Value::Number(f64::from(index)),
        )?;
        Ok(Value::Object(object))
    }

    fn object(&self, properties: &[crate::ast::ObjectProperty]) -> RuntimeResult<Value> {
        let object = Object::new();
        for property in properties {
            if property.spread {
                let spread_value = self.evaluate(&property.key)?;
                if matches!(spread_value, Value::Undefined | Value::Null) {
                    continue;
                }
                let source = match spread_value {
                    Value::Object(source) => source,
                    Value::String(string) => {
                        let source = Object::new();
                        for (index, unit) in string.code_units().iter().enumerate() {
                            source.create_data_property(
                                index_key(index as u32),
                                Value::String(JsString::from_code_units(vec![*unit])),
                            )?;
                        }
                        source
                    }
                    _ => Object::new(),
                };
                for key in source.own_property_keys() {
                    if source
                        .get_own_property(&key)
                        .is_some_and(|descriptor| descriptor.enumerable == Some(true))
                    {
                        object.create_data_property(key.clone(), source.get(&key)?)?;
                    }
                }
                continue;
            }
            let key = if property.computed {
                self.evaluate(&property.key)?.to_property_key()?
            } else {
                PropertyKey::String(member_name(&property.key)?)
            };
            if let Some(Expr {
                kind: ExprKind::Method { function, kind },
                ..
            }) = &property.value
            {
                let function = create_function(
                    function,
                    FunctionKind::Method(*kind),
                    self,
                    Some(object.clone()),
                )?;
                match kind {
                    MethodKind::Get => {
                        object.define_own_property(
                            key,
                            super::PropertyDescriptor {
                                get: Some(Some(function)),
                                enumerable: Some(true),
                                configurable: Some(true),
                                ..super::PropertyDescriptor::default()
                            },
                        )?;
                    }
                    MethodKind::Set => {
                        object.define_own_property(
                            key,
                            super::PropertyDescriptor {
                                set: Some(Some(function)),
                                enumerable: Some(true),
                                configurable: Some(true),
                                ..super::PropertyDescriptor::default()
                            },
                        )?;
                    }
                    MethodKind::Method | MethodKind::Constructor => {
                        object.create_data_property(key, Value::Object(function))?;
                    }
                }
            } else {
                let value = property.value.as_ref().map_or_else(
                    || self.evaluate(&property.key),
                    |value| self.evaluate(value),
                )?;
                object.create_data_property(key, value)?;
            }
        }
        Ok(Value::Object(object))
    }
}

fn get_property(base: &Value, key: &PropertyKey) -> RuntimeResult<Value> {
    match base {
        Value::Object(object) => object.get_with_receiver(key, base.clone()),
        Value::String(string) => match key {
            PropertyKey::String(name) if name == &JsString::from("length") => {
                Ok(Value::Number(string.code_units().len() as f64))
            }
            PropertyKey::String(name) => name
                .to_string_lossy()
                .parse::<usize>()
                .ok()
                .and_then(|index| string.code_units().get(index))
                .map_or(Ok(Value::Undefined), |unit| {
                    Ok(Value::String(JsString::from_code_units(vec![*unit])))
                }),
            PropertyKey::Symbol(_) => Ok(Value::Undefined),
        },
        Value::Undefined | Value::Null => Err(RuntimeError::type_error(
            "cannot access a property of null or undefined",
        )),
        _ => Ok(Value::Undefined),
    }
}

fn set_property(base: &Value, key: &PropertyKey, value: Value) -> RuntimeResult<bool> {
    match base {
        Value::Object(object) => object.set_with_receiver(key, value, base.clone()),
        Value::Undefined | Value::Null => Err(RuntimeError::type_error(
            "cannot assign a property of null or undefined",
        )),
        _ => Ok(false),
    }
}

fn member_name(expression: &Expr) -> RuntimeResult<JsString> {
    match &expression.kind {
        ExprKind::Identifier(name) | ExprKind::PrivateIdentifier(name) => {
            Ok(JsString::from(name.as_str()))
        }
        ExprKind::Literal(Literal::String { value, .. }) => {
            Ok(JsString::from_code_units(value.clone()))
        }
        ExprKind::Literal(Literal::Number { raw, .. }) => Ok(JsString::from(raw.as_str())),
        _ => Err(RuntimeError::syntax_error(
            "invalid non-computed property name",
        )),
    }
}

fn literal_value(literal: &Literal, strict: bool) -> RuntimeResult<Value> {
    match literal {
        Literal::Null => Ok(Value::Null),
        Literal::Boolean(value) => Ok(Value::Boolean(*value)),
        Literal::String {
            value: _,
            legacy_escape,
        } if strict && *legacy_escape => Err(RuntimeError::syntax_error(
            "legacy string escape in strict code",
        )),
        Literal::String { value, .. } => {
            Ok(Value::String(JsString::from_code_units(value.clone())))
        }
        Literal::RegExp(_) => Err(RuntimeError::internal_error(
            "RegExp literal evaluation is implemented by milestone #35",
        )),
        Literal::Number { raw, kind } => parse_numeric_literal(raw, *kind),
    }
}

fn parse_numeric_literal(raw: &str, kind: NumberKind) -> RuntimeResult<Value> {
    let cleaned = raw.replace('_', "");
    let bigint = kind == NumberKind::BigInt;
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
    if bigint {
        return BigInt::parse_bytes(digits.as_bytes(), radix)
            .map(Value::BigInt)
            .ok_or_else(|| RuntimeError::syntax_error("invalid BigInt literal"));
    }
    let number = if radix == 10 {
        spelling.parse::<f64>().ok()
    } else {
        u128::from_str_radix(digits, radix)
            .ok()
            .map(|value| value as f64)
    };
    number
        .map(Value::Number)
        .ok_or_else(|| RuntimeError::syntax_error("invalid numeric literal"))
}

fn binary_values(operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
    match operator {
        "+" => {
            let left = left.to_primitive(super::PrimitiveHint::Default)?;
            let right = right.to_primitive(super::PrimitiveHint::Default)?;
            if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                let mut units = left.to_js_string()?.code_units().to_vec();
                units.extend(right.to_js_string()?.code_units());
                return Ok(Value::String(JsString::from_code_units(units)));
            }
            numeric_binary(operator, left, right)
        }
        "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
            numeric_binary(operator, left, right)
        }
        "===" => Ok(Value::Boolean(strict_equal(&left, &right))),
        "!==" => Ok(Value::Boolean(!strict_equal(&left, &right))),
        "==" => Ok(Value::Boolean(abstract_equal(left, right)?)),
        "!=" => Ok(Value::Boolean(!abstract_equal(left, right)?)),
        "<" | ">" | "<=" | ">=" => relational(operator, left, right),
        "in" => {
            let Value::Object(object) = right else {
                return Err(RuntimeError::type_error(
                    "right-hand side of 'in' is not an object",
                ));
            };
            Ok(Value::Boolean(
                object.has_property(&left.to_property_key()?)?,
            ))
        }
        "instanceof" => instance_of(left, right).map(Value::Boolean),
        "&&" | "||" | "??" => Ok(right),
        _ => Err(RuntimeError::syntax_error(format!(
            "unsupported binary operator {operator}"
        ))),
    }
}

fn numeric_binary(operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
    match (left.to_numeric()?, right.to_numeric()?) {
        (Numeric::Number(a), Numeric::Number(b)) => Ok(Value::Number(match operator {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => a / b,
            "%" => a % b,
            "**" => a.powf(b),
            "&" => f64::from(to_i32(a) & to_i32(b)),
            "|" => f64::from(to_i32(a) | to_i32(b)),
            "^" => f64::from(to_i32(a) ^ to_i32(b)),
            "<<" => f64::from(to_i32(a).wrapping_shl(to_u32(b) & 31)),
            ">>" => f64::from(to_i32(a).wrapping_shr(to_u32(b) & 31)),
            ">>>" => f64::from(to_u32(a).wrapping_shr(to_u32(b) & 31)),
            _ => unreachable!(),
        })),
        (Numeric::BigInt(a), Numeric::BigInt(b)) => {
            let result = match operator {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" if b == BigInt::from(0) => {
                    return Err(RuntimeError::range_error("BigInt division by zero"));
                }
                "/" => a / b,
                "%" if b == BigInt::from(0) => {
                    return Err(RuntimeError::range_error("BigInt division by zero"));
                }
                "%" => a % b,
                "**" => {
                    let exponent = bigint_shift(&b)?;
                    if b < BigInt::from(0) {
                        return Err(RuntimeError::range_error("negative BigInt exponent"));
                    }
                    a.pow(exponent)
                }
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" => {
                    if b < BigInt::from(0) {
                        a >> bigint_shift(&(-b))?
                    } else {
                        a << bigint_shift(&b)?
                    }
                }
                ">>" => {
                    if b < BigInt::from(0) {
                        a << bigint_shift(&(-b))?
                    } else {
                        a >> bigint_shift(&b)?
                    }
                }
                ">>>" => {
                    return Err(RuntimeError::type_error(
                        "BigInt has no unsigned right shift",
                    ));
                }
                _ => unreachable!(),
            };
            Ok(Value::BigInt(result))
        }
        _ => Err(RuntimeError::type_error(
            "cannot mix BigInt and Number operands",
        )),
    }
}

fn bigint_shift(value: &BigInt) -> RuntimeResult<u32> {
    value
        .to_string()
        .parse::<u32>()
        .map_err(|_| RuntimeError::range_error("BigInt exponent or shift count is too large"))
}

pub(super) fn strict_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => !a.is_nan() && !b.is_nan() && a == b,
        _ => left.same_value(right),
    }
}

fn abstract_equal(left: Value, right: Value) -> RuntimeResult<bool> {
    if std::mem::discriminant(&left) == std::mem::discriminant(&right) {
        return Ok(strict_equal(&left, &right));
    }
    match (left, right) {
        (Value::Null, Value::Undefined) | (Value::Undefined, Value::Null) => Ok(true),
        (Value::Number(a), Value::String(b)) | (Value::String(b), Value::Number(a)) => {
            Ok(!a.is_nan() && a == Value::String(b).to_number()?)
        }
        (Value::Boolean(a), b) => abstract_equal(Value::Number(if a { 1.0 } else { 0.0 }), b),
        (a, Value::Boolean(b)) => abstract_equal(a, Value::Number(if b { 1.0 } else { 0.0 })),
        (Value::Object(a), b) => abstract_equal(
            Value::Object(a).to_primitive(super::PrimitiveHint::Default)?,
            b,
        ),
        (a, Value::Object(b)) => abstract_equal(
            a,
            Value::Object(b).to_primitive(super::PrimitiveHint::Default)?,
        ),
        (Value::BigInt(a), Value::String(b)) | (Value::String(b), Value::BigInt(a)) => {
            Ok(Value::String(b).to_bigint().is_ok_and(|b| a == b))
        }
        (Value::BigInt(a), Value::Number(b)) | (Value::Number(b), Value::BigInt(a)) => {
            Ok(b.is_finite() && b.fract() == 0.0 && a.to_string() == format!("{b:.0}"))
        }
        _ => Ok(false),
    }
}

fn relational(operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
    let left = left.to_primitive(super::PrimitiveHint::Number)?;
    let right = right.to_primitive(super::PrimitiveHint::Number)?;
    let less = if let (Value::String(a), Value::String(b)) = (&left, &right) {
        a < b
    } else {
        let a = left.to_number()?;
        let b = right.to_number()?;
        if a.is_nan() || b.is_nan() {
            return Ok(Value::Boolean(false));
        }
        a < b
    };
    let equal = strict_equal(&left, &right)
        || (!matches!((&left, &right), (Value::String(_), Value::String(_)))
            && left.to_number()? == right.to_number()?);
    Ok(Value::Boolean(match operator {
        "<" => less,
        ">" => !less && !equal,
        "<=" => less || equal,
        ">=" => !less,
        _ => unreachable!(),
    }))
}

fn instance_of(left: Value, right: Value) -> RuntimeResult<bool> {
    let Value::Object(constructor) = right else {
        return Err(RuntimeError::type_error(
            "right-hand side of instanceof is not an object",
        ));
    };
    if !constructor.is_callable() {
        return Err(RuntimeError::type_error(
            "right-hand side of instanceof is not callable",
        ));
    }
    let Value::Object(prototype) =
        constructor.get(&PropertyKey::String(JsString::from("prototype")))?
    else {
        return Err(RuntimeError::type_error(
            "constructor prototype is not an object",
        ));
    };
    let Value::Object(object) = left else {
        return Ok(false);
    };
    let mut current = object.prototype();
    while let Some(candidate) = current {
        if candidate == prototype {
            return Ok(true);
        }
        current = candidate.prototype();
    }
    Ok(false)
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Null => "object",
        Value::Boolean(_) => "boolean",
        Value::String(_) => "string",
        Value::Symbol(_) => "symbol",
        Value::Number(_) => "number",
        Value::BigInt(_) => "bigint",
        Value::Object(object) if object.is_callable() => "function",
        Value::Object(_) => "object",
    }
}

fn to_u32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        0
    } else {
        value.trunc().rem_euclid(4_294_967_296.0) as u32
    }
}
fn to_i32(value: f64) -> i32 {
    to_u32(value) as i32
}
fn index_key(index: u32) -> PropertyKey {
    PropertyKey::String(JsString::from(index.to_string()))
}

fn array_like_values(value: &Value) -> RuntimeResult<Vec<Value>> {
    match value {
        Value::String(string) => Ok(string
            .code_units()
            .iter()
            .map(|unit| Value::String(JsString::from_code_units(vec![*unit])))
            .collect()),
        Value::Object(object) => {
            let length = object
                .get(&PropertyKey::String(JsString::from("length")))?
                .to_number()?;
            let length = if length.is_finite() && length > 0.0 {
                length.min(u32::MAX as f64) as u32
            } else {
                0
            };
            (0..length)
                .map(|index| object.get(&index_key(index)))
                .collect()
        }
        _ => Err(RuntimeError::type_error("spread value is not iterable")),
    }
}
