//! Promise values used by async functions and `await`.

use std::{cell::RefCell, rc::Rc};

use super::{JsString, Object, RuntimeError, RuntimeResult, Value};

/// Observable state of a Promise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromiseState {
    /// No settlement has occurred yet.
    Pending,
    /// The Promise was fulfilled with a value.
    Fulfilled,
    /// The Promise was rejected with a runtime error.
    Rejected,
}

#[derive(Clone)]
enum Settlement {
    Pending,
    Fulfilled(Value),
    Rejected(RuntimeError),
}

/// A Promise object and its engine-managed settlement state.
#[derive(Clone)]
pub struct Promise {
    object: Object,
    settlement: Rc<RefCell<Settlement>>,
}

impl std::fmt::Debug for Promise {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Promise")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Promise {
    /// Creates a pending Promise for host or Job Queue integration.
    pub fn pending() -> RuntimeResult<Self> {
        Self::new(Settlement::Pending)
    }

    /// Creates an already-fulfilled Promise.
    pub fn fulfilled(value: Value) -> RuntimeResult<Self> {
        Self::new(Settlement::Fulfilled(value))
    }

    /// Creates an already-rejected Promise.
    pub fn rejected(error: RuntimeError) -> RuntimeResult<Self> {
        Self::new(Settlement::Rejected(error))
    }

    fn new(settlement: Settlement) -> RuntimeResult<Self> {
        let object = Object::new();
        let settlement = Rc::new(RefCell::new(settlement));
        install_then(&object, settlement.clone())?;
        object.create_data_property(
            "state".into(),
            Value::String(JsString::from(match &*settlement.borrow() {
                Settlement::Pending => "pending",
                Settlement::Fulfilled(_) => "fulfilled",
                Settlement::Rejected(_) => "rejected",
            })),
        )?;
        Ok(Self { object, settlement })
    }

    /// Returns the current Promise state.
    #[must_use]
    pub fn state(&self) -> PromiseState {
        match &*self.settlement.borrow() {
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

    /// Extracts a settled value as performed by `await`.
    pub fn await_result(&self) -> RuntimeResult<Value> {
        match &*self.settlement.borrow() {
            Settlement::Pending => Err(RuntimeError::internal_error(
                "awaited Promise is still pending without a Job Queue",
            )),
            Settlement::Fulfilled(value) => Ok(value.clone()),
            Settlement::Rejected(error) => Err(error.clone()),
        }
    }
}

thread_local! {
    static PROMISES: RefCell<Vec<(u64, Rc<RefCell<Settlement>>)>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn settled_promise_object(result: RuntimeResult<Value>) -> RuntimeResult<Object> {
    let promise = match result {
        Ok(value) => Promise::fulfilled(value)?,
        Err(error) => Promise::rejected(error)?,
    };
    PROMISES.with(|promises| {
        promises
            .borrow_mut()
            .push((promise.object.identity(), promise.settlement.clone()));
    });
    Ok(promise.object)
}

pub(super) fn await_value(value: Value) -> RuntimeResult<Value> {
    let Value::Object(object) = &value else {
        return Ok(value);
    };
    let settlement = PROMISES.with(|promises| {
        promises
            .borrow()
            .iter()
            .find(|(identity, _)| *identity == object.identity())
            .map(|(_, settlement)| settlement.clone())
    });
    let Some(settlement) = settlement else {
        return Ok(value);
    };
    match &*settlement.borrow() {
        Settlement::Pending => Err(RuntimeError::internal_error(
            "awaited Promise is still pending without a Job Queue",
        )),
        Settlement::Fulfilled(value) => Ok(value.clone()),
        Settlement::Rejected(error) => Err(error.clone()),
    }
}

fn install_then(object: &Object, settlement: Rc<RefCell<Settlement>>) -> RuntimeResult<()> {
    object.create_data_property(
        "then".into(),
        Value::Object(Object::native_function(move |_, arguments| {
            let current = settlement.borrow().clone();
            let result = match current {
                Settlement::Fulfilled(value) => match arguments.first() {
                    Some(Value::Object(callback)) if callback.is_callable() => {
                        callback.call(Value::Undefined, &[value])
                    }
                    _ => Ok(value),
                },
                Settlement::Rejected(error) => match arguments.get(1) {
                    Some(Value::Object(callback)) if callback.is_callable() => callback.call(
                        Value::Undefined,
                        &[Value::String(JsString::from(error.to_string()))],
                    ),
                    _ => Err(error),
                },
                Settlement::Pending => Err(RuntimeError::internal_error(
                    "pending Promise reactions require the Job Queue from milestone #29",
                )),
            };
            settled_promise_object(result).map(Value::Object)
        })),
    )?;
    Ok(())
}
