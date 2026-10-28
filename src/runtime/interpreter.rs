//! Direct expression evaluation and ECMAScript Reference Records.

use num_bigint::BigInt;
use std::cell::RefCell;

use crate::ast::{Expr, ExprKind, Literal, MethodKind, NumberKind};
use crate::bytecode::BinaryOperator;

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
    /// An object-valued computed key using a Realm's @@toPrimitive symbol.
    Computed {
        /// Original key value retained until GetValue/PutValue/Delete.
        value: Value,
        /// Symbol identity to use for JavaScript key coercion.
        to_primitive: super::Symbol,
    },
}

impl ReferencedName {
    fn property_key(&self) -> RuntimeResult<PropertyKey> {
        match self {
            Self::PropertyKey(key) => Ok(key.clone()),
            Self::Value(value) => value.to_property_key(),
            Self::Computed {
                value,
                to_primitive,
            } => {
                let primitive = super::coercion::primitive_with_hint(
                    value,
                    &PropertyKey::Symbol(to_primitive.clone()),
                    super::PrimitiveHint::String,
                )?;
                primitive.to_property_key()
            }
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
    FunctionBinding(Environment, usize),
    Property(Value),
    Unresolvable(Object),
}

#[cfg(test)]
mod function_reference_tests {
    use super::*;
    use crate::runtime::ThisBindingStatus;

    #[test]
    fn function_hint_does_not_enlarge_reference_base() {
        #[allow(dead_code)]
        enum PreviousBase {
            Environment(Environment),
            Property(Value),
            Unresolvable(Object),
        }
        assert_eq!(
            std::mem::size_of::<ReferenceBase>(),
            std::mem::size_of::<PreviousBase>()
        );
    }

    #[test]
    fn retained_function_hint_survives_binding_vector_changes() {
        let environment =
            Environment::function(None, Object::new(), None, ThisBindingStatus::Uninitialized);
        let prefix: JsString = "prefix".into();
        let value: JsString = "value".into();
        environment
            .create_mutable_binding(prefix.clone(), true)
            .unwrap();
        environment
            .initialize_binding(&prefix, Value::Null)
            .unwrap();
        environment
            .create_mutable_binding(value.clone(), false)
            .unwrap();
        environment
            .initialize_binding(&value, Value::Number(41.0))
            .unwrap();
        let reference = Reference::function_binding(
            environment.clone(),
            value.clone(),
            false,
            environment.function_binding_index(&value),
        );
        assert!(environment.delete_binding(&prefix).unwrap());
        let other: JsString = "other".into();
        environment
            .create_mutable_binding(other.clone(), false)
            .unwrap();
        environment
            .initialize_binding(&other, Value::Number(99.0))
            .unwrap();
        assert!(
            reference
                .get_value()
                .unwrap()
                .same_value(&Value::Number(41.0))
        );
        reference.put_value(Value::Number(42.0)).unwrap();
        assert!(
            environment
                .get_binding_value(&value, false)
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert!(
            environment
                .get_binding_value(&other, false)
                .unwrap()
                .same_value(&Value::Number(99.0))
        );
    }
}

// Primitive names need no observable conversion state. Only object-valued
// computed names allocate a mutable cell, keeping ordinary environment/static
// property references free of dynamic borrows. Cloning copies the Record state.
#[derive(Clone, Debug)]
enum ReferenceName {
    Plain(ReferencedName),
    Computed(Box<RefCell<ReferencedName>>),
}

impl ReferenceName {
    fn new(name: ReferencedName) -> Self {
        if matches!(
            name,
            ReferencedName::Computed { .. } | ReferencedName::Value(Value::Object(_))
        ) {
            Self::Computed(Box::new(RefCell::new(name)))
        } else {
            Self::Plain(name)
        }
    }

    fn property_key(&self) -> RuntimeResult<PropertyKey> {
        let Self::Computed(cell) = self else {
            let Self::Plain(name) = self else {
                unreachable!()
            };
            return name.property_key();
        };
        let name = cell.borrow();
        if let ReferencedName::PropertyKey(key) = &*name {
            return Ok(key.clone());
        }
        let unresolved = name.clone();
        drop(name);
        // ToPropertyKey can run arbitrary/reentrant JavaScript. Store the key
        // before getter/PutValue and release the original key object now.
        let key = unresolved.property_key()?;
        *cell.borrow_mut() = ReferencedName::PropertyKey(key.clone());
        Ok(key)
    }

    fn binding_name(&self) -> RuntimeResult<JsString> {
        match self {
            Self::Plain(name) => name.binding_name(),
            Self::Computed(_) => match self.property_key()? {
                PropertyKey::String(name) => Ok(name),
                PropertyKey::Symbol(_) => Err(RuntimeError::reference_error(
                    "environment name cannot be a Symbol",
                )),
            },
        }
    }
}

/// An ECMAScript Reference Record used by identifier and property expressions.
#[derive(Clone, Debug)]
pub struct Reference {
    base: ReferenceBase,
    name: ReferenceName,
    strict: bool,
    this_value: Option<Value>,
    primitive_prototype: Option<Object>,
}

impl Reference {
    // Only VM-verified function slots provide hints. Each read/write validates
    // the name again, so reentrant deletion or insertion cannot retarget it.
    pub(crate) fn function_binding(
        environment: Environment,
        name: JsString,
        strict: bool,
        index: Option<usize>,
    ) -> Self {
        Self {
            base: match index {
                Some(index) => ReferenceBase::FunctionBinding(environment, index),
                None => ReferenceBase::Environment(environment),
            },
            name: ReferenceName::Plain(ReferencedName::PropertyKey(PropertyKey::String(name))),
            strict,
            this_value: None,
            primitive_prototype: None,
        }
    }

    /// Creates a resolved environment reference.
    #[must_use]
    pub fn environment(environment: Environment, name: JsString, strict: bool) -> Self {
        Self {
            base: ReferenceBase::Environment(environment),
            name: ReferenceName::Plain(ReferencedName::PropertyKey(PropertyKey::String(name))),
            strict,
            this_value: None,
            primitive_prototype: None,
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
            name: ReferenceName::new(name),
            strict,
            this_value,
            primitive_prototype: None,
        }
    }

    /// Creates a property Reference with primitive prototype fallback for this Realm.
    /// Null checks and key coercion occur when the Reference is consumed.
    pub(crate) fn property_in_realm(
        realm: &Realm,
        base: Value,
        name: ReferencedName,
        strict: bool,
    ) -> Self {
        let prototype = match &base {
            Value::String(_) => Some(super::IntrinsicId::StringPrototype),
            Value::Number(_) => Some(super::IntrinsicId::NumberPrototype),
            Value::Boolean(_) => Some(super::IntrinsicId::BooleanPrototype),
            Value::BigInt(_) => Some(super::IntrinsicId::BigIntPrototype),
            Value::Symbol(_) => Some(super::IntrinsicId::SymbolPrototype),
            _ => None,
        };
        let name = match name {
            ReferencedName::Value(value @ Value::Object(_)) => ReferencedName::Computed {
                value,
                to_primitive: realm.well_known_symbol(super::WellKnownSymbol::ToPrimitive),
            },
            name => name,
        };
        let mut reference = Self::property(base, name, strict, None);
        reference.primitive_prototype = prototype.map(|id| realm.intrinsic(id));
        reference
    }

    /// Creates an unresolvable reference associated with a Realm global object.
    #[must_use]
    pub fn unresolvable(global_object: Object, name: JsString, strict: bool) -> Self {
        Self {
            base: ReferenceBase::Unresolvable(global_object),
            name: ReferenceName::Plain(ReferencedName::PropertyKey(PropertyKey::String(name))),
            strict,
            this_value: None,
            primitive_prototype: None,
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
            ReferenceBase::Environment(environment)
            | ReferenceBase::FunctionBinding(environment, _) => {
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
            ReferenceBase::FunctionBinding(environment, index) => environment
                .get_function_binding_at(&self.name.binding_name()?, Some(*index), self.strict),
            ReferenceBase::Unresolvable(_) => Err(RuntimeError::reference_error(format!(
                "{} is not defined",
                self.name.binding_name()?.to_string_lossy()
            ))),
            ReferenceBase::Property(Value::Undefined | Value::Null) => Err(
                RuntimeError::type_error("cannot access a property of null or undefined"),
            ),
            ReferenceBase::Property(base) => get_property(
                base,
                &self.name.property_key()?,
                self.primitive_prototype.as_ref(),
            ),
        }
    }

    /// Implements `PutValue`, including sloppy assignment to the global object.
    pub fn put_value(&self, value: Value) -> RuntimeResult<()> {
        match &self.base {
            ReferenceBase::Environment(environment) => {
                environment.set_mutable_binding(&self.name.binding_name()?, value, self.strict)
            }
            ReferenceBase::FunctionBinding(environment, index) => environment
                .set_function_binding_at(
                    &self.name.binding_name()?,
                    Some(*index),
                    value,
                    self.strict,
                ),
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
            ReferenceBase::Property(Value::Undefined | Value::Null) => Err(
                RuntimeError::type_error("cannot assign a property of null or undefined"),
            ),
            ReferenceBase::Property(base) => {
                let key = self.name.property_key()?;
                let succeeded = set_property(base, &key, value, self.primitive_prototype.as_ref())?;
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
            ReferenceBase::Environment(environment)
            | ReferenceBase::FunctionBinding(environment, _) => {
                if self.strict {
                    Err(RuntimeError::syntax_error(
                        "cannot delete an unqualified identifier in strict mode",
                    ))
                } else {
                    environment.delete_binding(&self.name.binding_name()?)
                }
            }
            ReferenceBase::Property(Value::Undefined | Value::Null) => Err(
                RuntimeError::type_error("cannot delete a property of null or undefined"),
            ),
            ReferenceBase::Property(base) => {
                let key = self.name.property_key()?;
                let deleted = match base {
                    Value::Object(object) => object.try_delete(&key)?,
                    Value::String(string) => {
                        !(key == PropertyKey::from("length")
                            || key
                                .array_index()
                                .is_some_and(|index| (index as usize) < string.len()))
                    }
                    _ => true,
                };
                if !deleted && self.strict {
                    Err(RuntimeError::type_error("property deletion was rejected"))
                } else {
                    Ok(deleted)
                }
            }
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
        let this_value = realm
            .global_environment()
            .global_this_value()
            .expect("Realm has a global environment");
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
            ExprKind::Literal(literal) => self.literal(literal).map(Evaluation::Value),
            ExprKind::This => Ok(Evaluation::Value(self.this_value.clone())),
            ExprKind::Parenthesized(inner) => match self.eval(inner)? {
                Evaluation::ShortCircuit => Ok(Evaluation::Value(Value::Undefined)),
                result => Ok(result),
            },
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
        let name = if computed {
            ReferencedName::Value(self.evaluate(property)?)
        } else {
            ReferencedName::PropertyKey(PropertyKey::String(member_name(property)?))
        };
        Ok(Evaluation::Reference(Reference::property_in_realm(
            &self.realm,
            base,
            name,
            self.strict,
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
        let arguments = self.arguments(arguments)?;
        let Value::Object(function) = function else {
            return Err(RuntimeError::type_error("value is not callable"));
        };
        if !function.is_callable() {
            return Err(RuntimeError::type_error("value is not callable"));
        }
        function.call(this_value, &arguments).map(Evaluation::Value)
    }

    fn construct(&self, callee: &Expr, arguments: &[Expr]) -> RuntimeResult<Value> {
        let constructor = self.evaluate(callee)?;
        let arguments = self.arguments(arguments)?;
        let Value::Object(constructor) = constructor else {
            return Err(RuntimeError::type_error("value is not a constructor"));
        };
        if !constructor.is_constructor() {
            return Err(RuntimeError::type_error("value is not a constructor"));
        }
        Ok(Value::Object(
            constructor.construct(&arguments, constructor.clone())?,
        ))
    }

    fn arguments(&self, arguments: &[Expr]) -> RuntimeResult<Vec<Value>> {
        let mut values = Vec::new();
        for argument in arguments {
            if let ExprKind::Spread(value) = &argument.kind {
                super::iterator::spread_arguments(&self.realm, self.evaluate(value)?, &mut values)?;
            } else {
                super::iterator::push_argument(&mut values, self.evaluate(argument)?)?;
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
            "+" | "-" | "~" => unary_numeric_value(Some(&self.realm), operator, value),
            "await" => super::promise::await_value(value),
            _ => Err(RuntimeError::syntax_error("unknown unary operator")),
        }
    }

    fn update(&self, operator: &str, argument: &Expr, prefix: bool) -> RuntimeResult<Value> {
        let reference = self.evaluate_reference(argument)?;
        update_reference_in_realm(&self.realm, &reference, operator == "++", prefix)
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
        self.apply_binary(operator, left, right)
    }

    #[inline]
    fn apply_binary(&self, operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
        binary_values_in_realm(&self.realm, operator, left, right)
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
            self.apply_binary(operator.strip_suffix('=').unwrap_or(operator), left, right)?
        };
        reference.put_value(assigned.clone())?;
        Ok(assigned)
    }

    fn array(&self, elements: &[Option<Expr>]) -> RuntimeResult<Value> {
        let object = super::array_literal::create(&self.realm)?;
        let mut next = 0;
        for element in elements {
            match element {
                Some(Expr {
                    kind: ExprKind::Spread(value),
                    ..
                }) => super::array_literal::spread(
                    &self.realm,
                    &object,
                    &mut next,
                    self.evaluate(value)?,
                )?,
                Some(value) => {
                    super::array_literal::append(&object, &mut next, self.evaluate(value)?)?
                }
                None => super::array_literal::elide(&mut next)?,
            }
        }
        super::array_literal::finish(&object, next)?;
        Ok(Value::Object(object))
    }

    fn literal(&self, literal: &Literal) -> RuntimeResult<Value> {
        match literal {
            Literal::RegExp(spelling) => {
                let prototype = self.realm.intrinsic(super::IntrinsicId::RegExpPrototype);
                let regexp = super::RegExp::from_literal_for(&prototype, spelling)?;
                regexp.object().initialize_prototype(Some(prototype));
                Ok(Value::Object(regexp.object()))
            }
            _ => literal_value(literal, self.strict),
        }
    }

    fn object(&self, properties: &[crate::ast::ObjectProperty]) -> RuntimeResult<Value> {
        let object = super::object_literal::create(&self.realm);
        for property in properties {
            if property.spread {
                super::object_literal::spread(&object, self.evaluate(&property.key)?)?;
                continue;
            }
            let key = if property.computed
                || matches!(property.key.kind, ExprKind::Literal(Literal::Number { .. }))
            {
                super::object_literal::property_key(&self.realm, self.evaluate(&property.key)?)?
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
                let prototype_setter = !property.computed
                    && !property.shorthand
                    && key == PropertyKey::from("__proto__");
                super::object_literal::define_data(&object, key, value, prototype_setter)?;
            }
        }
        Ok(Value::Object(object))
    }
}

fn get_property(
    base: &Value,
    key: &PropertyKey,
    primitive_prototype: Option<&Object>,
) -> RuntimeResult<Value> {
    match base {
        Value::Object(object) => object.get_with_receiver(key, base.clone()),
        Value::String(string) => {
            if matches!(key, PropertyKey::String(name) if name.code_units().iter().copied().eq("length".encode_utf16()))
            {
                return Ok(Value::Number(string.len() as f64));
            }
            if let Some(unit) = key
                .array_index()
                .and_then(|index| string.code_units().get(index as usize))
            {
                return Ok(Value::String(JsString::from_code_units_slice(&[*unit])));
            }
            primitive_prototype.map_or(Ok(Value::Undefined), |prototype| {
                prototype.get_with_receiver(key, base.clone())
            })
        }
        Value::Undefined | Value::Null => Err(RuntimeError::type_error(
            "cannot access a property of null or undefined",
        )),
        _ => primitive_prototype.map_or(Ok(Value::Undefined), |prototype| {
            prototype.get_with_receiver(key, base.clone())
        }),
    }
}

fn set_property(
    base: &Value,
    key: &PropertyKey,
    value: Value,
    primitive_prototype: Option<&Object>,
) -> RuntimeResult<bool> {
    match base {
        Value::Object(object) => object.set_with_receiver(key, value, base.clone()),
        Value::Undefined | Value::Null => Err(RuntimeError::type_error(
            "cannot assign a property of null or undefined",
        )),
        Value::String(string)
            if key == &PropertyKey::from("length")
                || key
                    .array_index()
                    .is_some_and(|index| (index as usize) < string.len()) =>
        {
            Ok(false)
        }
        _ => primitive_prototype.map_or(Ok(false), |prototype| {
            prototype.set_with_receiver(key, value, base.clone())
        }),
    }
}

fn member_name(expression: &Expr) -> RuntimeResult<JsString> {
    match &expression.kind {
        ExprKind::Identifier(name) | ExprKind::PrivateIdentifier(name) => {
            Ok(JsString::from(name.as_str()))
        }
        ExprKind::Literal(Literal::String { value, .. }) => {
            Ok(JsString::from_code_units_slice(value))
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
        Literal::String { value, .. } => Ok(Value::String(JsString::from_code_units_slice(value))),
        Literal::RegExp(_) => unreachable!("RegExp literals require a Realm"),
        Literal::Number { raw, kind } => parse_numeric_literal(raw, *kind),
    }
}

pub(crate) fn parse_numeric_literal(raw: &str, kind: NumberKind) -> RuntimeResult<Value> {
    let cleaned = if raw.as_bytes().contains(&b'_') {
        std::borrow::Cow::Owned(raw.replace('_', ""))
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
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

// Shared primitive-Number semantics; the VM already has a typed operator.
#[inline]
pub(crate) fn binary_number(operator: BinaryOperator, a: f64, b: f64) -> Option<Value> {
    match operator {
        BinaryOperator::Add => Some(Value::Number(a + b)),
        BinaryOperator::Subtract => Some(Value::Number(a - b)),
        BinaryOperator::Multiply => Some(Value::Number(a * b)),
        BinaryOperator::Divide => Some(Value::Number(a / b)),
        BinaryOperator::Remainder => Some(Value::Number(number_remainder(a, b))),
        BinaryOperator::BitAnd => Some(Value::Number(f64::from(
            number_operand_u32(a) as i32 & number_operand_u32(b) as i32,
        ))),
        BinaryOperator::BitOr => Some(Value::Number(f64::from(
            number_operand_u32(a) as i32 | number_operand_u32(b) as i32,
        ))),
        BinaryOperator::BitXor => Some(Value::Number(f64::from(
            number_operand_u32(a) as i32 ^ number_operand_u32(b) as i32,
        ))),
        BinaryOperator::LeftShift => Some(Value::Number(f64::from(
            (number_operand_u32(a) as i32).wrapping_shl(number_operand_u32(b) & 31),
        ))),
        BinaryOperator::RightShift => Some(Value::Number(f64::from(
            (number_operand_u32(a) as i32).wrapping_shr(number_operand_u32(b) & 31),
        ))),
        BinaryOperator::UnsignedRightShift => Some(Value::Number(f64::from(
            number_operand_u32(a).wrapping_shr(number_operand_u32(b) & 31),
        ))),
        BinaryOperator::Less => Some(Value::Boolean(a < b)),
        BinaryOperator::Greater => Some(Value::Boolean(a > b)),
        BinaryOperator::LessEqual => Some(Value::Boolean(a <= b)),
        BinaryOperator::GreaterEqual => Some(Value::Boolean(a >= b)),
        BinaryOperator::Equal | BinaryOperator::StrictEqual => Some(Value::Boolean(a == b)),
        BinaryOperator::NotEqual | BinaryOperator::StrictNotEqual => Some(Value::Boolean(a != b)),
        _ => None,
    }
}

pub(crate) fn binary_values(operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
    // Primitive Numbers require no coercion or observable callback. Keep the
    // general path for every other pair, including mixed Number/BigInt values.
    if let (Value::Number(a), Value::Number(b)) = (&left, &right) {
        let result = BinaryOperator::from_spelling(operator)
            .and_then(|operator| binary_number(operator, *a, *b));
        if let Some(result) = result {
            return Ok(result);
        }
    } else if operator == "+" && matches!((&left, &right), (Value::String(_), Value::String(_))) {
        // Both operands are already primitive strings. Consume them without
        // cloning through ToPrimitive and ToString; mixed pairs keep that path.
        let (Value::String(left), Value::String(right)) = (left, right) else {
            unreachable!()
        };
        return Ok(Value::String(left.concat(right)));
    }
    match operator {
        "+" => {
            let left = left.to_primitive(super::PrimitiveHint::Default)?;
            let right = right.to_primitive(super::PrimitiveHint::Default)?;
            if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
                let left = left.to_js_string()?;
                let right = right.to_js_string()?;
                return Ok(Value::String(left.concat(right)));
            }
            numeric_binary(operator, left, right)
        }
        "-" | "*" | "/" | "%" | "**" | "&" | "|" | "^" | "<<" | ">>" | ">>>" => {
            numeric_binary(operator, left, right)
        }
        "===" => Ok(Value::Boolean(strict_equal(&left, &right))),
        "!==" => Ok(Value::Boolean(!strict_equal(&left, &right))),
        "==" => Ok(Value::Boolean(super::operators::equal(None, left, right)?)),
        "!=" => Ok(Value::Boolean(!super::operators::equal(None, left, right)?)),
        "<" | ">" | "<=" | ">=" => super::operators::relational(None, operator, left, right),
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
        "instanceof" => super::operators::instance_of(left, right, None).map(Value::Boolean),
        "&&" | "||" | "??" => Ok(right),
        _ => Err(RuntimeError::syntax_error(format!(
            "unsupported binary operator {operator}"
        ))),
    }
}

fn number_operand_u32(value: f64) -> u32 {
    // Already-primitive Numbers need no observable coercion. In this bounded
    // interval truncation fits i64 and retaining low bits is exactly ToUint32.
    // Nonfinite and larger inputs retain the existing general conversion.
    const MODULUS: f64 = 4_294_967_296.0;
    if (-MODULUS..MODULUS).contains(&value) {
        (value as i64) as u32
    } else {
        to_u32(value)
    }
}

fn number_remainder(left: f64, right: f64) -> f64 {
    // In this range integral operands and their remainder are exact binary64
    // integers, and signed i64 division cannot overflow. All other pairs keep
    // IEEE remainder semantics, including zero divisors and non-finite values.
    const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_991.0;
    if left.abs() <= MAX_EXACT_INTEGER && right.abs() <= MAX_EXACT_INTEGER {
        let dividend = left as i64;
        let divisor = right as i64;
        if divisor != 0 && dividend as f64 == left && divisor as f64 == right {
            return ((dividend % divisor) as f64).copysign(left);
        }
    }
    left % right
}

fn numeric_binary(operator: &str, left: Value, right: Value) -> RuntimeResult<Value> {
    numeric_binary_result(operator, left.to_numeric()?, right.to_numeric()?)
}

pub(super) fn numeric_binary_result(
    operator: &str,
    left: Numeric,
    right: Numeric,
) -> RuntimeResult<Value> {
    match (left, right) {
        (Numeric::Number(a), Numeric::Number(b)) => Ok(Value::Number(match operator {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => a / b,
            "%" => a % b,
            "**" => super::operators::number_power(a, b),
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
                    if b < BigInt::from(0) {
                        return Err(RuntimeError::range_error("negative BigInt exponent"));
                    }
                    if b == BigInt::from(0) {
                        BigInt::from(1)
                    } else if a == BigInt::from(0) || a == BigInt::from(1) {
                        a
                    } else if a == BigInt::from(-1) {
                        if (&b & BigInt::from(1)) == BigInt::from(0) {
                            BigInt::from(1)
                        } else {
                            BigInt::from(-1)
                        }
                    } else {
                        a.pow(bigint_shift(&b)?)
                    }
                }
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" => {
                    if b < BigInt::from(0) {
                        super::operators::right_shift(a, -b)?
                    } else {
                        a << bigint_shift(&b)?
                    }
                }
                ">>" => {
                    if b < BigInt::from(0) {
                        a << bigint_shift(&(-b))?
                    } else {
                        super::operators::right_shift(a, b)?
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

pub(crate) fn type_of(value: &Value) -> &'static str {
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
#[inline]
pub(crate) fn binary_values_in_realm(
    realm: &Realm,
    operator: &str,
    left: Value,
    right: Value,
) -> RuntimeResult<Value> {
    if matches!(left, Value::Object(_)) || matches!(right, Value::Object(_)) {
        return super::operators::object_binary(realm, operator, left, right);
    }
    binary_values(operator, left, right)
}

/// Performs the complete update on one retained Reference, including Realm-aware
/// ToPrimitive(Number), ToNumeric, PutValue and numeric postfix results.
pub(crate) fn update_reference_in_realm(
    realm: &Realm,
    reference: &Reference,
    increment: bool,
    prefix: bool,
) -> RuntimeResult<Value> {
    let old = reference.get_value()?;
    let old = if matches!(old, Value::Object(_)) {
        super::coercion::primitive(
            &old,
            &realm
                .well_known_symbol(super::WellKnownSymbol::ToPrimitive)
                .into(),
            false,
        )?
        .to_numeric()?
    } else {
        old.to_numeric()?
    };
    let (old, new) = match old {
        Numeric::Number(value) => (
            Value::Number(value),
            Value::Number(if increment { value + 1.0 } else { value - 1.0 }),
        ),
        Numeric::BigInt(value) => {
            let next = if increment { &value + 1 } else { &value - 1 };
            (Value::BigInt(value), Value::BigInt(next))
        }
    };
    reference.put_value(new.clone())?;
    Ok(if prefix { new } else { old })
}

/// Shared numeric unary operation; primitive operands do not need a Realm.
pub(crate) fn unary_numeric_value(
    realm: Option<&Realm>,
    operator: &str,
    value: Value,
) -> RuntimeResult<Value> {
    let primitive = if matches!(value, Value::Object(_)) {
        let realm = realm
            .ok_or_else(|| RuntimeError::internal_error("object unary operation requires Realm"))?;
        super::coercion::primitive(
            &value,
            &realm
                .well_known_symbol(super::WellKnownSymbol::ToPrimitive)
                .into(),
            false,
        )?
    } else {
        value
    };
    if operator == "+" {
        return primitive.to_number().map(Value::Number);
    }
    match (operator, primitive.to_numeric()?) {
        ("-", Numeric::Number(value)) => Ok(Value::Number(-value)),
        ("-", Numeric::BigInt(value)) => Ok(Value::BigInt(-value)),
        ("~", Numeric::Number(value)) => Ok(Value::Number(f64::from(!to_i32(value)))),
        ("~", Numeric::BigInt(value)) => Ok(Value::BigInt(!value)),
        _ => Err(RuntimeError::internal_error(
            "invalid numeric unary operator",
        )),
    }
}
