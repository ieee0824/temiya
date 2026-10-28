//! Realm-local intrinsic objects and well-known symbols.

use std::collections::BTreeMap;

use super::{
    HostHooks, JsString, Object, PropertyDescriptor, RuntimeError, RuntimeResult, Symbol, Value,
};
use super::{
    Runtime,
    stored::{StoredObject, StoredOwner},
};

/// Identifies an intrinsic object owned by a Realm.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IntrinsicId {
    /// `%Object%`.
    Object,
    /// `%Object.prototype%`.
    ObjectPrototype,
    /// `%Function%`.
    Function,
    /// `%Function.prototype%`.
    FunctionPrototype,
    /// `%Array%`.
    Array,
    /// `%Array.prototype%`.
    ArrayPrototype,
    /// `%String%`.
    String,
    /// `%String.prototype%`.
    StringPrototype,
    /// `%Number%`.
    Number,
    /// `%Number.prototype%`.
    NumberPrototype,
    /// `%Boolean%`.
    Boolean,
    /// `%Boolean.prototype%`.
    BooleanPrototype,
    /// `%Symbol%`.
    Symbol,
    /// `%Symbol.prototype%`.
    SymbolPrototype,
    /// `%BigInt%`.
    BigInt,
    /// `%BigInt.prototype%`.
    BigIntPrototype,
    /// `%RegExp%`.
    RegExp,
    /// `%RegExp.prototype%`.
    RegExpPrototype,
    /// `%Promise%`.
    Promise,
    /// `%Promise.prototype%`.
    PromisePrototype,
    /// `%Map%`.
    Map,
    /// `%Map.prototype%`.
    MapPrototype,
    /// `%Set%`.
    Set,
    /// `%Set.prototype%`.
    SetPrototype,
    /// `%WeakMap%`.
    WeakMap,
    /// `%WeakMap.prototype%`.
    WeakMapPrototype,
    /// `%WeakSet%`.
    WeakSet,
    /// `%WeakSet.prototype%`.
    WeakSetPrototype,
    /// `%Date%`.
    Date,
    /// `%Date.prototype%`.
    DatePrototype,
    /// `%Error%`.
    Error,
    /// `%Error.prototype%`.
    ErrorPrototype,
    /// `%TypeError%`.
    TypeError,
    /// `%TypeError.prototype%`.
    TypeErrorPrototype,
    /// `%RangeError%`.
    RangeError,
    /// `%RangeError.prototype%`.
    RangeErrorPrototype,
    /// `%ReferenceError%`.
    ReferenceError,
    /// `%ReferenceError.prototype%`.
    ReferenceErrorPrototype,
    /// `%SyntaxError%`.
    SyntaxError,
    /// `%SyntaxError.prototype%`.
    SyntaxErrorPrototype,
    /// `%ArrayBuffer%`.
    ArrayBuffer,
    /// `%ArrayBuffer.prototype%`.
    ArrayBufferPrototype,
    /// `%TypedArray%`.
    TypedArray,
    /// `%TypedArray.prototype%`.
    TypedArrayPrototype,
    /// `%DataView%`.
    DataView,
    /// `%DataView.prototype%`.
    DataViewPrototype,
    /// `%Reflect%`.
    Reflect,
    /// `%Math%`.
    Math,
    /// `%JSON%`.
    Json,
    /// `%Iterator.prototype%` (iterator helpers remain separate work).
    IteratorPrototype,
    /// `%ArrayIteratorPrototype%`.
    ArrayIteratorPrototype,
    /// `%StringIteratorPrototype%`.
    StringIteratorPrototype,
    /// `%Proxy%`.
    Proxy,
}

impl IntrinsicId {
    /// Every intrinsic identity, in deterministic initialization order.
    pub const ALL: [Self; 53] = [
        Self::IteratorPrototype,
        Self::ArrayIteratorPrototype,
        Self::StringIteratorPrototype,
        Self::ObjectPrototype,
        Self::FunctionPrototype,
        Self::ArrayPrototype,
        Self::StringPrototype,
        Self::NumberPrototype,
        Self::BooleanPrototype,
        Self::SymbolPrototype,
        Self::BigIntPrototype,
        Self::RegExpPrototype,
        Self::PromisePrototype,
        Self::MapPrototype,
        Self::SetPrototype,
        Self::WeakMapPrototype,
        Self::WeakSetPrototype,
        Self::DatePrototype,
        Self::ErrorPrototype,
        Self::TypeErrorPrototype,
        Self::RangeErrorPrototype,
        Self::ReferenceErrorPrototype,
        Self::SyntaxErrorPrototype,
        Self::ArrayBufferPrototype,
        Self::TypedArrayPrototype,
        Self::DataViewPrototype,
        Self::Reflect,
        Self::Math,
        Self::Json,
        Self::Object,
        Self::Function,
        Self::Array,
        Self::String,
        Self::Number,
        Self::Boolean,
        Self::Symbol,
        Self::BigInt,
        Self::RegExp,
        Self::Promise,
        Self::Map,
        Self::Set,
        Self::WeakMap,
        Self::WeakSet,
        Self::Date,
        Self::Error,
        Self::TypeError,
        Self::RangeError,
        Self::ReferenceError,
        Self::SyntaxError,
        Self::ArrayBuffer,
        Self::TypedArray,
        Self::DataView,
        Self::Proxy,
    ];

    fn pair(self) -> Option<(Self, &'static str)> {
        Some(match self {
            Self::Object => (Self::ObjectPrototype, "Object"),
            Self::Function => (Self::FunctionPrototype, "Function"),
            Self::Array => (Self::ArrayPrototype, "Array"),
            Self::String => (Self::StringPrototype, "String"),
            Self::Number => (Self::NumberPrototype, "Number"),
            Self::Boolean => (Self::BooleanPrototype, "Boolean"),
            Self::Symbol => (Self::SymbolPrototype, "Symbol"),
            Self::BigInt => (Self::BigIntPrototype, "BigInt"),
            Self::RegExp => (Self::RegExpPrototype, "RegExp"),
            Self::Promise => (Self::PromisePrototype, "Promise"),
            Self::Map => (Self::MapPrototype, "Map"),
            Self::Set => (Self::SetPrototype, "Set"),
            Self::WeakMap => (Self::WeakMapPrototype, "WeakMap"),
            Self::WeakSet => (Self::WeakSetPrototype, "WeakSet"),
            Self::Date => (Self::DatePrototype, "Date"),
            Self::Error => (Self::ErrorPrototype, "Error"),
            Self::TypeError => (Self::TypeErrorPrototype, "TypeError"),
            Self::RangeError => (Self::RangeErrorPrototype, "RangeError"),
            Self::ReferenceError => (Self::ReferenceErrorPrototype, "ReferenceError"),
            Self::SyntaxError => (Self::SyntaxErrorPrototype, "SyntaxError"),
            Self::ArrayBuffer => (Self::ArrayBufferPrototype, "ArrayBuffer"),
            Self::TypedArray => (Self::TypedArrayPrototype, "TypedArray"),
            Self::DataView => (Self::DataViewPrototype, "DataView"),
            _ => return None,
        })
    }
}

/// A complete, identity-stable table of intrinsic objects for one Realm.
#[derive(Clone, Debug)]
pub struct IntrinsicRegistry(BTreeMap<IntrinsicId, Object>);

pub(super) struct StoredIntrinsicRegistry(BTreeMap<IntrinsicId, StoredObject>);
impl StoredIntrinsicRegistry {
    pub(super) fn initial(
        runtime: Option<&Runtime>,
        registry: &IntrinsicRegistry,
    ) -> RuntimeResult<Self> {
        registry
            .0
            .iter()
            .map(|(id, object)| Ok((*id, StoredObject::initial(runtime, object)?)))
            .collect::<RuntimeResult<BTreeMap<_, _>>>()
            .map(Self)
    }
    pub(super) fn get(&self, owner: &impl StoredOwner, id: IntrinsicId) -> Object {
        self.0
            .get(&id)
            .expect("complete stored intrinsic registry")
            .load(owner)
            .expect("live intrinsic object")
    }
    pub(super) fn load(&self, owner: &impl StoredOwner) -> IntrinsicRegistry {
        IntrinsicRegistry(
            self.0
                .iter()
                .map(|(id, object)| (*id, object.load(owner).expect("live intrinsic object")))
                .collect(),
        )
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        for object in self.0.values() {
            object.trace(tracer);
        }
    }
}

impl IntrinsicRegistry {
    pub(crate) fn initialize(
        global: &Object,
        host_hooks: HostHooks,
        symbols: &WellKnownSymbols,
    ) -> RuntimeResult<Self> {
        let mut objects: BTreeMap<IntrinsicId, Object> = BTreeMap::new();
        for id in IntrinsicId::ALL {
            let object = if id == IntrinsicId::FunctionPrototype {
                Object::native_function_for(global, |_, _| Ok(Value::Undefined))
            } else if id == IntrinsicId::ArrayPrototype {
                super::ArrayObject::new_for(global, 0)?.object()
            } else if id == IntrinsicId::StringPrototype {
                super::StringObject::new_for(global, super::JsString::default()).object()
            } else if id == IntrinsicId::Object {
                let prototypes = [
                    IntrinsicId::ObjectPrototype,
                    IntrinsicId::NumberPrototype,
                    IntrinsicId::StringPrototype,
                    IntrinsicId::BooleanPrototype,
                    IntrinsicId::SymbolPrototype,
                    IntrinsicId::BigIntPrototype,
                ]
                .map(|id| {
                    objects
                        .get(&id)
                        .expect("primitive prototype allocated")
                        .clone()
                });
                let construct_prototypes = prototypes.clone();
                Object::native_callable_constructor_for(
                    global,
                    move |_, arguments| {
                        object_from_value(
                            arguments.first().unwrap_or(&Value::Undefined),
                            &prototypes,
                        )
                        .map(Value::Object)
                    },
                    move |arguments, _| {
                        object_from_value(
                            arguments.first().unwrap_or(&Value::Undefined),
                            &construct_prototypes,
                        )
                    },
                )
            } else if let Some((prototype_id, _)) = id.pair() {
                let prototype = objects.get(&prototype_id).cloned().ok_or_else(|| {
                    RuntimeError::internal_error("intrinsic prototype was not allocated")
                })?;
                Object::native_callable_constructor_for(
                    global,
                    |_, _| Ok(Value::Undefined),
                    move |_, _| {
                        let instance = Object::new_for(&prototype);
                        instance.set_prototype(Some(prototype.clone()));
                        Ok(instance)
                    },
                )
            } else if id == IntrinsicId::Proxy {
                Object::native_constructor_for(global, |_, new_target| {
                    Ok(Object::new_for(&new_target))
                })
            } else {
                Object::new_for(global)
            };
            objects.insert(id, object);
        }
        let registry = Self(objects);
        registry.wire_graph()?;
        for (name, symbol) in [
            ("iterator", WellKnownSymbol::Iterator),
            ("hasInstance", WellKnownSymbol::HasInstance),
            ("toPrimitive", WellKnownSymbol::ToPrimitive),
            ("toStringTag", WellKnownSymbol::ToStringTag),
        ] {
            define(
                &registry.get(IntrinsicId::Symbol),
                name,
                Value::Symbol(symbols.get(symbol)),
                false,
                false,
            )?;
        }
        super::builtins::initialize(&registry, host_hooks, symbols)?;
        super::builtins::install_has_instance(&registry, symbols)?;
        super::array_methods::install(&registry, symbols)?;
        super::string_methods::install(&registry, symbols)?;
        super::iterator::install(&registry, symbols)?;
        registry.install_globals(global)?;
        Ok(registry)
    }

    /// Returns the object for an intrinsic identity.
    #[must_use]
    pub fn get(&self, id: IntrinsicId) -> Object {
        self.0
            .get(&id)
            .cloned()
            .expect("complete intrinsic registry")
    }

    /// Iterates over every intrinsic in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (IntrinsicId, Object)> + '_ {
        IntrinsicId::ALL.into_iter().map(|id| (id, self.get(id)))
    }

    fn wire_graph(&self) -> RuntimeResult<()> {
        let object_prototype = self.get(IntrinsicId::ObjectPrototype);
        let function_prototype = self.get(IntrinsicId::FunctionPrototype);
        function_prototype.set_prototype(Some(object_prototype.clone()));
        for id in IntrinsicId::ALL {
            let object = self.get(id);
            if id != IntrinsicId::ObjectPrototype && id != IntrinsicId::FunctionPrototype {
                let prototype = if id.pair().is_some() || id == IntrinsicId::Proxy {
                    &function_prototype
                } else {
                    &object_prototype
                };
                object.set_prototype(Some(prototype.clone()));
            }
            if let Some((prototype_id, _)) = id.pair() {
                let prototype = self.get(prototype_id);
                define(
                    &object,
                    "prototype",
                    Value::Object(prototype.clone()),
                    false,
                    false,
                )?;
                define(&prototype, "constructor", Value::Object(object), true, true)?;
            }
        }
        for (id, name) in [
            (IntrinsicId::ErrorPrototype, "Error"),
            (IntrinsicId::TypeErrorPrototype, "TypeError"),
            (IntrinsicId::RangeErrorPrototype, "RangeError"),
            (IntrinsicId::ReferenceErrorPrototype, "ReferenceError"),
            (IntrinsicId::SyntaxErrorPrototype, "SyntaxError"),
        ] {
            let prototype = self.get(id);
            if id != IntrinsicId::ErrorPrototype {
                prototype.set_prototype(Some(self.get(IntrinsicId::ErrorPrototype)));
            }
            define(&prototype, "name", Value::String(name.into()), true, true)?;
            define(&prototype, "message", Value::String("".into()), true, true)?;
        }
        self.get(IntrinsicId::TypedArrayPrototype)
            .set_prototype(Some(object_prototype));
        Ok(())
    }

    fn install_globals(&self, global: &Object) -> RuntimeResult<()> {
        for id in IntrinsicId::ALL {
            let Some((_, name)) = id.pair() else { continue };
            define(global, name, Value::Object(self.get(id)), true, true)?;
        }
        for (id, name) in [
            (IntrinsicId::Reflect, "Reflect"),
            (IntrinsicId::Math, "Math"),
            (IntrinsicId::Json, "JSON"),
            (IntrinsicId::Proxy, "Proxy"),
        ] {
            define(global, name, Value::Object(self.get(id)), true, true)?;
        }
        define(global, "undefined", Value::Undefined, false, false)?;
        define(global, "NaN", Value::Number(f64::NAN), false, false)?;
        define(
            global,
            "Infinity",
            Value::Number(f64::INFINITY),
            false,
            false,
        )
    }
}

/// Identifies one Realm-local well-known symbol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum WellKnownSymbol {
    /// `Symbol.iterator`.
    Iterator,
    /// `Symbol.asyncIterator`.
    AsyncIterator,
    /// `Symbol.hasInstance`.
    HasInstance,
    /// `Symbol.toPrimitive`.
    ToPrimitive,
    /// `Symbol.toStringTag`.
    ToStringTag,
    /// `Symbol.species`.
    Species,
}

/// The well-known symbols belonging to one Realm.
#[derive(Clone, Debug)]
pub struct WellKnownSymbols(BTreeMap<WellKnownSymbol, Symbol>);

impl WellKnownSymbols {
    pub(crate) fn initialize() -> Self {
        let mut symbols = BTreeMap::new();
        for (id, description) in [
            (WellKnownSymbol::Iterator, "Symbol.iterator"),
            (WellKnownSymbol::AsyncIterator, "Symbol.asyncIterator"),
            (WellKnownSymbol::HasInstance, "Symbol.hasInstance"),
            (WellKnownSymbol::ToPrimitive, "Symbol.toPrimitive"),
            (WellKnownSymbol::ToStringTag, "Symbol.toStringTag"),
            (WellKnownSymbol::Species, "Symbol.species"),
        ] {
            symbols.insert(id, Symbol::new(Some(JsString::from(description))));
        }
        Self(symbols)
    }

    /// Returns a Realm's symbol for the requested well-known identity.
    #[must_use]
    pub fn get(&self, id: WellKnownSymbol) -> Symbol {
        self.0.get(&id).cloned().expect("complete symbol registry")
    }
}

fn define(
    object: &Object,
    name: &str,
    value: Value,
    writable: bool,
    configurable: bool,
) -> RuntimeResult<()> {
    let accepted = object.define_own_property(
        name.into(),
        PropertyDescriptor::data(value, writable, false, configurable),
    )?;
    accepted.then_some(()).ok_or_else(|| {
        RuntimeError::type_error(format!("cannot install intrinsic property {name}"))
    })
}

// ToObject for the intrinsic Object function. Captures prototype identities, not
// an entire Realm/registry or the Object constructor itself.
fn object_from_value(value: &Value, prototypes: &[Object; 6]) -> RuntimeResult<Object> {
    let (object, index) = match value {
        Value::Object(object) => return Ok(object.clone()),
        Value::Undefined | Value::Null => (Object::new(), 0),
        Value::Number(value) => (super::NumberObject::new(*value).object(), 1),
        Value::String(value) => (super::StringObject::new(value.clone()).object(), 2),
        Value::Boolean(_) | Value::Symbol(_) => {
            let object = Object::new();
            object.0.state.borrow_mut().primitive_data = Some(value.clone());
            (
                object,
                if matches!(value, Value::Boolean(_)) {
                    3
                } else {
                    4
                },
            )
        }
        Value::BigInt(value) => (super::BigIntObject::new(value.clone()).object(), 5),
    };
    object.set_prototype(Some(prototypes[index].clone()));
    Ok(object)
}
