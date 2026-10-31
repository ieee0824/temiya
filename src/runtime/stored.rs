//! Non-owning heap edges. Public Values/Objects remain owning rooted facades.
use super::{
    JsString, Object, PropertyDescriptor, Runtime, RuntimeError, RuntimeResult, Symbol, Value,
    managed::ManagedObject,
};
use crate::gc::{Handle, Trace, Tracer};
use num_bigint::BigInt;

#[derive(Clone)]
pub(super) enum StoredRuntimeError {
    Native {
        kind: super::RuntimeErrorKind,
        message: super::ErrorMessage,
        thrown: Option<StoredValue>,
    },
    Thrown(StoredValue),
}
impl StoredRuntimeError {
    pub(super) fn store(owner: &impl StoredOwner, error: RuntimeError) -> RuntimeResult<Self> {
        Ok(match error.0 {
            super::RuntimeErrorData::Thrown(value) => {
                Self::Thrown(StoredValue::store_owned(owner, value)?)
            }
            super::RuntimeErrorData::Native(error) => Self::Native {
                kind: error.kind,
                message: error.message,
                thrown: error
                    .thrown
                    .map(|value| StoredValue::store_owned(owner, *value))
                    .transpose()?,
            },
        })
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<RuntimeError> {
        Ok(match self {
            Self::Thrown(value) => RuntimeError::thrown(value.load(owner)?),
            Self::Native {
                kind,
                message,
                thrown,
            } => RuntimeError::native(
                *kind,
                message.clone(),
                thrown
                    .as_ref()
                    .map(|value| value.load(owner).map(Box::new))
                    .transpose()?,
            ),
        })
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        match self {
            Self::Thrown(value) => value.trace(tracer),
            Self::Native {
                thrown: Some(value),
                ..
            } => value.trace(tracer),
            _ => {}
        }
    }
}

// An active facade supplies context; stored records never own this facade.
// Typed owner handles let environments/realms use the same barrier protocol.
pub(super) trait StoredOwner {
    type Allocation: Trace;
    fn runtime(&self) -> Option<&Runtime>;
    fn handle(&self) -> Option<Handle<Self::Allocation>>;
}
impl StoredOwner for Object {
    type Allocation = ManagedObject;
    fn runtime(&self) -> Option<&Runtime> {
        self.0.runtime()
    }
    fn handle(&self) -> Option<Handle<ManagedObject>> {
        self.0.handle()
    }
}

#[derive(Clone, Debug)]
pub(super) struct StoredObject(pub(super) Handle<ManagedObject>);
impl StoredObject {
    // For edges prepared before allocation of their new, young owner.
    pub(super) fn initial(runtime: Option<&Runtime>, value: &Object) -> RuntimeResult<Self> {
        match (runtime, value.0.runtime()) {
            (Some(left), Some(right)) if left.same_heap(right) => {
                Ok(Self(value.0.handle().expect("managed object handle")))
            }
            _ => Err(RuntimeError::type_error(
                "object belongs to a different allocation context",
            )),
        }
    }
    pub(super) fn check_context(owner: &impl StoredOwner, value: &Object) -> RuntimeResult<()> {
        match (owner.runtime(), value.0.runtime()) {
            (Some(left), Some(right)) if left.same_heap(right) => Ok(()),
            _ => Err(RuntimeError::type_error(
                "object belongs to a different allocation context",
            )),
        }
    }
    pub(super) fn store(owner: &impl StoredOwner, value: &Object) -> RuntimeResult<Self> {
        match (owner.runtime(), value.0.runtime()) {
            (Some(left), Some(right)) if left.same_heap(right) => {
                let target = value.0.handle().expect("managed object handle");
                left.record_object_edge(owner.handle().expect("managed owner handle"), target)?;
                Ok(Self(target))
            }
            _ => Err(RuntimeError::type_error(
                "object belongs to a different allocation context",
            )),
        }
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<Object> {
        match owner.runtime() {
            Some(runtime) => runtime
                .root_object(self.0)
                .ok_or_else(|| RuntimeError::internal_error("stale managed object edge")),
            _ => Err(RuntimeError::internal_error(
                "object edge allocation context mismatch",
            )),
        }
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        tracer.mark(self.0);
    }
}

#[derive(Clone, Debug)]
pub(super) enum StoredValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(JsString),
    Symbol(Symbol),
    BigInt(BigInt),
    Object(StoredObject),
}
impl StoredValue {
    pub(super) fn into_value(self, owner: &impl StoredOwner) -> RuntimeResult<Value> {
        Ok(match self {
            Self::Undefined => Value::Undefined,
            Self::Null => Value::Null,
            Self::Boolean(value) => Value::Boolean(value),
            Self::Number(value) => Value::Number(value),
            Self::String(value) => Value::String(value),
            Self::Symbol(value) => Value::Symbol(value),
            Self::BigInt(value) => Value::BigInt(value),
            Self::Object(value) => Value::Object(value.load(owner)?),
        })
    }
    // Collection scans compare stored keys directly, without registering roots
    // or materializing Object facades for each candidate.
    pub(super) fn same_value_zero(&self, value: &Value) -> bool {
        match (self, value) {
            (Self::Undefined, Value::Undefined) | (Self::Null, Value::Null) => true,
            (Self::Boolean(a), Value::Boolean(b)) => a == b,
            (Self::Number(a), Value::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
            (Self::String(a), Value::String(b)) => a == b,
            (Self::Symbol(a), Value::Symbol(b)) => a == b,
            (Self::BigInt(a), Value::BigInt(b)) => a == b,
            (Self::Object(StoredObject(a)), Value::Object(b)) => Some(*a) == b.0.handle(),
            _ => false,
        }
    }
    pub(super) fn initial(runtime: Option<&Runtime>, value: &Value) -> RuntimeResult<Self> {
        Ok(match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Boolean(v) => Self::Boolean(*v),
            Value::Number(v) => Self::Number(*v),
            Value::String(v) => Self::String(v.clone()),
            Value::Symbol(v) => Self::Symbol(v.clone()),
            Value::BigInt(v) => Self::BigInt(v.clone()),
            Value::Object(v) => Self::Object(StoredObject::initial(runtime, v)?),
        })
    }
    // Owned binding writes move leaf payloads instead of cloning strings/BigInts.
    // A managed owner keeps Runtime alive while the incoming facade is released.
    pub(super) fn store_owned(owner: &impl StoredOwner, value: Value) -> RuntimeResult<Self> {
        Ok(match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Number(v) => Self::Number(v),
            Value::String(v) => Self::String(v),
            Value::Symbol(v) => Self::Symbol(v),
            Value::BigInt(v) => Self::BigInt(v),
            Value::Object(v) => Self::Object(StoredObject::store(owner, &v)?),
        })
    }
    pub(super) fn store(owner: &impl StoredOwner, value: &Value) -> RuntimeResult<Self> {
        Ok(match value {
            Value::Undefined => Self::Undefined,
            Value::Null => Self::Null,
            Value::Boolean(v) => Self::Boolean(*v),
            Value::Number(v) => Self::Number(*v),
            Value::String(v) => Self::String(v.clone()),
            Value::Symbol(v) => Self::Symbol(v.clone()),
            Value::BigInt(v) => Self::BigInt(v.clone()),
            Value::Object(v) => Self::Object(StoredObject::store(owner, v)?),
        })
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<Value> {
        Ok(match self {
            Self::Undefined => Value::Undefined,
            Self::Null => Value::Null,
            Self::Boolean(v) => Value::Boolean(*v),
            Self::Number(v) => Value::Number(*v),
            Self::String(v) => Value::String(v.clone()),
            Self::Symbol(v) => Value::Symbol(v.clone()),
            Self::BigInt(v) => Value::BigInt(v.clone()),
            Self::Object(v) => Value::Object(v.load(owner)?),
        })
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Self::Object(value) = self {
            value.trace(tracer);
        }
    }
}

// Heap descriptors are always complete. Keeping distinct data/accessor variants
// avoids two unused getter/setter handles on every ordinary numeric property.
#[derive(Clone, Debug)]
pub(super) enum StoredDescriptor {
    Data {
        value: StoredValue,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    },
    Accessor {
        get: Option<StoredObject>,
        set: Option<StoredObject>,
        enumerable: bool,
        configurable: bool,
    },
}
impl StoredDescriptor {
    pub(super) fn store(owner: &Object, descriptor: &PropertyDescriptor) -> RuntimeResult<Self> {
        let enumerable = descriptor
            .enumerable
            .ok_or_else(|| RuntimeError::internal_error("incomplete stored descriptor"))?;
        let configurable = descriptor
            .configurable
            .ok_or_else(|| RuntimeError::internal_error("incomplete stored descriptor"))?;
        if descriptor.is_accessor_descriptor() {
            Ok(Self::Accessor {
                get: descriptor
                    .get
                    .as_ref()
                    .ok_or_else(|| RuntimeError::internal_error("incomplete accessor"))?
                    .as_ref()
                    .map(|value| StoredObject::store(owner, value))
                    .transpose()?,
                set: descriptor
                    .set
                    .as_ref()
                    .ok_or_else(|| RuntimeError::internal_error("incomplete accessor"))?
                    .as_ref()
                    .map(|value| StoredObject::store(owner, value))
                    .transpose()?,
                enumerable,
                configurable,
            })
        } else {
            Ok(Self::Data {
                value: StoredValue::store(
                    owner,
                    descriptor.value.as_ref().ok_or_else(|| {
                        RuntimeError::internal_error("incomplete data descriptor")
                    })?,
                )?,
                writable: descriptor
                    .writable
                    .ok_or_else(|| RuntimeError::internal_error("incomplete data descriptor"))?,
                enumerable,
                configurable,
            })
        }
    }
    pub(super) fn load(&self, owner: &Object) -> RuntimeResult<PropertyDescriptor> {
        Ok(match self {
            Self::Data {
                value,
                writable,
                enumerable,
                configurable,
            } => {
                PropertyDescriptor::data(value.load(owner)?, *writable, *enumerable, *configurable)
            }
            Self::Accessor {
                get,
                set,
                enumerable,
                configurable,
            } => PropertyDescriptor::accessor(
                get.as_ref().map(|value| value.load(owner)).transpose()?,
                set.as_ref().map(|value| value.load(owner)).transpose()?,
                *enumerable,
                *configurable,
            ),
        })
    }
    pub(super) fn is_data_descriptor(&self) -> bool {
        matches!(self, Self::Data { .. })
    }
    pub(super) fn value(&self) -> Option<&StoredValue> {
        match self {
            Self::Data { value, .. } => Some(value),
            _ => None,
        }
    }
    pub(super) fn set_value(&mut self, value: StoredValue) {
        match self {
            Self::Data { value: slot, .. } => *slot = value,
            _ => panic!("data descriptor required"),
        }
    }
    pub(super) fn writable(&self) -> Option<bool> {
        match self {
            Self::Data { writable, .. } => Some(*writable),
            _ => None,
        }
    }
    pub(super) fn enumerable(&self) -> bool {
        match self {
            Self::Data { enumerable, .. } | Self::Accessor { enumerable, .. } => *enumerable,
        }
    }
    pub(super) fn configurable(&self) -> Option<bool> {
        Some(match self {
            Self::Data { configurable, .. } | Self::Accessor { configurable, .. } => *configurable,
        })
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        match self {
            Self::Data { value, .. } => value.trace(tracer),
            Self::Accessor { get, set, .. } => {
                if let Some(get) = get {
                    get.trace(tracer);
                }
                if let Some(set) = set {
                    set.trace(tracer);
                }
            }
        }
    }
}

#[cfg(test)]
mod stored_error_tests {
    use super::*;
    #[test]
    fn stored_errors_preserve_kind_identity_nonowning_edges_and_context_checks() {
        let runtime = Runtime::new();
        let owner = Object::new_in(&runtime);
        let mut stored = Vec::new();
        for native in [false, true] {
            let value = Object::new_in(&runtime);
            let weak = value.downgrade();
            let mut error = if native {
                RuntimeError::type_error("native diagnostic")
            } else {
                RuntimeError::thrown(Value::Object(value.clone()))
            };
            if native {
                error.attach_thrown(Value::Object(value.clone()));
            }
            let kind = error.kind();
            let message = error.message().to_owned();
            let record = StoredRuntimeError::store(&owner, error).unwrap();
            let loaded = record.load(&owner).unwrap();
            assert_eq!(loaded.kind(), kind);
            assert_eq!(loaded.message(), message);
            assert!(
                loaded
                    .thrown_value()
                    .unwrap()
                    .same_value(&Value::Object(value.clone()))
            );
            drop(loaded);
            drop(value);
            stored.push((weak, record));
        }
        assert_eq!(runtime.collect_garbage().unwrap().retained, 1);
        for (weak, error) in stored {
            assert!(weak.upgrade().is_none());
            assert!(error.load(&owner).is_err());
        }
        let other = Runtime::new();
        let foreign = Object::new_in(&other);
        assert!(
            StoredRuntimeError::store(&owner, RuntimeError::thrown(Value::Object(foreign.clone())))
                .is_err()
        );
        let mut native = RuntimeError::type_error("foreign native");
        native.attach_thrown(Value::Object(foreign));
        assert!(StoredRuntimeError::store(&owner, native).is_err());
        for value in [Value::Undefined, Value::Null, Value::Number(-0.0)] {
            let stored =
                StoredRuntimeError::store(&owner, RuntimeError::thrown(value.clone())).unwrap();
            assert!(
                stored
                    .load(&owner)
                    .unwrap()
                    .thrown_value()
                    .unwrap()
                    .same_value(&value)
            );
        }
    }
}
