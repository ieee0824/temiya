use std::{cell::RefCell, rc::Rc};

use temiya::runtime::{
    JobQueue, Object, Promise, PromiseState, PropertyDescriptor, RuntimeError, Value,
};

#[test]
fn settlement_is_one_shot_and_self_resolution_rejects() {
    let queue = JobQueue::new();
    let promise = Promise::pending_on(queue.clone()).unwrap();
    assert!(promise.resolve(Value::Number(1.0)).unwrap());
    assert!(!promise.resolve(Value::Number(2.0)).unwrap());
    assert!(!promise.reject(RuntimeError::type_error("late")).unwrap());
    assert!(
        promise
            .await_result()
            .unwrap()
            .same_value(&Value::Number(1.0))
    );

    let self_resolving = Promise::pending_on(queue).unwrap();
    assert!(
        self_resolving
            .resolve(Value::Object(self_resolving.object()))
            .unwrap()
    );
    assert_eq!(self_resolving.state(), PromiseState::Rejected);
}

#[test]
fn reactions_are_fifo_jobs_and_chaining_adopts_callback_results() {
    let queue = JobQueue::new();
    let promise = Promise::fulfilled_on(queue.clone(), Value::Number(3.0)).unwrap();
    let order = Rc::new(RefCell::new(Vec::new()));
    let first_order = order.clone();
    let first = Object::native_function(move |_, arguments| {
        first_order.borrow_mut().push(1);
        Ok(Value::Number(arguments[0].to_number()? + 1.0))
    });
    let second_order = order.clone();
    let second = Object::native_function(move |_, _| {
        second_order.borrow_mut().push(2);
        Ok(Value::Undefined)
    });
    let chained = promise.then(Some(first), None).unwrap();
    promise.then(Some(second), None).unwrap();

    assert!(order.borrow().is_empty());
    assert_eq!(chained.state(), PromiseState::Pending);
    assert_eq!(queue.drain().unwrap(), 2);
    assert_eq!(&*order.borrow(), &[1, 2]);
    assert!(
        chained
            .await_result()
            .unwrap()
            .same_value(&Value::Number(4.0))
    );
}

#[test]
fn thenables_are_assimilated_once_by_a_queued_job() {
    let queue = JobQueue::new();
    let thenable = Object::new();
    thenable
        .create_data_property(
            "then".into(),
            Value::Object(Object::native_function(|_, arguments| {
                let Value::Object(resolve) = &arguments[0] else {
                    panic!("resolve callback")
                };
                let Value::Object(reject) = &arguments[1] else {
                    panic!("reject callback")
                };
                resolve.call(Value::Undefined, &[Value::Number(8.0)])?;
                resolve.call(Value::Undefined, &[Value::Number(9.0)])?;
                reject.call(Value::Undefined, &[Value::String("late".into())])?;
                Ok(Value::Undefined)
            })),
        )
        .unwrap();
    let promise = Promise::pending_on(queue.clone()).unwrap();
    promise.resolve(Value::Object(thenable)).unwrap();

    assert_eq!(promise.state(), PromiseState::Pending);
    queue.drain().unwrap();
    assert!(
        promise
            .await_result()
            .unwrap()
            .same_value(&Value::Number(8.0))
    );
}

#[test]
fn throwing_then_getter_rejects_the_promise() {
    let queue = JobQueue::new();
    let thenable = Object::new();
    thenable
        .define_own_property(
            "then".into(),
            PropertyDescriptor::accessor(
                Some(Object::native_function(|_, _| {
                    Err(RuntimeError::type_error("getter failed"))
                })),
                None,
                true,
                true,
            ),
        )
        .unwrap();
    let promise = Promise::pending_on(queue).unwrap();

    assert!(promise.resolve(Value::Object(thenable)).unwrap());
    assert_eq!(promise.state(), PromiseState::Rejected);
    assert_eq!(
        promise.await_result().unwrap_err().message(),
        "getter failed"
    );
}

#[test]
fn unhandled_hook_runs_at_checkpoint_and_is_suppressed_by_then() {
    let queue = JobQueue::new();
    let reported = Rc::new(RefCell::new(Vec::new()));
    let observed = reported.clone();
    queue.set_unhandled_rejection_hook(move |error| {
        observed.borrow_mut().push(error.message().to_owned());
    });
    let handled = Promise::rejected_on(queue.clone(), RuntimeError::type_error("handled")).unwrap();
    handled
        .then(
            None,
            Some(Object::native_function(|_, _| Ok(Value::Undefined))),
        )
        .unwrap();
    Promise::rejected_on(queue.clone(), RuntimeError::type_error("unhandled")).unwrap();

    assert!(reported.borrow().is_empty());
    queue.drain().unwrap();
    assert_eq!(&*reported.borrow(), &["unhandled"]);
}
