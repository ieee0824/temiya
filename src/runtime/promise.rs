//! Promise resolution, reactions, thenable assimilation, and Job Queue integration.

use super::{JobQueue, JsString, Object, RuntimeError, RuntimeResult, Value};
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
    /// Rejected with an error.
    Rejected,
}

#[derive(Clone)]
enum Settlement {
    Pending,
    Fulfilled(Value),
    Rejected(RuntimeError),
}

#[derive(Clone)]
struct Reaction {
    on_fulfilled: Option<Object>,
    on_rejected: Option<Object>,
    result: Promise,
}

struct PromiseData {
    settlement: Settlement,
    reactions: Vec<Reaction>,
    handled: bool,
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
    /// Creates a pending Promise with a fresh queue.
    pub fn pending() -> RuntimeResult<Self> {
        Self::pending_on(JobQueue::new())
    }

    /// Creates a pending Promise on the supplied queue.
    pub fn pending_on(queue: JobQueue) -> RuntimeResult<Self> {
        let promise = Self {
            object: Object::new(),
            data: Rc::new(RefCell::new(PromiseData {
                settlement: Settlement::Pending,
                reactions: Vec::new(),
                handled: false,
            })),
            queue,
        };
        promise.install_methods()?;
        promise.update_state("pending")?;
        register(&promise);
        Ok(promise)
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
        if matches!(&value, Value::Object(object) if object == &self.object) {
            return self.reject(RuntimeError::type_error(
                "a Promise cannot resolve to itself",
            ));
        }
        if let Value::Object(object) = &value {
            if let Some(source) = lookup(object) {
                source.add_reaction(Reaction {
                    on_fulfilled: None,
                    on_rejected: None,
                    result: self.clone(),
                })?;
                return Ok(true);
            }
            let then_value = match object.get(&"then".into()) {
                Ok(value) => value,
                Err(error) => {
                    self.reject(error)?;
                    return Ok(true);
                }
            };
            if let Value::Object(then) = then_value {
                if then.is_callable() {
                    let promise = self.clone();
                    let thenable = object.clone();
                    self.queue.enqueue(move || {
                        let resolve_target = promise.clone();
                        let resolve = Object::native_function(move |_, arguments| {
                            resolve_target
                                .resolve(arguments.first().cloned().unwrap_or(Value::Undefined))?;
                            Ok(Value::Undefined)
                        });
                        let reject_target = promise.clone();
                        let reject = Object::native_function(move |_, arguments| {
                            let reason = arguments.first().cloned().unwrap_or(Value::Undefined);
                            reject_target.reject(RuntimeError::type_error(format!(
                                "thenable rejected: {reason:?}"
                            )))?;
                            Ok(Value::Undefined)
                        });
                        if let Err(error) = then.call(
                            Value::Object(thenable),
                            &[Value::Object(resolve), Value::Object(reject)],
                        ) {
                            promise.reject(error)?;
                        }
                        Ok(())
                    });
                    return Ok(true);
                }
            }
        }
        self.fulfill(value)
    }

    /// Rejects once; later attempts return false.
    pub fn reject(&self, error: RuntimeError) -> RuntimeResult<bool> {
        self.settle(Settlement::Rejected(error))
    }

    /// Registers reactions and returns their derived Promise.
    pub fn then(
        &self,
        on_fulfilled: Option<Object>,
        on_rejected: Option<Object>,
    ) -> RuntimeResult<Self> {
        let result = Self::pending_on(self.queue.clone())?;
        self.add_reaction(Reaction {
            on_fulfilled,
            on_rejected,
            result: result.clone(),
        })?;
        Ok(result)
    }

    /// Extracts the result at the synchronous await boundary.
    pub fn await_result(&self) -> RuntimeResult<Value> {
        match &self.data.borrow().settlement {
            Settlement::Pending => Err(RuntimeError::internal_error(
                "awaited Promise is still pending",
            )),
            Settlement::Fulfilled(value) => Ok(value.clone()),
            Settlement::Rejected(error) => Err(error.clone()),
        }
    }

    fn fulfill(&self, value: Value) -> RuntimeResult<bool> {
        self.settle(Settlement::Fulfilled(value))
    }

    fn settle(&self, settlement: Settlement) -> RuntimeResult<bool> {
        let (reactions, rejected) = {
            let mut data = self.data.borrow_mut();
            if !matches!(data.settlement, Settlement::Pending) {
                return Ok(false);
            }
            let rejected = matches!(settlement, Settlement::Rejected(_));
            data.settlement = settlement.clone();
            (std::mem::take(&mut data.reactions), rejected)
        };
        self.update_state(if rejected { "rejected" } else { "fulfilled" })?;
        for reaction in reactions {
            enqueue_reaction(self.queue.clone(), settlement.clone(), reaction);
        }
        if let Settlement::Rejected(error) = settlement {
            let weak = Rc::downgrade(&self.data);
            let queue = self.queue.clone();
            self.queue.enqueue(move || {
                if weak.upgrade().is_some_and(|data| !data.borrow().handled) {
                    queue.report_unhandled(error);
                }
                Ok(())
            });
        }
        Ok(true)
    }

    fn add_reaction(&self, reaction: Reaction) -> RuntimeResult<()> {
        let settled = {
            let mut data = self.data.borrow_mut();
            data.handled = true;
            if matches!(data.settlement, Settlement::Pending) {
                data.reactions.push(reaction.clone());
                None
            } else {
                Some(data.settlement.clone())
            }
        };
        if let Some(settlement) = settled {
            enqueue_reaction(self.queue.clone(), settlement, reaction);
        }
        Ok(())
    }

    fn install_methods(&self) -> RuntimeResult<()> {
        let source = self.clone();
        self.object.create_data_property(
            "then".into(),
            Value::Object(Object::native_function(move |_, arguments| {
                let fulfilled = callable(arguments.first())?;
                let rejected = callable(arguments.get(1))?;
                source
                    .then(fulfilled, rejected)
                    .map(|promise| Value::Object(promise.object()))
            })),
        )?;
        Ok(())
    }

    fn update_state(&self, state: &str) -> RuntimeResult<()> {
        self.object
            .set(&"state".into(), Value::String(JsString::from(state)))?;
        Ok(())
    }
}

struct RegistryEntry {
    identity: u64,
    data: Weak<RefCell<PromiseData>>,
    object: Object,
    queue: JobQueue,
}

thread_local! { static PROMISES: RefCell<Vec<RegistryEntry>> = const { RefCell::new(Vec::new()) }; }

fn register(promise: &Promise) {
    PROMISES.with(|promises| {
        promises.borrow_mut().push(RegistryEntry {
            identity: promise.object.identity(),
            data: Rc::downgrade(&promise.data),
            object: promise.object.clone(),
            queue: promise.queue.clone(),
        })
    });
}

fn lookup(object: &Object) -> Option<Promise> {
    PROMISES.with(|promises| {
        let mut entries = promises.borrow_mut();
        entries.retain(|entry| entry.data.strong_count() > 0);
        entries.iter().find_map(|entry| {
            if entry.identity != object.identity() {
                return None;
            }
            Some(Promise {
                object: entry.object.clone(),
                data: entry.data.upgrade()?,
                queue: entry.queue.clone(),
            })
        })
    })
}

fn callable(value: Option<&Value>) -> RuntimeResult<Option<Object>> {
    match value {
        None | Some(Value::Undefined) => Ok(None),
        Some(Value::Object(object)) if object.is_callable() => Ok(Some(object.clone())),
        _ => Err(RuntimeError::type_error("Promise reaction is not callable")),
    }
}

fn enqueue_reaction(queue: JobQueue, settlement: Settlement, reaction: Reaction) {
    queue.enqueue(move || {
        let result = match settlement {
            Settlement::Fulfilled(value) => match reaction.on_fulfilled {
                Some(callback) => callback.call(Value::Undefined, &[value]),
                None => return reaction.result.resolve(value).map(|_| ()),
            },
            Settlement::Rejected(error) => match reaction.on_rejected {
                Some(callback) => callback.call(
                    Value::Undefined,
                    &[Value::String(JsString::from(error.to_string()))],
                ),
                None => return reaction.result.reject(error).map(|_| ()),
            },
            Settlement::Pending => unreachable!(),
        };
        match result {
            Ok(value) => {
                reaction.result.resolve(value)?;
            }
            Err(error) => {
                reaction.result.reject(error)?;
            }
        }
        Ok(())
    });
}

pub(super) fn settled_promise_object(
    queue: JobQueue,
    result: RuntimeResult<Value>,
) -> RuntimeResult<Object> {
    let promise = match result {
        Ok(value) => Promise::fulfilled_on(queue, value)?,
        Err(error) => Promise::rejected_on(queue, error)?,
    };
    Ok(promise.object)
}

pub(super) fn await_value(value: Value) -> RuntimeResult<Value> {
    let Value::Object(object) = &value else {
        return Ok(value);
    };
    lookup(object).map_or(Ok(value), |promise| promise.await_result())
}
