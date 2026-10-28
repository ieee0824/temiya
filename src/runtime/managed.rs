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
    pub(super) fn new(object: &Object) -> Self {
        match &object.0 {
            ObjectReference::Legacy(data) => Self::Legacy(Rc::downgrade(data)),
            ObjectReference::Managed(root) => Self::Managed {
                runtime: Rc::downgrade(&root.runtime.0),
                handle: root.root.handle(),
            },
        }
    }

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
    pub(super) fn record_object_edge<T: Trace>(
        &self,
        owner: Handle<T>,
        target: Handle<ManagedObject>,
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
    pub(crate) fn diagnostics(&self) -> crate::gc::HeapDiagnostics {
        self.0.heap.borrow().diagnostics()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
