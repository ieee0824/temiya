//! Shared allocation context for the coordinated runtime graph migration.
//!
//! Engine Realms share this context. Allocation safepoints trace incoming records
//! before publication; public facades register roots in the same heap.
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::{Rc, Weak},
};

use super::{Object, ObjectData};
use crate::gc::{Handle, Heap, HeapOptions, Root, Trace, Tracer};

#[derive(Clone)]
pub struct Runtime(Rc<RuntimeData>);
impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}
pub(super) struct RuntimeData {
    heap: RefCell<Heap>,
    collecting: Cell<bool>,
    allocation_interval: Cell<usize>,
    allocation_remaining: Cell<usize>,
    active_objects: RefCell<Weak<RefCell<Vec<Object>>>>,
    #[cfg(all(test, not(miri)))]
    collection_ns: Cell<u128>,
}

#[cfg(all(test, not(miri)))]
struct CollectionTimer<'a> {
    total: &'a Cell<u128>,
    start: std::time::Instant,
}
#[cfg(all(test, not(miri)))]
impl Drop for CollectionTimer<'_> {
    fn drop(&mut self) {
        self.total.set(
            self.total
                .get()
                .saturating_add(self.start.elapsed().as_nanos()),
        );
    }
}

// Leave room for the container as well as a small polymorphic receiver set.
// The whole bounded region is still cleared before any collection.
const ACTIVE_OBJECTS: usize = 16;

// This is an external activation region, never a payload stored in the heap.
// Nested VM calls share the outer region. Its bounded facade cache is cleared
// before every collection, so cached facades cannot keep a dead JS graph live.
pub(crate) struct ObjectRootRegion {
    runtime: Runtime,
    objects: Rc<RefCell<Vec<Object>>>,
}
impl Drop for ObjectRootRegion {
    fn drop(&mut self) {
        let mut active = self.runtime.0.active_objects.borrow_mut();
        debug_assert_eq!(active.as_ptr(), Rc::as_ptr(&self.objects));
        *active = Weak::new();
    }
}

pub(super) type ManagedObject = ObjectData;
impl Trace for ObjectData {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.trace_object_edges(tracer);
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
pub(super) struct ObjectReference(Rc<ObjectRoot>);
impl Deref for ObjectReference {
    type Target = ObjectData;
    fn deref(&self) -> &Self::Target {
        &self.0.data
    }
}
impl ObjectReference {
    pub(super) fn runtime(&self) -> Option<&Runtime> {
        Some(&self.0.runtime)
    }
    pub(super) fn handle(&self) -> Option<Handle<ManagedObject>> {
        Some(self.0.root.handle())
    }
    pub(super) fn same_object(&self, other: &Self) -> bool {
        std::ptr::eq(&**self, &**other)
    }
}

#[derive(Clone)]
pub(super) struct WeakObjectRoot {
    runtime: Weak<RuntimeData>,
    handle: Handle<ManagedObject>,
}
impl WeakObjectRoot {
    pub(super) fn handle(&self) -> Option<Handle<ManagedObject>> {
        Some(self.handle)
    }

    pub(super) fn is_live(&self) -> bool {
        self.runtime
            .upgrade()
            .is_some_and(|runtime| runtime.heap.borrow().with(self.handle, |_| ()).is_some())
    }

    pub(super) fn matches(&self, object: &Object) -> bool {
        self.handle == object.0.0.root.handle()
    }

    pub(super) fn new(object: &Object) -> Self {
        Self {
            runtime: Rc::downgrade(&object.0.0.runtime.0),
            handle: object.0.0.root.handle(),
        }
    }

    #[cfg(test)]
    pub(super) fn upgrade(&self) -> Option<Object> {
        Runtime(self.runtime.upgrade()?).root_object(self.handle)
    }
}

impl Runtime {
    pub(crate) fn enter_object_root_region(&self) -> Option<ObjectRootRegion> {
        let mut active = self.0.active_objects.borrow_mut();
        if active.upgrade().is_some() {
            return None;
        }
        let objects = Rc::new(RefCell::new(Vec::with_capacity(ACTIVE_OBJECTS)));
        *active = Rc::downgrade(&objects);
        Some(ObjectRootRegion {
            runtime: self.clone(),
            objects,
        })
    }
    fn remember_active_object(&self, object: &Object) {
        let Some(active) = self.0.active_objects.borrow().upgrade() else {
            return;
        };
        let mut objects = active.borrow_mut();
        if objects.last().is_some_and(|last| last == object) {
            return;
        }
        if let Some(index) = objects.iter().position(|cached| cached == object) {
            let cached = objects.remove(index);
            objects.push(cached);
        } else {
            if objects.len() == ACTIVE_OBJECTS {
                objects.remove(0);
            }
            objects.push(object.clone());
        }
    }
    fn clear_active_objects(&self) {
        if let Some(active) = self.0.active_objects.borrow().upgrade() {
            active.borrow_mut().clear();
        }
    }
    fn allocation_safepoint(&self, incoming: &impl Trace) {
        let interval = self.0.allocation_interval.get();
        if interval == usize::MAX || self.0.collecting.get() {
            return;
        }
        let remaining = self.0.allocation_remaining.get();
        if remaining > 1 {
            self.0.allocation_remaining.set(remaining - 1);
            return;
        }
        self.0.collecting.set(true);
        struct Reset<'a>(&'a Cell<bool>);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _reset = Reset(&self.0.collecting);
        #[cfg(all(test, not(miri)))]
        let _timer = CollectionTimer {
            total: &self.0.collection_ns,
            start: std::time::Instant::now(),
        };
        self.clear_active_objects();
        let (_, retired) = self.0.heap.borrow_mut().collect_retiring_with(incoming);
        // A destructor may allocate/evaluate. Nested allocations skip collection
        // until this collection's retired payloads have all been released.
        drop(retired);
        self.0.allocation_remaining.set(interval);
    }

    #[cfg(test)]
    pub(crate) fn set_allocation_collection_interval(&self, interval: usize) {
        self.0.allocation_interval.set(interval);
        self.0.allocation_remaining.set(interval);
    }
    pub(super) fn allocate_module(
        &self,
        data: super::pipeline::ModuleData,
    ) -> super::SourceTextModule {
        use super::pipeline::{ModuleReference, ModuleRoot};
        let data = Rc::new(data);
        self.allocation_safepoint(data.as_ref());
        let root = self.0.heap.borrow_mut().allocate_shared(data.clone());
        let facade = Rc::new(ModuleRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        super::SourceTextModule(ModuleReference(facade))
    }
    pub(super) fn root_module(
        &self,
        handle: Handle<super::pipeline::ManagedModule>,
    ) -> Option<super::SourceTextModule> {
        use super::pipeline::{ModuleReference, ModuleRoot};
        let heap = self.0.heap.borrow();
        let data = heap.shared(handle)?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(super::SourceTextModule(ModuleReference(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(ModuleRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(super::SourceTextModule(ModuleReference(facade)))
    }
    pub(super) fn allocate_job_queue(
        &self,
        data: super::job_queue::JobQueueData,
    ) -> super::JobQueue {
        use super::job_queue::{JobQueueReference, JobQueueRoot};
        let data = Rc::new(RefCell::new(data));
        self.allocation_safepoint(data.as_ref());
        let root = self.0.heap.borrow_mut().allocate_shared(data.clone());
        let facade = Rc::new(JobQueueRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        data.borrow_mut().facade = Rc::downgrade(&facade);
        super::JobQueue(JobQueueReference(facade))
    }

    pub(super) fn root_job_queue(
        &self,
        handle: crate::gc::Handle<super::job_queue::ManagedJobQueue>,
    ) -> Option<super::JobQueue> {
        use super::job_queue::{JobQueueReference, JobQueueRoot};
        let heap = self.0.heap.borrow();
        let data = heap.shared(handle)?;
        if let Some(facade) = data.borrow().facade.upgrade() {
            return Some(super::JobQueue(JobQueueReference(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(JobQueueRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        data.borrow_mut().facade = Rc::downgrade(&facade);
        Some(super::JobQueue(JobQueueReference(facade)))
    }

    pub(super) fn record_job_edges(
        &self,
        owner: Handle<super::job_queue::ManagedJobQueue>,
        edges: &impl Trace,
    ) -> super::RuntimeResult<()> {
        self.0
            .heap
            .borrow_mut()
            .record_traced_edges(owner, edges)
            .map_err(|_| {
                super::RuntimeError::type_error("Job contains an invalid or foreign heap edge")
            })
    }

    pub(super) fn root_realm(
        &self,
        handle: Handle<super::execution::ManagedRealm>,
    ) -> Option<super::Realm> {
        use super::execution::{RealmReference, RealmRoot};
        let heap = self.0.heap.borrow();
        let data = heap.shared(handle)?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(super::Realm(RealmReference(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(RealmRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(super::Realm(RealmReference(facade)))
    }
    pub(super) fn allocate_realm(&self, data: super::execution::RealmData) -> super::Realm {
        use super::execution::{RealmReference, RealmRoot};
        let data = Rc::new(data);
        self.allocation_safepoint(data.as_ref());
        let root = self.0.heap.borrow_mut().allocate_shared(data.clone());
        let facade = Rc::new(RealmRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        super::Realm(RealmReference(facade))
    }
    pub(super) fn root_environment(
        &self,
        handle: Handle<super::environment::ManagedEnvironment>,
    ) -> Option<super::Environment> {
        use super::environment::{EnvironmentReference, EnvironmentRoot};
        let heap = self.0.heap.borrow();
        let data = heap.shared(handle)?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            return Some(super::Environment(EnvironmentReference(facade)));
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(EnvironmentRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Some(super::Environment(EnvironmentReference(facade)))
    }
    pub(super) fn allocate_environment(
        &self,
        data: super::environment::EnvironmentData,
    ) -> super::Environment {
        use super::environment::{EnvironmentReference, EnvironmentRoot};
        let data = Rc::new(data);
        self.allocation_safepoint(data.as_ref());
        let root = self.0.heap.borrow_mut().allocate_shared(data.clone());
        let facade = Rc::new(EnvironmentRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        super::Environment(EnvironmentReference(facade))
    }
    /// Creates an independent allocation context. An Engine shares one context
    /// across all its Realms; use Engine::allocation_context for host values.
    pub fn new() -> Self {
        Self(Rc::new(RuntimeData {
            heap: RefCell::new(Heap::with_options(HeapOptions {
                allocation_threshold: usize::MAX,
                max_allocations: None,
            })),
            collecting: Cell::new(false),
            allocation_interval: Cell::new(4096),
            allocation_remaining: Cell::new(4096),
            active_objects: RefCell::new(Weak::new()),
            #[cfg(all(test, not(miri)))]
            collection_ns: Cell::new(0),
        }))
    }
    pub(super) fn allocate_object(&self, data: ObjectData) -> Object {
        let data = Rc::new(data);
        self.allocation_safepoint(data.as_ref());
        let root = self.0.heap.borrow_mut().allocate_shared(data.clone());
        let facade = Rc::new(ObjectRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        Object(ObjectReference(facade))
    }
    pub(super) fn root_object(&self, handle: Handle<ManagedObject>) -> Option<Object> {
        let heap = self.0.heap.borrow();
        let data = heap.shared(handle)?;
        if let Some(facade) = data.facade.borrow().upgrade() {
            let object = Object(ObjectReference(facade));
            self.remember_active_object(&object);
            return Some(object);
        }
        let root = heap.root(handle)?;
        drop(heap);
        let facade = Rc::new(ObjectRoot {
            root,
            data: data.clone(),
            runtime: self.clone(),
        });
        *data.facade.borrow_mut() = Rc::downgrade(&facade);
        let object = Object(ObjectReference(facade));
        self.remember_active_object(&object);
        Some(object)
    }
    #[cfg(test)]
    pub(crate) fn collect_migration_graph(
        &self,
    ) -> super::RuntimeResult<crate::gc::CollectionStats> {
        self.collect_garbage()
    }
    /// Collects unreachable allocations while preserving every owning facade.
    pub fn collect_garbage(&self) -> super::RuntimeResult<crate::gc::CollectionStats> {
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
        #[cfg(all(test, not(miri)))]
        let _timer = CollectionTimer {
            total: &self.0.collection_ns,
            start: std::time::Instant::now(),
        };
        self.clear_active_objects();
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

    #[cfg(all(test, not(miri)))]
    pub(crate) fn profile_snapshot(&self) -> (crate::gc::HeapDiagnostics, [usize; 3], u128) {
        let heap = self.0.heap.borrow();
        (
            heap.diagnostics(),
            heap.root_activity(),
            self.0.collection_ns.get(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(miri))]
    #[test]
    fn container_and_polymorphic_receivers_reuse_warm_activation_roots() {
        let runtime = Runtime::new();
        let container = Object::new_in(&runtime);
        for index in 0..8 {
            container
                .create_data_property(
                    index.to_string().as_str().into(),
                    super::super::Value::Object(Object::new_in(&runtime)),
                )
                .unwrap();
        }
        let _region = runtime.enter_object_root_region().unwrap();
        let read = |index: usize| {
            let owner = runtime.root_object(container.0.handle().unwrap()).unwrap();
            owner.get(&index.to_string().as_str().into()).unwrap()
        };
        for index in 0..8 {
            drop(read(index));
        }
        let (_, before, _) = runtime.profile_snapshot();
        for index in 0..1_000 {
            drop(read(index % 8));
        }
        let (_, after, _) = runtime.profile_snapshot();
        assert_eq!(
            after, before,
            "warm reads must not allocate more Root registrations"
        );
    }
    #[test]
    fn activation_facades_are_bounded_and_do_not_retain_dead_graphs_at_collection() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let region = runtime.enter_object_root_region().unwrap();
        assert!(runtime.enter_object_root_region().is_none());
        let mut weak = Vec::new();
        for _ in 0..32 {
            let object = Object::new_in(&runtime);
            weak.push(object.downgrade());
            let revived = runtime.root_object(object.0.handle().unwrap()).unwrap();
            assert_eq!(object, revived);
        }
        assert_eq!(runtime.diagnostics().rooted, ACTIVE_OBJECTS);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(weak.iter().all(|object| object.upgrade().is_none()));

        let object = Object::new_in(&runtime);
        object
            .create_data_property("answer".into(), super::super::Value::Number(42.0))
            .unwrap();
        let live_clone = runtime.root_object(object.0.handle().unwrap()).unwrap();
        drop(object);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 1);
        assert!(
            live_clone
                .get(&"answer".into())
                .unwrap()
                .same_value(&super::super::Value::Number(42.0))
        );
        drop(live_clone);
        drop(region);
        assert_eq!(runtime.diagnostics().rooted, 0);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(runtime.enter_object_root_region().is_some());
    }

    #[test]
    fn activation_facades_clear_at_allocation_gc_and_unwind() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _region = runtime.enter_object_root_region().unwrap();
            let object = Object::new_in(&runtime);
            let weak = object.downgrade();
            let revived = runtime.root_object(object.0.handle().unwrap()).unwrap();
            drop((object, revived));
            assert_eq!(runtime.diagnostics().rooted, 1);
            runtime.set_allocation_collection_interval(1);
            let incoming = Object::new_in(&runtime);
            assert!(weak.upgrade().is_none());
            let revived = runtime.root_object(incoming.0.handle().unwrap()).unwrap();
            drop((incoming, revived));
            assert_eq!(runtime.diagnostics().rooted, 1);
            panic!("unwind an active root region");
        }));
        assert!(result.is_err());
        assert_eq!(runtime.diagnostics().rooted, 0);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(runtime.enter_object_root_region().is_some());
    }
    #[test]
    fn allocation_collection_retires_host_payloads_before_reentrant_allocation() {
        use crate::runtime::Value;
        struct Reenter {
            runtime: Runtime,
            observed: Rc<RefCell<Option<super::super::WeakObject>>>,
        }
        impl Drop for Reenter {
            fn drop(&mut self) {
                let before = self.runtime.diagnostics().collections;
                assert!(self.runtime.collect_migration_graph().is_err());
                let object = Object::new_in(&self.runtime);
                object
                    .create_data_property("answer".into(), Value::Number(42.0))
                    .unwrap();
                *self.observed.borrow_mut() = Some(object.downgrade());
                assert_eq!(
                    self.runtime.diagnostics().collections,
                    before,
                    "nested allocations defer collection"
                );
            }
        }
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(0);
        let context = Object::new_in(&runtime);
        let observed = Rc::new(RefCell::new(None));
        let dropper = Reenter {
            runtime: runtime.clone(),
            observed: observed.clone(),
        };
        let function = Object::native_function_for(&context, move |_, _| {
            let _keep_capture = &dropper;
            Ok(Value::Undefined)
        });
        let weak_function = function.downgrade();
        drop((function, context));
        let object = Object::new_in(&runtime);
        assert!(
            weak_function.upgrade().is_none(),
            "slot reuse must not resurrect the old callable"
        );
        assert!(observed.borrow().as_ref().unwrap().upgrade().is_some());
        drop(object);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(observed.borrow().as_ref().unwrap().upgrade().is_none());
    }

    #[test]
    fn allocation_interval_collects_unrooted_cycles_without_number_roots() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(3);
        for _ in 0..12 {
            let object = Object::new_in(&runtime);
            object
                .create_data_property("self".into(), super::super::Value::Object(object.clone()))
                .unwrap();
        }
        assert_eq!(runtime.diagnostics().collections, 4);
        assert_eq!(runtime.diagnostics().rooted, 0);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
    #[test]
    fn native_functions_retain_source_realms_and_materialize_errors_there() {
        use crate::runtime::{IntrinsicId, Realm, RuntimeError, Value};
        let runtime = Runtime::new();
        let source = Realm::ordinary_in(&runtime);
        let caller = Realm::ordinary_in(&runtime);
        let weak_global = source.global_object().downgrade();
        let weak_error = source
            .intrinsic(IntrinsicId::TypeErrorPrototype)
            .downgrade();
        let function_prototype = source.intrinsic(IntrinsicId::FunctionPrototype);
        function_prototype.set_prototype(Some(caller.intrinsic(IntrinsicId::ObjectPrototype)));
        let function = source
            .create_native_function("fail", 0, |_, _| {
                Err(RuntimeError::type_error("source error"))
            })
            .unwrap();
        function.set_prototype(None);
        drop((source, function_prototype));
        runtime.collect_migration_graph().unwrap();
        assert!(weak_global.upgrade().is_some());
        let error = function.call(Value::Undefined, &[]).unwrap_err();
        let Some(Value::Object(exception)) = error.thrown_value() else {
            panic!("materialized error")
        };
        assert_eq!(exception.prototype(), weak_error.upgrade());
        assert_ne!(
            exception.prototype(),
            Some(caller.intrinsic(IntrinsicId::TypeErrorPrototype))
        );
        drop(function);
        runtime.collect_migration_graph().unwrap();
        assert!(
            weak_global.upgrade().is_some(),
            "escaped exception retains its prototype graph"
        );
        drop(error);
        runtime.collect_migration_graph().unwrap();
        assert!(weak_global.upgrade().is_none());
        drop(caller);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn realm_global_proxy_callbacks_observe_complete_tables_and_can_collect() {
        use crate::runtime::{IntrinsicId, ProxyObject, Realm, Value};
        let runtime = Runtime::new();
        let target = Object::new_in(&runtime);
        let handler = Object::new_in(&runtime);
        let callbacks = Rc::new(Cell::new(0usize));
        let callback_count = callbacks.clone();
        let callback_runtime = runtime.clone();
        let trap = Object::native_function_for(&target, move |_, args| {
            let Value::Object(descriptor) = &args[2] else {
                panic!("descriptor")
            };
            if let Value::Object(function) = descriptor.get(&"value".into())? {
                if let Some(source) = function.source_realm() {
                    let realm = source.load(&function)?;
                    assert!(realm.intrinsic(IntrinsicId::Number).is_callable());
                    assert!(realm.intrinsic(IntrinsicId::Number).is_constructor());
                    assert!(realm.intrinsic(IntrinsicId::BigInt).is_callable());
                    assert!(!realm.intrinsic(IntrinsicId::BigInt).is_constructor());
                    assert_eq!(realm.intrinsics().iter().count(), IntrinsicId::ALL.len());
                    callback_count.set(callback_count.get() + 1);
                }
            }
            callback_runtime.verify_migration_barriers().unwrap();
            callback_runtime.collect_migration_graph().unwrap();
            let Value::Object(target) = &args[0] else {
                panic!("target")
            };
            let descriptor = crate::runtime::PropertyDescriptor::from_object(descriptor)?;
            target
                .define_own_property(args[1].to_property_key()?, descriptor)
                .map(Value::Boolean)
        });
        handler
            .create_data_property("defineProperty".into(), Value::Object(trap.clone()))
            .unwrap();
        let global = ProxyObject::try_new(target.clone(), handler.clone())
            .unwrap()
            .object();
        let realm = Realm::try_new(global.clone(), Value::Object(global.clone())).unwrap();
        assert!(callbacks.get() > 10);
        handler.try_delete(&"defineProperty".into()).unwrap();
        drop((trap, target, handler, global, realm));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
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
        assert!(runtime.0.heap.borrow().with(realm_handle, |_| ()).is_some());
        assert_eq!(runtime.0.heap.borrow().root_count(realm_handle), 0);
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
        assert_eq!(runtime.diagnostics().live, 0);
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
                let super::super::stored::StoredObject(handle) = capture;
                let runtime = owner.0.runtime().unwrap();
                assert!(
                    runtime
                        .0
                        .heap
                        .borrow()
                        .with(*handle, |object| object.facade.borrow().upgrade().is_none())
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
    fn proxy_revocable_factory_traces_prototypes_and_releases_revoked_graphs() {
        let runtime = Runtime::new();
        let function_prototype = Object::new_in(&runtime);
        let object_prototype = Object::new_in(&runtime);
        let factory =
            crate::runtime::proxy::ProxyRevocable::create(&function_prototype, &object_prototype)
                .unwrap();
        for prototype in [&function_prototype, &object_prototype] {
            prototype
                .create_data_property("back".into(), Value::Object(factory.clone()))
                .unwrap();
        }
        let weak = [function_prototype.downgrade(), object_prototype.downgrade()];
        drop(function_prototype);
        drop(object_prototype);
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        let foreign_context = Runtime::new();
        let foreign = Object::new_in(&foreign_context);
        assert!(crate::runtime::proxy::ProxyRevocable::create(&factory, &foreign).is_err());
        assert_eq!(runtime.diagnostics().live, 3);
        let target = Object::new_in(&runtime);
        target
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let handler = Object::new_in(&runtime);
        let Value::Object(result) = factory
            .call(
                Value::Undefined,
                &[
                    Value::Object(target.clone()),
                    Value::Object(handler.clone()),
                ],
            )
            .unwrap()
        else {
            panic!("result")
        };
        assert_eq!(result.prototype(), weak[1].upgrade());
        let Value::Object(revoke) = result.get(&"revoke".into()).unwrap() else {
            panic!("revoker")
        };
        assert_eq!(revoke.prototype(), weak[0].upgrade());
        assert!(
            revoke
                .get(&"name".into())
                .unwrap()
                .same_value(&Value::String("".into()))
        );
        let weak_proxy = match result.get(&"proxy".into()).unwrap() {
            Value::Object(proxy) => proxy.downgrade(),
            _ => panic!("proxy"),
        };
        drop((target, handler, factory, result));
        runtime.verify_migration_barriers().unwrap();
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 7);
        assert!(!revoke.is_constructor());
        revoke.call(Value::Null, &[]).unwrap();
        revoke.call(Value::Null, &[]).unwrap();
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 4);
        assert!(weak_proxy.upgrade().is_none());
        drop(revoke);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.iter().all(|prototype| prototype.upgrade().is_none()));
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
    fn foreign_heap_edges_are_rejected_before_property_or_prototype_changes() {
        let runtime = Runtime::new();
        let other = Runtime::new();
        let owner = Object::new_in(&runtime);
        let third = Runtime::new();
        for foreign in [Object::new_in(&other), Object::new_in(&third)] {
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
