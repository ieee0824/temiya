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
    initializer: Option<StoredObject>,
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
        let environment = Environment::function(
            Some(self.environment.load(owner)?),
            owner.clone(),
            None,
            ThisBindingStatus::Uninitialized,
        );
        environment.bind_this_value(this.clone())?;
        let evaluator =
            ExpressionEvaluator::with_environment(realm.clone(), environment.clone(), this, true)
                .with_this_environment(Some(environment));
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

pub(super) fn add(
    constructor: &Object,
    home: &Object,
    evaluator: &ExpressionEvaluator,
    key: PropertyKey,
    expression: Option<&Expr>,
) -> RuntimeResult<()> {
    let initializer = expression
        .map(|expression| {
            let function = Object::new_with_prototype(
                evaluator.realm.intrinsic(IntrinsicId::FunctionPrototype),
            );
            let name = match &key {
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
                name,
                realm: StoredRealm::store(&function, &evaluator.realm)?,
                environment: StoredEnvironment::store(&function, &evaluator.environment)?,
                home: StoredObject::store(&function, home)?,
            };
            function.set_managed_callable(super::object::ManagedCallable::FieldInitializer(
                initializer,
            ));
            StoredObject::store(constructor, &function)
        })
        .transpose()?;
    constructor
        .0
        .state
        .borrow_mut()
        .instance_fields
        .push(Field { key, initializer });
    Ok(())
}

impl Object {
    pub(super) fn initialize_instance_fields(&self, receiver: &Object) -> RuntimeResult<()> {
        // Release the state borrow before invoking an initializer or a Proxy
        // defineProperty trap. Snapshot stored edges while self roots the list.
        let fields = self.0.state.borrow().instance_fields.clone();
        for field in fields {
            let value = match field.initializer {
                Some(initializer) => initializer
                    .load(self)?
                    .call(Value::Object(receiver.clone()), &[])?,
                None => Value::Undefined,
            };
            if !receiver
                .define_own_property(field.key, PropertyDescriptor::data(value, true, true, true))?
            {
                return Err(RuntimeError::type_error("cannot define instance field"));
            }
        }
        Ok(())
    }
}
