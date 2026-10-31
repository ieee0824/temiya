//! Traced public instance field definitions and per-instance initialization.
use std::rc::Rc;

use super::{
    Environment, ExpressionEvaluator, IntrinsicId, JsString, Object, PropertyDescriptor,
    PropertyKey, RuntimeError, RuntimeResult, ThisBindingStatus, Value,
    environment::StoredEnvironment, execution::StoredRealm, stored::StoredObject,
};
use crate::ast::{Expr, ExprKind};

#[derive(Clone)]
pub(super) struct Field {
    key: PropertyKey,
    private: Option<super::private_fields::Name>,
    initializer: Option<StoredObject>,
}

// No function-local binding is reachable from these expressions. Property
// callbacks execute in their own contexts; eval, calls, super and escaping
// definitions retain the initializer's ordinary function environment.
pub(super) fn can_evaluate_without_function_environment(expression: &Expr) -> bool {
    use ExprKind as E;
    let simple = can_evaluate_without_function_environment;
    match &expression.kind {
        E::Literal(_) | E::This => true,
        E::Identifier(name) => !matches!(name.as_str(), "eval" | "arguments"),
        E::MetaProperty { meta, property } => meta == "new" && property == "target",
        E::Parenthesized(inner)
        | E::Unary {
            argument: inner, ..
        }
        | E::Update {
            argument: inner, ..
        } => simple(inner),
        E::Binary { left, right, .. } => simple(left) && simple(right),
        E::Assignment { target, value, .. } => simple(target) && simple(value),
        E::Conditional {
            test,
            consequent,
            alternate,
        } => simple(test) && simple(consequent) && simple(alternate),
        E::Sequence(expressions) => expressions.iter().all(simple),
        E::Member {
            object,
            property,
            computed,
            ..
        } => simple(object) && (!computed || simple(property)),
        _ => false,
    }
}
impl Field {
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        if let Some(initializer) = &self.initializer {
            initializer.trace(tracer);
        }
    }
}

pub(super) struct Initializer {
    expression: Rc<Expr>,
    // Immutable code cache only: it owns no Realm, environment or JS objects.
    // Each call still creates its own receiver/lexical environment.
    cache: Option<super::function::FunctionCache>,
    direct: bool,
    name: JsString,
    realm: StoredRealm,
    environment: StoredEnvironment,
    home: StoredObject,
}
impl Initializer {
    pub(super) fn source_realm(&self) -> &StoredRealm {
        &self.realm
    }
    pub(super) fn super_base(&self, owner: &Object) -> RuntimeResult<Value> {
        Ok(self
            .home
            .load(owner)?
            .try_prototype()?
            .map_or(Value::Null, Value::Object))
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.realm.trace(tracer);
        self.environment.trace(tracer);
        self.home.trace(tracer);
    }
    pub(super) fn call(&self, owner: &Object, this: Value) -> RuntimeResult<Value> {
        let realm = self.realm.load(owner)?;
        let captured = self.environment.load(owner)?;
        let evaluator = if self.direct {
            ExpressionEvaluator::with_environment(realm.clone(), captured, this, true)
        } else {
            let environment = Environment::function(
                Some(captured),
                owner.clone(),
                None,
                ThisBindingStatus::Uninitialized,
            );
            environment.bind_this_value(this.clone())?;
            ExpressionEvaluator::with_environment(realm.clone(), environment.clone(), this, true)
                .with_this_environment(Some(environment))
        };
        let evaluator = evaluator.with_function_cache(self.cache.clone());
        let value = evaluator
            .evaluate_named(&self.expression, &self.name)
            .map_err(|error| realm.bind_exception(error))?;
        // Anonymous function definitions receive the field name without creating
        // a named-function lexical binding. Classes use NamedEvaluation above.
        let mut expression = self.expression.as_ref();
        while let ExprKind::Parenthesized(inner) = &expression.kind {
            expression = inner;
        }
        if matches!(&expression.kind, ExprKind::Function(function) if function.name.is_none())
            || matches!(expression.kind, ExprKind::Arrow(_))
        {
            if let Value::Object(function) = &value {
                function.define_own_property(
                    "name".into(),
                    PropertyDescriptor::data(Value::String(self.name.clone()), false, false, true),
                )?;
            }
        }
        Ok(value)
    }
}

pub(super) fn initializer(
    home: &Object,
    evaluator: &ExpressionEvaluator,
    key: &PropertyKey,
    expression: &Expr,
) -> RuntimeResult<Object> {
    let function =
        Object::new_with_prototype(evaluator.realm.intrinsic(IntrinsicId::FunctionPrototype));
    let name = match key {
        PropertyKey::String(name) => name.clone(),
        PropertyKey::Symbol(symbol) => {
            symbol
                .description()
                .map_or_else(JsString::default, |description| {
                    let mut units = vec![u16::from(b'[')];
                    units.extend_from_slice(description.code_units());
                    units.push(u16::from(b']'));
                    JsString::from_code_units_slice(&units)
                })
        }
    };
    let initializer = Initializer {
        expression: Rc::new(expression.clone()),
        cache: evaluator.function_cache.clone(),
        direct: can_evaluate_without_function_environment(expression),
        name,
        realm: StoredRealm::store(&function, &evaluator.realm)?,
        environment: StoredEnvironment::store(&function, &evaluator.environment)?,
        home: StoredObject::store(&function, home)?,
    };
    function.set_managed_callable(super::object::ManagedCallable::FieldInitializer(
        initializer,
    ));
    Ok(function)
}

pub(super) fn add(
    constructor: &Object,
    home: &Object,
    evaluator: &ExpressionEvaluator,
    key: PropertyKey,
    expression: Option<&Expr>,
) -> RuntimeResult<()> {
    let initializer = expression
        .map(|expression| {
            let function = initializer(home, evaluator, &key, expression)?;
            StoredObject::store(constructor, &function)
        })
        .transpose()?;
    constructor
        .0
        .state
        .borrow_mut()
        .instance_fields
        .push(Field {
            key,
            private: None,
            initializer,
        });
    Ok(())
}

pub(super) fn add_private(
    constructor: &Object,
    home: &Object,
    evaluator: &ExpressionEvaluator,
    name: &str,
    expression: Option<&Expr>,
) -> RuntimeResult<()> {
    let key = PropertyKey::from(format!("#{name}").as_str());
    let private = Some(evaluator.environment.resolve_private(name)?);
    let initializer = expression
        .map(|expression| {
            let function = initializer(home, evaluator, &key, expression)?;
            StoredObject::store(constructor, &function)
        })
        .transpose()?;
    constructor
        .0
        .state
        .borrow_mut()
        .instance_fields
        .push(Field {
            key,
            private,
            initializer,
        });
    Ok(())
}

pub(super) fn add_private_method(
    constructor: &Object,
    name: super::private_fields::Name,
    value: &Object,
) -> RuntimeResult<()> {
    let value = StoredObject::store(constructor, value)?;
    constructor
        .0
        .state
        .borrow_mut()
        .instance_fields
        .push(Field {
            key: PropertyKey::from(""),
            private: Some(name),
            initializer: Some(value),
        });
    Ok(())
}

impl Object {
    pub(super) fn initialize_instance_fields(&self, receiver: &Object) -> RuntimeResult<()> {
        {
            let state = self.0.state.borrow();
            // Default private slots neither invoke JS nor allocate managed heap
            // objects. Avoid cloning the definition list in this case. A
            // stamped receiver can be the constructor itself, so keep that
            // alias on the snapshot path before borrowing its state mutably.
            if self != receiver
                && state.instance_fields.iter().all(|field| {
                    field
                        .private
                        .as_ref()
                        .is_some_and(|name| name.kind() == super::private_fields::Kind::Field)
                        && field.initializer.is_none()
                })
            {
                for field in &state.instance_fields {
                    receiver.add_private(
                        field.private.as_ref().expect("private field").clone(),
                        Value::Undefined,
                    )?;
                }
                return Ok(());
            }
        }
        // Release the state borrow before invoking an initializer or a Proxy
        // defineProperty trap. Snapshot stored edges while self roots the list.
        let fields = self.0.state.borrow().instance_fields.clone();
        // Private methods/accessors are shared class values installed before
        // any field initializer, regardless of their declaration position.
        for field in &fields {
            if let Some(name) = field
                .private
                .as_ref()
                .filter(|name| name.kind() != super::private_fields::Kind::Field)
            {
                let value = field
                    .initializer
                    .as_ref()
                    .expect("private method value")
                    .load(self)?;
                receiver.add_private(name.clone(), Value::Object(value))?;
            }
        }
        for field in fields {
            if field
                .private
                .as_ref()
                .is_some_and(|name| name.kind() != super::private_fields::Kind::Field)
            {
                continue;
            }
            let value = match field.initializer {
                Some(initializer) => initializer
                    .load(self)?
                    .call(Value::Object(receiver.clone()), &[])?,
                None => Value::Undefined,
            };
            if let Some(name) = field.private {
                receiver.add_private(name, value)?;
                continue;
            }
            if !receiver
                .define_own_property(field.key, PropertyDescriptor::data(value, true, true, true))?
            {
                return Err(RuntimeError::type_error("cannot define instance field"));
            }
        }
        Ok(())
    }
}
