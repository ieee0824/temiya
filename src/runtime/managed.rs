//! Shared allocation context for the coordinated runtime graph migration.
//!
//! Allocation collection stays disabled until stored edges and every callable,
//! environment, frame and host boundary have moved to the tracing protocol.
use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};

use super::{Object, ObjectData};
use crate::gc::{Handle, Heap, HeapOptions, Root, Trace, Tracer};
#[cfg(test)]
use std::cell::Cell;

#[derive(Clone)]
pub(crate) struct Runtime(Rc<RuntimeData>);
pub(super) struct RuntimeData {
    heap: RefCell<Heap>,
    #[cfg(test)]
    collecting: Cell<bool>,
}

pub(super) struct ManagedObject {
    data: Rc<ObjectData>,
}
impl Trace for ManagedObject {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.data.trace_object_edges(tracer);
    }
}

// One shared facade registration, not a registration per facade clone. Internal
// stored edges will contain only Handle<ManagedObject>, never this owner.
pub(super) struct ObjectRoot {
    // Drop the registration and data reference before the last Runtime owner.
    root: Root<ManagedObject>,
    data: Rc<ObjectData>,
    runtime: Runtime,
}
#[derive(Clone)]
pub(super) enum ObjectReference {
    Legacy(Rc<ObjectData>),
    Managed(Rc<ObjectRoot>),
}
impl Deref for ObjectReference {
    type Target = ObjectData;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(data) => data,
            Self::Managed(root) => &root.data,
        }
    }
}
impl ObjectReference {
    pub(super) fn legacy(data: ObjectData) -> Self {
        Self::Legacy(Rc::new(data))
    }
    pub(super) fn runtime(&self) -> Option<&Runtime> {
        match self {
            Self::Legacy(_) => None,
            Self::Managed(root) => Some(&root.runtime),
        }
    }
    pub(super) fn handle(&self) -> Option<Handle<ManagedObject>> {
        match self {
            Self::Legacy(_) => None,
            Self::Managed(root) => Some(root.root.handle()),
        }
    }
    pub(super) fn same_object(&self, other: &Self) -> bool {
        std::ptr::eq(&**self, &**other)
    }
}

#[derive(Clone)]
pub(super) enum WeakObjectRoot {
    Legacy(Weak<ObjectData>),
    Managed {
        runtime: Weak<RuntimeData>,
        handle: Handle<ManagedObject>,
    },
}
impl WeakObjectRoot {
    pub(super) fn handle(&self) -> Option<Handle<ManagedObject>> {
        match self {
            Self::Managed { handle, .. } => Some(*handle),
            Self::Legacy(_) => None,
        }
    }

    pub(super) fn is_live(&self) -> bool {
        match self {
            Self::Legacy(weak) => weak.strong_count() > 0,
            Self::Managed { runtime, handle } => runtime
                .upgrade()
                .is_some_and(|runtime| runtime.heap.borrow().with(*handle, |_| ()).is_some()),
        }
    }

    pub(super) fn matches(&self, object: &Object) -> bool {
        match (self, &object.0) {
            (Self::Legacy(weak), ObjectReference::Legacy(data)) => {
                weak.as_ptr() == Rc::as_ptr(data)
            }
            (Self::Managed { handle, .. }, ObjectReference::Managed(root)) => {
                *handle == root.root.handle()
            }
            _ => false,
        }
    }

    pub(super) fn new(object: &Object) -> Self {
        match &object.0 {
            ObjectReference::Legacy(data) => Self::Legacy(Rc::downgrade(data)),
            ObjectReference::Managed(root) => Self::Managed {
                runtime: Rc::downgrade(&root.runtime.0),
                handle: root.root.handle(),
            },
        }
    }

    #[cfg(test)]
    pub(super) fn upgrade(&self) -> Option<Object> {
        match self {
            Self::Legacy(weak) => weak
                .upgrade()
                .map(|data| Object(ObjectReference::Legacy(data))),
            Self::Managed { runtime, handle } => Runtime(runtime.upgrade()?).root_object(*handle),
        }
    }
}

impl Runtime {
    pub(super) fn root_realm(
        &self,
        handle: Handle<super::execution::ManagedRealm>,
    ) -> Option<super::Realm> {
        use super::execution::{RealmReference, RealmRoot};
        let heap = self.0.heap.borrow();
        let data = heap.with(handle, |realm| realm.data.clone())?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(super::Realm(RealmReference::Managed(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(RealmRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(super::Realm(RealmReference::Managed(facade)))
    }
    pub(super) fn allocate_realm(&self, data: super::execution::RealmData) -> super::Realm {
        use super::execution::{ManagedRealm, RealmReference, RealmRoot};
        let data = Rc::new(data);
        let root = self
            .0
            .heap
            .borrow_mut()
            .allocate(ManagedRealm { data: data.clone() });
        let facade = Rc::new(RealmRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        super::Realm(RealmReference::Managed(facade))
    }
    pub(super) fn root_environment(
        &self,
        handle: Handle<super::environment::ManagedEnvironment>,
    ) -> Option<super::Environment> {
        use super::environment::{EnvironmentReference, EnvironmentRoot};
        let heap = self.0.heap.borrow();
        let data = heap.with(handle, |environment| environment.data.clone())?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(super::Environment(EnvironmentReference::Managed(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(EnvironmentRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(super::Environment(EnvironmentReference::Managed(facade)))
    }
    pub(super) fn allocate_environment(
        &self,
        data: super::environment::EnvironmentData,
    ) -> super::Environment {
        use super::environment::{EnvironmentReference, EnvironmentRoot, ManagedEnvironment};
        let data = Rc::new(data);
        let root = self
            .0
            .heap
            .borrow_mut()
            .allocate(ManagedEnvironment { data: data.clone() });
        let facade = Rc::new(EnvironmentRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        super::Environment(EnvironmentReference::Managed(facade))
    }
    pub(crate) fn new() -> Self {
        Self(Rc::new(RuntimeData {
            heap: RefCell::new(Heap::with_options(HeapOptions {
                allocation_threshold: usize::MAX,
                max_allocations: None,
            })),
            #[cfg(test)]
            collecting: Cell::new(false),
        }))
    }
    pub(super) fn allocate_object(&self, data: ObjectData) -> Object {
        let data = Rc::new(data);
        let root = self
            .0
            .heap
            .borrow_mut()
            .allocate(ManagedObject { data: data.clone() });
        let facade = Rc::new(ObjectRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Object(ObjectReference::Managed(facade))
    }
    pub(super) fn root_object(&self, handle: Handle<ManagedObject>) -> Option<Object> {
        let heap = self.0.heap.borrow();
        let data = heap.with(handle, |object| object.data.clone())?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(Object(ObjectReference::Managed(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(ObjectRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(Object(ObjectReference::Managed(facade)))
    }
    #[cfg(test)]
    pub(crate) fn collect_migration_graph(
        &self,
    ) -> super::RuntimeResult<crate::gc::CollectionStats> {
        // An explicit test safepoint for migrated graph subsets, not the public
        // allocation-collection enablement gate.
        if self.0.collecting.replace(true) {
            return Err(super::RuntimeError::type_error(
                "collection is already active",
            ));
        }
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _reset = Reset(&self.0.collecting);
        let (stats, retired) = self.0.heap.borrow_mut().collect_retiring();
        // Host Drop may reenter evaluation/allocation after slot retirement.
        // The Heap borrow is gone and old generations cannot be resurrected.
        drop(retired);
        Ok(stats)
    }
    pub(super) fn record_object_edge<T: Trace, U: Trace>(
        &self,
        owner: Handle<T>,
        target: Handle<U>,
    ) -> super::RuntimeResult<()> {
        self.0
            .heap
            .borrow_mut()
            .write_barrier(owner, target)
            .map_err(|_| super::RuntimeError::internal_error("invalid managed object edge"))
    }
    pub(super) fn same_heap(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    #[cfg(test)]
    pub(crate) fn verify_migration_barriers(&self) -> Result<(), crate::gc::WriteBarrierError> {
        self.0.heap.borrow().verify_write_barriers()
    }
    #[cfg(test)]
    pub(crate) fn diagnostics(&self) -> crate::gc::HeapDiagnostics {
        self.0.heap.borrow().diagnostics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_object_factory_boxes_in_context_and_reclaims_prototype_cycles() {
        use crate::runtime::{Value, intrinsics::ObjectFactory, object::ManagedCallable};
        let runtime = Runtime::new();
        let constructor = Object::new_in(&runtime);
        runtime.collect_migration_graph().unwrap();
        let prototypes = std::array::from_fn(|_| Object::new_in(&runtime));
        constructor.set_managed_callable(ManagedCallable::ObjectFactory(
            ObjectFactory::new(&constructor, &prototypes).unwrap(),
        ));
        for prototype in &prototypes {
            prototype
                .create_data_property("constructor".into(), Value::Object(constructor.clone()))
                .unwrap();
        }
        runtime.verify_migration_barriers().unwrap();
        let weak = constructor.downgrade();
        let original = Object::new_in(&runtime);
        assert!(
            constructor
                .call(Value::Undefined, &[Value::Object(original.clone())])
                .unwrap()
                .same_value(&Value::Object(original.clone()))
        );
        assert_eq!(
            constructor
                .construct(&[Value::Object(original.clone())], constructor.clone())
                .unwrap(),
            original
        );
        drop(original);
        for (value, index) in [
            (Value::Undefined, 0),
            (Value::Null, 0),
            (Value::Number(42.0), 1),
            (Value::String("abc".into()), 2),
            (Value::Boolean(true), 3),
        ] {
            let Value::Object(boxed) = constructor
                .call(Value::Undefined, std::slice::from_ref(&value))
                .unwrap()
            else {
                panic!("object result")
            };
            assert_eq!(boxed.prototype(), Some(prototypes[index].clone()));
            let constructed = constructor
                .construct(&[value], constructor.clone())
                .unwrap();
            assert_eq!(constructed.prototype(), boxed.prototype());
        }
        let escaped = constructor
            .construct(&[Value::Number(42.0)], constructor.clone())
            .unwrap();
        drop(prototypes);
        drop(constructor);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_some());
        assert!(
            matches!(escaped.get(&"constructor".into()).unwrap(), Value::Object(object) if object.is_callable())
        );
        drop(escaped);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn managed_intrinsic_registry_snapshots_own_roots_but_the_realm_table_does_not() {
        use crate::runtime::{IntrinsicId, Realm, stored::StoredOwner};
        let runtime = Runtime::new();
        let realm = Realm::ordinary_in(&runtime);
        let realm_handle = realm.handle().unwrap();
        let reflect = realm.intrinsic(IntrinsicId::Reflect);
        let reflect_handle = reflect.0.handle().unwrap();
        let weak = reflect.downgrade();
        drop(reflect);
        assert_eq!(runtime.0.heap.borrow().root_count(reflect_handle), 0);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_some());
        let registry = realm.intrinsics();
        let clone = registry.clone();
        assert_eq!(runtime.0.heap.borrow().root_count(reflect_handle), 1);
        drop(realm);
        runtime.collect_migration_graph().unwrap();
        assert!(runtime.0.heap.borrow().with(realm_handle, |_| ()).is_none());
        assert_eq!(
            registry.get(IntrinsicId::Reflect),
            clone.get(IntrinsicId::Reflect)
        );
        drop(registry);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_some());
        drop(clone);
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn managed_realm_clones_share_a_root_and_trace_the_global_records() {
        use crate::runtime::{Realm, Value, stored::StoredOwner};
        let runtime = Runtime::new();
        let global = Object::new_in(&runtime);
        let global_handle = global.0.handle().unwrap();
        let realm = Realm::try_new(global.clone(), Value::Object(global.clone())).unwrap();
        let handle = realm.handle().unwrap();
        let clone = realm.clone();
        assert_eq!(realm, clone);
        assert_eq!(runtime.0.heap.borrow().root_count(handle), 1);
        drop(global);
        assert_eq!(runtime.0.heap.borrow().root_count(global_handle), 0);
        runtime.collect_migration_graph().unwrap();
        let global = clone.global_object();
        assert_eq!(global.0.handle(), Some(global_handle));
        assert_eq!(
            clone.global_environment().binding_object(),
            Some(global.clone())
        );
        drop(global);
        drop(realm);
        runtime.collect_migration_graph().unwrap();
        assert_eq!(runtime.0.heap.borrow().root_count(handle), 1);
        drop(clone);
        runtime.collect_migration_graph().unwrap();
        assert!(runtime.0.heap.borrow().with(handle, |_| ()).is_none());
    }
    #[test]
    fn managed_realm_rejects_foreign_global_this_before_initializing_intrinsics() {
        use crate::runtime::{Realm, Value};
        let runtime = Runtime::new();
        let global = Object::new_in(&runtime);
        let foreign = Object::new_in(&Runtime::new());
        assert!(Realm::try_new(global.clone(), Value::Object(foreign)).is_err());
        assert!(global.own_property_keys().is_empty());
    }
    #[test]
    fn typed_non_object_owners_store_and_trace_object_edges_with_barriers() {
        use crate::runtime::stored::{StoredOwner, StoredValue};
        struct Record(RefCell<Option<StoredValue>>);
        impl Trace for Record {
            fn trace(&self, tracer: &mut Tracer<'_>) {
                if let Some(value) = &*self.0.borrow() {
                    value.trace(tracer);
                }
            }
        }
        struct Facade {
            root: Root<Record>,
            runtime: Runtime,
        }
        impl StoredOwner for Facade {
            type Allocation = Record;
            fn runtime(&self) -> Option<&Runtime> {
                Some(&self.runtime)
            }
            fn handle(&self) -> Option<Handle<Record>> {
                Some(self.root.handle())
            }
        }
        let runtime = Runtime::new();
        let root = runtime
            .0
            .heap
            .borrow_mut()
            .allocate(Record(RefCell::new(None)));
        let owner = Facade {
            root,
            runtime: runtime.clone(),
        };
        runtime.collect_migration_graph().unwrap();
        let target = Object::new_in(&runtime);
        let weak = target.downgrade();
        let value =
            StoredValue::store(&owner, &super::super::Value::Object(target.clone())).unwrap();
        runtime
            .0
            .heap
            .borrow()
            .with(owner.root.handle(), |record| {
                *record.0.borrow_mut() = Some(value)
            })
            .unwrap();
        drop(target);
        assert_eq!(runtime.diagnostics().rooted, 1);
        let mut heap = runtime.0.heap.borrow_mut();
        heap.verify_write_barriers().unwrap();
        assert_eq!(heap.collect_minor().reclaimed, 0);
        drop(heap);
        let loaded = runtime
            .0
            .heap
            .borrow()
            .with(owner.root.handle(), |record| {
                record.0.borrow().clone().unwrap()
            })
            .unwrap()
            .load(&owner)
            .unwrap();
        assert!(matches!(loaded, super::super::Value::Object(_)));
        drop(loaded);
        assert!(weak.upgrade().is_some());
        let foreign = Object::new_in(&Runtime::new());
        assert!(StoredValue::store(&owner, &super::super::Value::Object(foreign)).is_err());
        drop(owner);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 2);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn managed_facades_share_identity_and_registration_across_clones() {
        let runtime = Runtime::new();
        let first = Object::new_in(&runtime);
        let cloned = first.clone();
        assert_eq!(runtime.diagnostics().live, 1);
        assert_eq!(runtime.diagnostics().rooted, 1);
        let handle = first.0.handle().unwrap();
        let other_facade = runtime.root_object(handle).unwrap();
        assert_eq!(first, other_facade);
        assert_eq!(runtime.0.heap.borrow().root_count(handle), 1);
        drop(first);
        drop(cloned);
        assert_eq!(runtime.0.heap.borrow().root_count(handle), 1);
        drop(other_facade);
        assert_eq!(runtime.diagnostics().rooted, 0);
    }
    #[test]
    fn managed_weak_edges_do_not_keep_the_runtime_alive() {
        let runtime = Runtime::new();
        let object = Object::new_in(&runtime);
        let weak = object.downgrade();
        let other = weak.upgrade().unwrap();
        assert_eq!(object, other);
        drop(object);
        drop(other);
        drop(runtime);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn another_runtime_cannot_root_a_foreign_handle() {
        let first = Runtime::new();
        let second = Runtime::new();
        let object = Object::new_in(&first);
        assert!(!first.same_heap(&second));
        assert!(second.root_object(object.0.handle().unwrap()).is_none());
    }
}

#[cfg(test)]
mod stored_graph_tests {
    use super::*;
    use crate::runtime::{PropertyDescriptor, Value};
    #[test]
    fn managed_enumerable_keys_keep_integer_order_and_skip_hidden_accessors() {
        let runtime = Runtime::new();
        let object = Object::new_in(&runtime);
        let calls = Rc::new(std::cell::Cell::new(0));
        let callback_calls = calls.clone();
        let getter = Object::new_in(&runtime);
        getter.set_native_call(move |_, _| {
            callback_calls.set(callback_calls.get() + 1);
            Ok(Value::Number(0.0))
        });
        object
            .create_data_property("10".into(), Value::Number(10.0))
            .unwrap();
        object
            .create_data_property("2".into(), Value::Number(2.0))
            .unwrap();
        object
            .create_data_property("a".into(), Value::Number(1.0))
            .unwrap();
        object
            .define_own_property(
                "hidden".into(),
                PropertyDescriptor::accessor(Some(getter.clone()), None, false, true),
            )
            .unwrap();
        object
            .create_data_property(crate::runtime::Symbol::new(None).into(), Value::Number(3.0))
            .unwrap();
        assert_eq!(
            object.enumerable_own_string_keys().unwrap(),
            vec!["2".into(), "10".into(), "a".into()]
        );
        assert_eq!(calls.get(), 0);
        drop(getter);
        drop(object);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
    #[test]
    fn lazy_native_captures_stay_traced_without_materializing_unused_facades() {
        let runtime = Runtime::new();
        let prototype = Object::new_in(&runtime);
        let capture = Object::new_in(&runtime);
        capture
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let function = Object::native_function_with_lazy_object_capture(
            &prototype,
            "lazy",
            0,
            &capture,
            Some("answer".into()),
            |owner, capture, key, _, arguments| {
                let super::super::stored::StoredObject::Managed(handle) = capture else {
                    panic!("managed capture");
                };
                let runtime = owner.0.runtime().unwrap();
                assert!(
                    runtime
                        .0
                        .heap
                        .borrow()
                        .with(*handle, |object| object
                            .data
                            .facade
                            .borrow()
                            .upgrade()
                            .is_none())
                        .unwrap()
                );
                if arguments.is_empty() {
                    return Ok(Value::Null);
                }
                capture.load(owner)?.get(key.expect("capture key"))
            },
        )
        .unwrap();
        capture
            .create_data_property("back".into(), Value::Object(function.clone()))
            .unwrap();
        let weak = capture.downgrade();
        drop(prototype);
        drop(capture);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        assert!(matches!(
            function.call(Value::Undefined, &[]).unwrap(),
            Value::Null
        ));
        assert!(
            function
                .call(Value::Undefined, &[Value::Boolean(true)])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(function);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn declared_factories_preserve_callability_and_reclaim_constructor_cycles() {
        for callable in [false, true] {
            let runtime = Runtime::new();
            let function = Object::new_in(&runtime);
            runtime.collect_migration_graph().unwrap();
            let prototype = Object::new_in(&runtime);
            let weak = [function.downgrade(), prototype.downgrade()];
            function
                .set_captured_factory(
                    &prototype,
                    None,
                    if callable {
                        Some((false, |capture, _, _, _| {
                            assert!(capture.is_none());
                            Ok(Value::Number(42.0))
                        }))
                    } else {
                        None
                    },
                    Some(|prototype, _, _, _| {
                        let instance = Object::new_for(prototype);
                        instance.initialize_prototype(Some(prototype.clone()));
                        Ok(instance)
                    }),
                )
                .unwrap();
            prototype
                .create_data_property("constructor".into(), Value::Object(function.clone()))
                .unwrap();
            drop(prototype);
            runtime.verify_migration_barriers().unwrap();
            assert_eq!(runtime.diagnostics().rooted, 1);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 2);
            assert_eq!(function.is_callable(), callable);
            assert!(function.is_constructor());
            if callable {
                assert!(
                    function
                        .call(Value::Undefined, &[])
                        .unwrap()
                        .same_value(&Value::Number(42.0))
                );
            } else {
                assert!(function.call(Value::Undefined, &[]).is_err());
            }
            let foreign = Object::new_in(&Runtime::new());
            assert!(
                function
                    .set_captured_factory(&foreign, None, None, None)
                    .is_err()
            );
            assert_eq!(function.is_callable(), callable);
            assert!(function.is_constructor());
            let instance = function.construct(&[], function.clone()).unwrap();
            assert_eq!(instance.prototype(), weak[1].upgrade());
            drop(function);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
            drop(instance);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
            assert!(weak.iter().all(|object| object.upgrade().is_none()));
        }
    }
    #[test]
    fn declared_native_object_captures_are_traced_without_hidden_roots() {
        let runtime = Runtime::new();
        let prototype = Object::new_in(&runtime);
        let capture = Object::new_in(&runtime);
        capture
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let function = Object::native_function_with_object_capture(
            &prototype,
            "captured",
            0,
            &capture,
            Some("answer".into()),
            |capture, key, _, _| capture.get(key.expect("capture key")),
        )
        .unwrap();
        capture
            .create_data_property("back".into(), Value::Object(function.clone()))
            .unwrap();
        let weak = [
            prototype.downgrade(),
            capture.downgrade(),
            function.downgrade(),
        ];
        drop(prototype);
        drop(capture);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        assert!(
            function
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        assert_eq!(
            function.own_property_keys(),
            vec!["name".into(), "length".into()]
        );
        assert!(!function.is_constructor());
        let foreign = Object::new_in(&Runtime::new());
        let before = runtime.diagnostics().live;
        assert!(
            Object::native_function_with_object_capture(
                &function,
                "invalid",
                0,
                &foreign,
                None,
                |_, _, _, _| Ok(Value::Undefined)
            )
            .is_err()
        );
        assert_eq!(runtime.diagnostics().live, before);
        drop(function);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 3);
        assert!(weak.iter().all(|object| object.upgrade().is_none()));
    }
    #[test]
    fn proxy_revoker_retains_its_cycle_until_called_or_unrooted() {
        for call in [false, true] {
            let runtime = Runtime::new();
            let target = Object::new_in(&runtime);
            let handler = Object::new_in(&runtime);
            let proxy =
                crate::runtime::ProxyObject::try_new(target.clone(), handler.clone()).unwrap();
            let revoke = Object::new_in(&runtime);
            revoke.initialize_proxy_revoker(&proxy.object()).unwrap();
            target
                .create_data_property("back".into(), Value::Object(revoke.clone()))
                .unwrap();
            let weak = [
                target.downgrade(),
                handler.downgrade(),
                proxy.object().downgrade(),
            ];
            drop(target);
            drop(handler);
            drop(proxy);
            assert_eq!(runtime.diagnostics().rooted, 1);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
            assert!(revoke.is_callable());
            assert!(!revoke.is_constructor());
            if call {
                for _ in 0..2 {
                    assert!(matches!(
                        revoke.call(Value::Null, &[]).unwrap(),
                        Value::Undefined
                    ));
                }
                assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 3);
                assert!(weak.iter().all(|object| object.upgrade().is_none()));
            }
            drop(revoke);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
            assert!(weak.iter().all(|object| object.upgrade().is_none()));
        }
    }
    #[test]
    fn proxy_cycles_are_traced_and_revocation_releases_target_and_handler() {
        for revoke in [false, true] {
            let runtime = Runtime::new();
            let target = Object::new_in(&runtime);
            target.set_native_call(|_, _| Ok(Value::Number(42.0)));
            let handler = Object::new_in(&runtime);
            let proxy =
                crate::runtime::ProxyObject::try_new(target.clone(), handler.clone()).unwrap();
            for object in [&target, &handler] {
                object
                    .create_data_property("back".into(), Value::Object(proxy.object()))
                    .unwrap();
            }
            let weak = [
                target.downgrade(),
                handler.downgrade(),
                proxy.object().downgrade(),
            ];
            drop(target);
            drop(handler);
            assert_eq!(runtime.diagnostics().rooted, 1);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
            assert!(
                proxy
                    .object()
                    .call(Value::Undefined, &[])
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
            if revoke {
                proxy.revoke();
                assert!(
                    proxy.object().is_callable(),
                    "revocation preserves callable identity"
                );
                assert!(proxy.object().call(Value::Undefined, &[]).is_err());
                assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 2);
                assert!(weak[..2].iter().all(|object| object.upgrade().is_none()));
            }
            drop(proxy);
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
            assert!(weak.iter().all(|object| object.upgrade().is_none()));
        }
    }
    #[test]
    fn bound_function_captures_are_traced_without_rooting_their_cycles() {
        let runtime = Runtime::new();
        let target = Object::new_in(&runtime);
        target.set_native_call(|this, arguments| {
            let Value::Object(this) = this else {
                panic!("bound this");
            };
            let Value::Object(argument) = &arguments[0] else {
                panic!("bound argument");
            };
            Ok(Value::Number(
                this.get(&"answer".into())?.to_number()?
                    + argument.get(&"answer".into())?.to_number()?,
            ))
        });
        let this = Object::new_in(&runtime);
        this.create_data_property("answer".into(), Value::Number(40.0))
            .unwrap();
        let argument = Object::new_in(&runtime);
        argument
            .create_data_property("answer".into(), Value::Number(2.0))
            .unwrap();
        let bound = Object::new_bound(
            &target,
            &Value::Object(this.clone()),
            &[Value::Object(argument.clone())],
        )
        .unwrap();
        let weak = [
            target.downgrade(),
            this.downgrade(),
            argument.downgrade(),
            bound.downgrade(),
        ];
        for object in [&target, &this, &argument] {
            object
                .create_data_property("back".into(), Value::Object(bound.clone()))
                .unwrap();
        }
        drop(target);
        drop(this);
        drop(argument);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
        assert!(
            bound
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(bound);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 4);
        assert!(weak.iter().all(|object| object.upgrade().is_none()));
    }
    #[test]
    fn real_object_properties_are_unrooted_edges_and_cycles_are_swept() {
        let runtime = Runtime::new();
        let object = Object::new_in(&runtime);
        let token = Rc::new(());
        let weak_token = Rc::downgrade(&token);
        object.set_to_string_hook(move || {
            let _ = Rc::strong_count(&token);
            Ok(Value::String("kept".into()))
        });
        object
            .create_data_property("self".into(), Value::Object(object.clone()))
            .unwrap();
        let handle = object.0.handle().unwrap();
        assert_eq!(
            runtime.0.heap.borrow().root_count(handle),
            1,
            "stored self edge must not register a root"
        );
        assert_eq!(runtime.0.heap.borrow_mut().collect().reclaimed, 0);
        assert!(weak_token.upgrade().is_some());
        let weak_object = object.downgrade();
        drop(object);
        assert_eq!(runtime.0.heap.borrow_mut().collect().reclaimed, 1);
        assert!(weak_token.upgrade().is_none());
        assert!(weak_object.upgrade().is_none());
    }
    #[test]
    fn prototype_and_data_edges_keep_an_entire_graph_until_the_last_root() {
        let runtime = Runtime::new();
        let first = Object::new_in(&runtime);
        let second = Object::new_in(&runtime);
        first.try_set_prototype(Some(second.clone())).unwrap();
        second
            .create_data_property("back".into(), Value::Object(first.clone()))
            .unwrap();
        let cloned = first.clone();
        drop(first);
        drop(second);
        assert_eq!(runtime.0.heap.borrow_mut().collect().retained, 2);
        assert!(
            matches!(cloned.get(&"back".into()).unwrap(),Value::Object(value) if value==cloned)
        );
        drop(cloned);
        assert_eq!(runtime.0.heap.borrow_mut().collect().reclaimed, 2);
    }
    #[test]
    fn accessor_edges_are_traced_and_the_callback_is_callable_after_collection() {
        let runtime = Runtime::new();
        let owner = Object::new_in(&runtime);
        let getter = Object::new_in(&runtime);
        getter.set_native_call(|_, _| Ok(Value::Number(42.0)));
        owner
            .define_own_property(
                "x".into(),
                PropertyDescriptor::accessor(Some(getter.clone()), None, true, true),
            )
            .unwrap();
        drop(getter);
        assert_eq!(runtime.0.heap.borrow_mut().collect().retained, 2);
        assert!(
            owner
                .get(&"x".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(owner);
        assert_eq!(runtime.0.heap.borrow_mut().collect().reclaimed, 2);
    }
    #[test]
    fn old_unrooted_owner_remembers_a_new_property_edge() {
        let runtime = Runtime::new();
        let root = Object::new_in(&runtime);
        let owner = Object::new_in(&runtime);
        root.create_data_property("owner".into(), Value::Object(owner.clone()))
            .unwrap();
        runtime.collect_migration_graph().unwrap();
        let child = Object::new_in(&runtime);
        let weak = child.downgrade();
        owner
            .create_data_property("child".into(), Value::Object(child.clone()))
            .unwrap();
        drop(child);
        drop(owner);
        let mut heap = runtime.0.heap.borrow_mut();
        heap.verify_write_barriers().unwrap();
        assert_eq!(heap.collect_minor().reclaimed, 0);
        drop(heap);
        assert!(weak.upgrade().is_some());
        drop(root);
        assert_eq!(runtime.collect_migration_graph().unwrap().reclaimed, 3);
        assert!(weak.upgrade().is_none());
    }
    #[test]
    fn foreign_and_unmanaged_edges_are_rejected_before_property_or_prototype_changes() {
        let runtime = Runtime::new();
        let other = Runtime::new();
        let owner = Object::new_in(&runtime);
        for foreign in [Object::new_in(&other), Object::new()] {
            assert!(
                owner
                    .create_data_property("x".into(), Value::Object(foreign.clone()))
                    .is_err()
            );
            assert!(owner.get_own_property(&"x".into()).is_none());
            assert!(owner.try_set_prototype(Some(foreign)).is_err());
            assert!(owner.prototype().is_none());
        }
    }
}
