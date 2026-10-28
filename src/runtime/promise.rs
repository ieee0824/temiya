//! Promise resolution, reactions, thenable assimilation, and Job Queue integration.

use super::{
    JobQueue, JsString, Object, RuntimeError, RuntimeResult, Value,
    job_queue::{StoredJobQueue, TracedJob},
    object::ManagedCallable,
    stored::{StoredObject, StoredOwner, StoredRuntimeError, StoredValue},
};
use crate::gc::{Trace, Tracer};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Observable state of a Promise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromiseState {
    /// Not settled.
    Pending,
    /// Fulfilled with a value.
    Fulfilled,
    /// Rejected with a JavaScript reason or a host error.
    Rejected,
}

#[derive(Clone)]
enum Settlement {
    Pending,
    Fulfilled(StoredValue),
    Rejected(StoredRuntimeError),
}
impl Settlement {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        match self {
            Self::Fulfilled(value) => value.trace(tracer),
            Self::Rejected(error) => error.trace(tracer),
            Self::Pending => {}
        }
    }
}
#[derive(Clone)]
struct Reaction {
    on_fulfilled: Option<StoredObject>,
    on_rejected: Option<StoredObject>,
    result: StoredObject,
}
impl Reaction {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.result.trace(tracer);
        for callback in [&self.on_fulfilled, &self.on_rejected]
            .into_iter()
            .flatten()
        {
            callback.trace(tracer);
        }
    }
}

pub(super) struct PromiseData {
    settlement: Settlement,
    reactions: Vec<Reaction>,
    handled: bool,
    function_prototype: Option<StoredObject>,
    queue: StoredJobQueue,
}
impl PromiseData {
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        self.settlement.trace(tracer);
        for reaction in &self.reactions {
            reaction.trace(tracer);
        }
        if let Some(prototype) = &self.function_prototype {
            prototype.trace(tracer);
        }
        self.queue.trace(tracer);
    }
}

/// A Promise object associated with a FIFO Job Queue.
#[derive(Clone)]
pub struct Promise {
    object: Object,
    data: Rc<RefCell<PromiseData>>,
    queue: JobQueue,
}
impl std::fmt::Debug for Promise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Promise")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}
impl Promise {
    pub(super) fn settled_in(
        realm: &super::Realm,
        result: RuntimeResult<Value>,
    ) -> RuntimeResult<Self> {
        let promise = Self::pending_with_function_prototype(
            realm.job_queue(),
            Some(realm.intrinsic(super::IntrinsicId::FunctionPrototype)),
        )?;
        match result {
            Ok(value) => {
                promise.resolve(value)?;
            }
            Err(error) => {
                promise.reject(error)?;
            }
        }
        Ok(promise)
    }
    /// Creates a pending Promise with a fresh queue.
    pub fn pending() -> RuntimeResult<Self> {
        Self::pending_on(JobQueue::new())
    }
    /// Creates a pending Promise on the supplied queue.
    pub fn pending_on(queue: JobQueue) -> RuntimeResult<Self> {
        Self::pending_with_function_prototype(queue, None)
    }
    fn pending_with_function_prototype(
        queue: JobQueue,
        function_prototype: Option<Object>,
    ) -> RuntimeResult<Self> {
        if let Some(prototype) = &function_prototype {
            StoredObject::initial(queue.runtime(), prototype)?;
        }
        let object = queue.runtime().map_or_else(Object::new, Object::new_in);
        let data = Rc::new(RefCell::new(PromiseData {
            settlement: Settlement::Pending,
            reactions: Vec::new(),
            handled: false,
            function_prototype: function_prototype
                .as_ref()
                .map(|prototype| StoredObject::store(&object, prototype))
                .transpose()?,
            queue: StoredJobQueue::store(&object, &queue)?,
        }));
        object.0.state.borrow_mut().promise_data = Some(data.clone());
        let promise = Self {
            object,
            data,
            queue,
        };
        let then = promise.action_function(Action::Then)?;
        promise
            .object
            .create_data_property("then".into(), Value::Object(then))?;
        promise.update_state("pending")?;
        Ok(promise)
    }
    /// Wraps an object with Promise internal slots, preserving identity and queue.
    #[must_use]
    pub fn from_object(object: Object) -> Option<Self> {
        let data = object.0.state.borrow().promise_data.clone()?;
        let queue = data
            .borrow()
            .queue
            .load(&object)
            .expect("live Promise Job Queue");
        Some(Self {
            object,
            data,
            queue,
        })
    }
    /// Creates an already-fulfilled Promise.
    pub fn fulfilled(value: Value) -> RuntimeResult<Self> {
        Self::fulfilled_on(JobQueue::new(), value)
    }
    /// Creates an already-fulfilled Promise on the supplied queue.
    pub fn fulfilled_on(queue: JobQueue, value: Value) -> RuntimeResult<Self> {
        let promise = Self::pending_on(queue)?;
        promise.resolve(value)?;
        Ok(promise)
    }
    /// Creates an already-rejected Promise.
    pub fn rejected(error: RuntimeError) -> RuntimeResult<Self> {
        Self::rejected_on(JobQueue::new(), error)
    }
    /// Creates an already-rejected Promise on the supplied queue.
    pub fn rejected_on(queue: JobQueue, error: RuntimeError) -> RuntimeResult<Self> {
        let promise = Self::pending_on(queue)?;
        promise.reject(error)?;
        Ok(promise)
    }
    /// Returns the current state.
    #[must_use]
    pub fn state(&self) -> PromiseState {
        match &self.data.borrow().settlement {
            Settlement::Pending => PromiseState::Pending,
            Settlement::Fulfilled(_) => PromiseState::Fulfilled,
            Settlement::Rejected(_) => PromiseState::Rejected,
        }
    }
    /// Returns the backing JavaScript object.
    #[must_use]
    pub fn object(&self) -> Object {
        self.object.clone()
    }
    /// Returns the associated queue.
    #[must_use]
    pub fn job_queue(&self) -> JobQueue {
        self.queue.clone()
    }

    /// Resolves once, recursively assimilating Promises and callable thenables.
    pub fn resolve(&self, value: Value) -> RuntimeResult<bool> {
        if !matches!(self.data.borrow().settlement, Settlement::Pending) {
            return Ok(false);
        }
        if let Value::Object(object) = &value {
            StoredObject::check_context(&self.object, object)?;
            if object == &self.object {
                return self.reject(RuntimeError::type_error(
                    "a Promise cannot resolve to itself",
                ));
            }
            if let Some(source) = Self::from_object(object.clone()) {
                source.add_reaction(None, None, self)?;
                return Ok(true);
            }
            let then = match object.get(&"then".into()) {
                Ok(value) => value,
                Err(error) => {
                    self.reject(error)?;
                    return Ok(true);
                }
            };
            if let Value::Object(then) = then {
                if then.is_callable() {
                    self.queue.enqueue_traced(ThenableJob {
                        target: StoredObject::initial(self.queue.runtime(), &self.object)?,
                        thenable: StoredObject::initial(self.queue.runtime(), object)?,
                        then: StoredObject::initial(self.queue.runtime(), &then)?,
                    })?;
                    return Ok(true);
                }
            }
        }
        let value = StoredValue::store_owned(&self.object, value)?;
        self.settle(Settlement::Fulfilled(value))
    }
    /// Rejects once, retaining arbitrary JavaScript throw payloads without coercion.
    pub fn reject(&self, error: RuntimeError) -> RuntimeResult<bool> {
        if !matches!(self.data.borrow().settlement, Settlement::Pending) {
            return Ok(false);
        }
        self.settle(Settlement::Rejected(StoredRuntimeError::store(
            &self.object,
            error,
        )?))
    }
    /// Rejects once with an arbitrary JavaScript value, without coercion.
    pub fn reject_value(&self, reason: Value) -> RuntimeResult<bool> {
        self.reject(RuntimeError::thrown(reason))
    }
    /// Registers reactions and returns their derived Promise.
    pub fn then(
        &self,
        on_fulfilled: Option<Object>,
        on_rejected: Option<Object>,
    ) -> RuntimeResult<Self> {
        for callback in [&on_fulfilled, &on_rejected].into_iter().flatten() {
            StoredObject::check_context(&self.object, callback)?;
        }
        let prototype = self
            .data
            .borrow()
            .function_prototype
            .as_ref()
            .map(|prototype| prototype.load(&self.object))
            .transpose()?;
        let result = Self::pending_with_function_prototype(self.queue.clone(), prototype)?;
        self.add_reaction(on_fulfilled.as_ref(), on_rejected.as_ref(), &result)?;
        Ok(result)
    }
    /// Extracts the result at the synchronous await boundary and marks it handled.
    pub fn await_result(&self) -> RuntimeResult<Value> {
        let mut data = self.data.borrow_mut();
        let result = match &data.settlement {
            Settlement::Pending => Err(RuntimeError::internal_error(
                "awaited Promise is still pending",
            )),
            Settlement::Fulfilled(value) => value.load(&self.object),
            Settlement::Rejected(error) => Err(error.load(&self.object)?),
        };
        if !matches!(data.settlement, Settlement::Pending) {
            data.handled = true;
        }
        result
    }
    fn settle(&self, settlement: Settlement) -> RuntimeResult<bool> {
        {
            let mut data = self.data.borrow_mut();
            if !matches!(data.settlement, Settlement::Pending) {
                return Ok(false);
            }
            data.settlement = settlement.clone();
        }
        // Keep pending reactions traced by the Promise across an observable
        // state setter. After taking them, publication invokes no JS/safepoints.
        self.update_state(if matches!(settlement, Settlement::Rejected(_)) {
            "rejected"
        } else {
            "fulfilled"
        })?;
        let reactions = std::mem::take(&mut self.data.borrow_mut().reactions);
        for reaction in reactions {
            self.queue.enqueue_traced(ReactionJob {
                settlement: settlement.clone(),
                reaction,
            })?;
        }
        if let Settlement::Rejected(error) = settlement {
            self.queue.enqueue_traced(UnhandledJob {
                data: Rc::downgrade(&self.data),
                error,
            })?;
        }
        Ok(true)
    }
    fn add_reaction(
        &self,
        on_fulfilled: Option<&Object>,
        on_rejected: Option<&Object>,
        result: &Promise,
    ) -> RuntimeResult<()> {
        let reaction = Reaction {
            on_fulfilled: on_fulfilled
                .map(|callback| StoredObject::store(&self.object, callback))
                .transpose()?,
            on_rejected: on_rejected
                .map(|callback| StoredObject::store(&self.object, callback))
                .transpose()?,
            result: StoredObject::store(&self.object, &result.object)?,
        };
        let settlement = {
            let mut data = self.data.borrow_mut();
            data.handled = true;
            if matches!(data.settlement, Settlement::Pending) {
                data.reactions.push(reaction);
                return Ok(());
            }
            data.settlement.clone()
        };
        self.queue.enqueue_traced(ReactionJob {
            settlement,
            reaction,
        })
    }
    fn action_function(&self, action: Action) -> RuntimeResult<Object> {
        let prototype = self
            .data
            .borrow()
            .function_prototype
            .as_ref()
            .map(|prototype| prototype.load(&self.object))
            .transpose()?;
        let capture = PromiseAction {
            target: StoredObject::initial(self.object.0.runtime(), &self.object)?,
            action,
        };
        let (name, length) = if matches!(action, Action::Then) {
            ("then", 2)
        } else {
            ("", 1)
        };
        Ok(match prototype {
            Some(prototype) => Object::managed_function_with_metadata(
                &prototype,
                name,
                length,
                ManagedCallable::Promise(capture),
            ),
            None => {
                let function = Object::new_for(&self.object);
                function.set_managed_callable(ManagedCallable::Promise(capture));
                function
            }
        })
    }
    fn update_state(&self, state: &str) -> RuntimeResult<()> {
        self.object
            .set(&"state".into(), Value::String(JsString::from(state)))?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Action {
    Then,
    Resolve,
    Reject,
}
pub(super) struct PromiseAction {
    target: StoredObject,
    action: Action,
}
impl PromiseAction {
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        self.target.trace(tracer);
    }
    pub(super) fn call(&self, owner: &Object, arguments: &[Value]) -> RuntimeResult<Value> {
        let source = Promise::from_object(self.target.load(owner)?)
            .ok_or_else(|| RuntimeError::internal_error("Promise slot missing"))?;
        match self.action {
            Action::Then => source
                .then(callable(arguments.first())?, callable(arguments.get(1))?)
                .map(|promise| Value::Object(promise.object())),
            Action::Resolve => {
                source.resolve(arguments.first().cloned().unwrap_or(Value::Undefined))?;
                Ok(Value::Undefined)
            }
            Action::Reject => {
                source.reject_value(arguments.first().cloned().unwrap_or(Value::Undefined))?;
                Ok(Value::Undefined)
            }
        }
    }
}
struct ThenableJob {
    target: StoredObject,
    thenable: StoredObject,
    then: StoredObject,
}
impl Trace for ThenableJob {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.target.trace(tracer);
        self.thenable.trace(tracer);
        self.then.trace(tracer);
    }
}
impl TracedJob for ThenableJob {
    fn run(&self, queue: &JobQueue) -> RuntimeResult<()> {
        let target = Promise::from_object(self.target.load(queue)?)
            .ok_or_else(|| RuntimeError::internal_error("Promise slot missing"))?;
        let thenable = self.thenable.load(queue)?;
        let then = self.then.load(queue)?;
        let resolve = target.action_function(Action::Resolve)?;
        let reject = target.action_function(Action::Reject)?;
        if let Err(error) = then.call(
            Value::Object(thenable),
            &[Value::Object(resolve), Value::Object(reject)],
        ) {
            target.reject(error)?;
        }
        Ok(())
    }
}
struct UnhandledJob {
    data: Weak<RefCell<PromiseData>>,
    error: StoredRuntimeError,
}
impl Trace for UnhandledJob {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.error.trace(tracer);
    }
}
impl TracedJob for UnhandledJob {
    fn run(&self, queue: &JobQueue) -> RuntimeResult<()> {
        let report = self
            .data
            .upgrade()
            .is_some_and(|data| !data.borrow().handled);
        if report {
            queue.report_unhandled(self.error.load(queue)?);
        }
        Ok(())
    }
}
struct ReactionJob {
    settlement: Settlement,
    reaction: Reaction,
}
impl Trace for ReactionJob {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        self.settlement.trace(tracer);
        self.reaction.trace(tracer);
    }
}
impl TracedJob for ReactionJob {
    fn run(&self, queue: &JobQueue) -> RuntimeResult<()> {
        let result = Promise::from_object(self.reaction.result.load(queue)?)
            .ok_or_else(|| RuntimeError::internal_error("Promise slot missing"))?;
        let outcome = match &self.settlement {
            Settlement::Fulfilled(value) => {
                let value = value.load(queue)?;
                match &self.reaction.on_fulfilled {
                    Some(callback) => callback.load(queue)?.call(Value::Undefined, &[value]),
                    None => return result.resolve(value).map(|_| ()),
                }
            }
            Settlement::Rejected(error) => {
                let error = error.load(queue)?;
                match &self.reaction.on_rejected {
                    Some(callback) => {
                        let reason = error
                            .thrown_value()
                            .cloned()
                            .unwrap_or_else(|| Value::String(JsString::from(error.to_string())));
                        callback.load(queue)?.call(Value::Undefined, &[reason])
                    }
                    None => return result.reject(error).map(|_| ()),
                }
            }
            Settlement::Pending => unreachable!(),
        };
        match outcome {
            Ok(value) => {
                result.resolve(value)?;
            }
            Err(error) => {
                result.reject(error)?;
            }
        }
        Ok(())
    }
}
fn callable(value: Option<&Value>) -> RuntimeResult<Option<Object>> {
    match value {
        None | Some(Value::Undefined) => Ok(None),
        Some(Value::Object(object)) if object.is_callable() => Ok(Some(object.clone())),
        _ => Err(RuntimeError::type_error("Promise reaction is not callable")),
    }
}
pub(super) fn settled_promise_object(
    realm: &super::Realm,
    result: RuntimeResult<Value>,
) -> RuntimeResult<Object> {
    Promise::settled_in(realm, result).map(|promise| promise.object)
}
pub(super) fn await_value(value: Value) -> RuntimeResult<Value> {
    let Value::Object(object) = &value else {
        return Ok(value);
    };
    Promise::from_object(object.clone()).map_or(Ok(value), |promise| promise.await_result())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{PropertyDescriptor, Runtime};

    #[test]
    fn managed_promise_slots_and_escaped_then_functions_have_no_registry_roots() {
        let runtime = Runtime::new();
        let queue = JobQueue::new_in(&runtime);
        let promise = Promise::pending_on(queue.clone()).unwrap();
        let object = promise.object();
        let weak = object.downgrade();
        let Value::Object(then) = object.get(&"then".into()).unwrap() else {
            panic!("then")
        };
        drop((promise, object, queue));
        assert_eq!(runtime.diagnostics().rooted, 1);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 3);
        let derived = then.call(Value::Undefined, &[]).unwrap();
        assert!(
            matches!(&derived, Value::Object(object) if Promise::from_object(object.clone()).is_some())
        );
        drop((derived, then));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn managed_settlements_and_reactions_survive_gc_and_release_cycles_after_jobs() {
        for rejected in [false, true] {
            let runtime = Runtime::new();
            let queue = JobQueue::new_in(&runtime);
            let source = Promise::pending_on(queue.clone()).unwrap();
            runtime.collect_migration_graph().unwrap();
            let value = Object::new_in(&runtime);
            value
                .create_data_property("source".into(), Value::Object(source.object()))
                .unwrap();
            value
                .create_data_property("answer".into(), Value::Number(42.0))
                .unwrap();
            let weak = [value.downgrade(), source.object().downgrade()];
            let callback_runtime = runtime.clone();
            let callback = Object::native_function_for(&value, move |_, args| {
                callback_runtime.verify_migration_barriers().unwrap();
                callback_runtime.collect_migration_graph().unwrap();
                let Value::Object(value) = &args[0] else {
                    panic!("payload")
                };
                value.get(&"answer".into())
            });
            let derived = if rejected {
                source.reject_value(Value::Object(value.clone())).unwrap();
                source.then(None, Some(callback)).unwrap()
            } else {
                source.resolve(Value::Object(value.clone())).unwrap();
                source.then(Some(callback), None).unwrap()
            };
            drop((value, source));
            runtime.verify_migration_barriers().unwrap();
            runtime.collect_migration_graph().unwrap();
            assert!(weak.iter().all(|object| object.upgrade().is_some()));
            assert_eq!(queue.drain().unwrap(), if rejected { 2 } else { 1 });
            assert!(
                derived
                    .await_result()
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
            runtime.collect_migration_graph().unwrap();
            assert!(weak.iter().all(|object| object.upgrade().is_none()));
            drop((derived, queue));
            assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        }
    }

    #[test]
    fn state_setter_collection_keeps_unpublished_reactions_alive() {
        let runtime = Runtime::new();
        let queue = JobQueue::new_in(&runtime);
        let source = Promise::pending_on(queue.clone()).unwrap();
        let callback_runtime = runtime.clone();
        let callback = Object::native_function_for(&source.object, move |_, args| {
            callback_runtime.collect_migration_graph().unwrap();
            Ok(Value::Number(args[0].to_number()? + 22.0))
        });
        let derived = source.then(Some(callback), None).unwrap();
        let weak = derived.object().downgrade();
        drop(derived);
        let setter_runtime = runtime.clone();
        let setter = Object::native_function_for(&source.object, move |_, _| {
            setter_runtime.verify_migration_barriers().unwrap();
            setter_runtime.collect_migration_graph().unwrap();
            Ok(Value::Undefined)
        });
        source
            .object
            .define_own_property(
                "state".into(),
                PropertyDescriptor::accessor(None, Some(setter), true, true),
            )
            .unwrap();
        source.resolve(Value::Number(20.0)).unwrap();
        queue.drain().unwrap();
        let derived = Promise::from_object(weak.upgrade().unwrap()).unwrap();
        assert!(
            derived
                .await_result()
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((derived, source, queue));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn thenable_resolvers_retain_their_target_after_dequeue_and_can_escape() {
        let runtime = Runtime::new();
        let queue = JobQueue::new_in(&runtime);
        let source = Promise::pending_on(queue.clone()).unwrap();
        let weak = source.object().downgrade();
        let escaped = Rc::new(RefCell::new(None));
        let callback_escape = escaped.clone();
        let callback_runtime = runtime.clone();
        let thenable = Object::new_in(&runtime);
        let then = Object::native_function_for(&thenable, move |_, args| {
            callback_runtime.verify_migration_barriers().unwrap();
            callback_runtime.collect_migration_graph().unwrap();
            let Value::Object(resolve) = &args[0] else {
                panic!("resolve")
            };
            *callback_escape.borrow_mut() = Some(resolve.clone());
            resolve.call(Value::Undefined, &[Value::Number(42.0)])?;
            let Value::Object(reject) = &args[1] else {
                panic!("reject")
            };
            reject.call(Value::Undefined, &[Value::Number(1.0)])?;
            Ok(Value::Undefined)
        });
        thenable
            .create_data_property("then".into(), Value::Object(then))
            .unwrap();
        source.resolve(Value::Object(thenable)).unwrap();
        drop(source);
        runtime.collect_migration_graph().unwrap();
        assert_eq!(queue.drain().unwrap(), 1);
        drop(queue);
        runtime.collect_migration_graph().unwrap();
        let resolver = escaped.borrow_mut().take().unwrap();
        assert!(!resolver.is_constructor());
        resolver
            .call(Value::Undefined, &[Value::Number(99.0)])
            .unwrap();
        let source = Promise::from_object(weak.upgrade().unwrap()).unwrap();
        assert!(
            source
                .await_result()
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((source, resolver));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn unhandled_error_payload_survives_source_drop_and_collection_in_the_hook() {
        let runtime = Runtime::new();
        let queue = JobQueue::new_in(&runtime);
        let source = Promise::pending_on(queue.clone()).unwrap();
        let weak_source = source.object().downgrade();
        runtime.collect_migration_graph().unwrap();
        let reason = Object::new_in(&runtime);
        reason
            .create_data_property("answer".into(), Value::Number(42.0))
            .unwrap();
        let weak_reason = reason.downgrade();
        source.reject_value(Value::Object(reason)).unwrap();
        let holder = Rc::new(RefCell::new(Some(source)));
        let hook_holder = holder.clone();
        let callback_runtime = runtime.clone();
        queue.set_unhandled_rejection_hook(move |error| {
            drop(hook_holder.borrow_mut().take());
            callback_runtime.verify_migration_barriers().unwrap();
            callback_runtime.collect_migration_graph().unwrap();
            let Some(Value::Object(reason)) = error.thrown_value() else {
                panic!("reason")
            };
            assert!(
                reason
                    .get(&"answer".into())
                    .unwrap()
                    .same_value(&Value::Number(42.0))
            );
        });
        runtime.verify_migration_barriers().unwrap();
        runtime.collect_migration_graph().unwrap();
        assert_eq!(queue.drain().unwrap(), 1);
        assert!(holder.borrow().is_none());
        assert!(weak_source.upgrade().is_none());
        queue.set_unhandled_rejection_hook(|_| {});
        runtime.collect_migration_graph().unwrap();
        assert!(weak_reason.upgrade().is_none());
        drop(queue);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn adoption_jobs_keep_derived_promises_and_other_queues_alive() {
        let runtime = Runtime::new();
        let source_queue = JobQueue::new_in(&runtime);
        let target_queue = JobQueue::new_in(&runtime);
        let source = Promise::pending_on(source_queue.clone()).unwrap();
        let target = Promise::pending_on(target_queue.clone()).unwrap();
        let weak = target.object().downgrade();
        target.resolve(Value::Object(source.object())).unwrap();
        source.resolve(Value::Number(42.0)).unwrap();
        drop((source, target, target_queue));
        runtime.collect_migration_graph().unwrap();
        assert_eq!(source_queue.drain().unwrap(), 1);
        let target = Promise::from_object(weak.upgrade().unwrap()).unwrap();
        assert!(
            target
                .await_result()
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((target, source_queue));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn foreign_promise_inputs_do_not_change_state_or_allocate_reactions() {
        let runtime = Runtime::new();
        let queue = JobQueue::new_in(&runtime);
        let source = Promise::pending_on(queue.clone()).unwrap();
        let foreign_runtime = Runtime::new();
        let foreign = Object::new_in(&foreign_runtime);
        let before = runtime.diagnostics().live;
        assert!(source.resolve(Value::Object(foreign.clone())).is_err());
        assert!(source.reject_value(Value::Object(foreign.clone())).is_err());
        assert!(source.then(Some(foreign), None).is_err());
        assert_eq!(source.state(), PromiseState::Pending);
        assert_eq!(runtime.diagnostics().live, before);
        assert!(queue.is_empty());
        drop((source, queue));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }
}
