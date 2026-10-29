//! Per-execution async protocol state. Callbacks retain no Engine/Realm roots.
use std::{cell::RefCell, rc::Rc};
use temiya::{
    embedding::Exception,
    runtime::{RuntimeError, RuntimeResult, Value},
};

#[derive(Clone)]
pub struct Monitor(Rc<RefCell<State>>);
#[derive(Clone, Debug)]
pub struct Outcome {
    pub kind: &'static str,
    pub diagnostic: String,
}
struct State {
    enabled: bool,
    phase: Phase,
    calls: u32,
    failure: Option<String>,
    rejection: Option<String>,
    premature: bool,
}
#[derive(PartialEq)]
enum Phase {
    Setup,
    Running,
    Closed,
}
impl Monitor {
    pub fn new(enabled: bool) -> Self {
        Self(Rc::new(RefCell::new(State {
            enabled,
            phase: Phase::Setup,
            calls: 0,
            failure: None,
            rejection: None,
            premature: false,
        })))
    }
    pub fn start(&self) -> Result<(), &'static str> {
        let mut state = self.0.borrow_mut();
        if state.premature {
            return Err("async completion during harness setup");
        }
        state.phase = Phase::Running;
        Ok(())
    }
    pub fn finish(&self) -> Outcome {
        let mut state = self.0.borrow_mut();
        state.phase = Phase::Closed;
        let (kind, diagnostic) = if state.calls > 1 {
            (
                "duplicate-completion",
                "multiple async terminal notifications".into(),
            )
        } else if let Some(error) = &state.failure {
            ("failure", error.clone())
        } else if let Some(error) = &state.rejection {
            ("rejected", error.clone())
        } else if state.calls == 1 {
            ("complete", String::new())
        } else {
            (
                "incomplete",
                "no async completion notification after the microtask checkpoint".into(),
            )
        };
        Outcome { kind, diagnostic }
    }
    pub fn print_callback(&self) -> impl Fn(Value, &[Value]) -> RuntimeResult<Value> + 'static {
        let weak = Rc::downgrade(&self.0);
        move |_, args| {
            let state = weak
                .upgrade()
                .ok_or_else(|| RuntimeError::internal_error("Test262 monitor ended"))?;
            // User ToPrimitive may reenter print. Never retain a state borrow.
            let message = args
                .first()
                .unwrap_or(&Value::Undefined)
                .to_js_string()?
                .to_string_lossy();
            let mut state = state.borrow_mut();
            if state.enabled && state.phase != Phase::Closed {
                let success = message == "Test262:AsyncTestComplete";
                let failure = message.starts_with("Test262:AsyncTestFailure:");
                if success || failure {
                    if state.phase == Phase::Setup {
                        state.premature = true;
                    }
                    state.calls = state.calls.saturating_add(1);
                    if failure && state.failure.is_none() {
                        state.failure = Some(message.chars().take(2048).collect());
                    }
                }
            }
            Ok(Value::Undefined)
        }
    }
    pub fn rejection_callback(&self) -> impl Fn(Exception) + 'static {
        let weak = Rc::downgrade(&self.0);
        move |error| {
            if let Some(state) = weak.upgrade() {
                let mut state = state.borrow_mut();
                if state.enabled && state.phase != Phase::Closed && state.rejection.is_none() {
                    state.rejection = Some(format!("unhandled rejection: {error}"));
                }
            }
        }
    }
}
