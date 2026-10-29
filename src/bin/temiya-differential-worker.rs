//! One isolated Script observation over the real public Engine/host APIs.
//! A parent adapter supplies process deadlines and captures infrastructure faults.
use serde_json::{Value as Json, json};
use std::{cell::RefCell, io::Read, rc::Rc};
use temiya::{
    Engine,
    embedding::{EmbedError, Exception, HostError, ScriptBackend},
    runtime::{RuntimeError, Value},
};

#[derive(Default)]
struct Trace {
    effects: Vec<Json>,
    jobs: Vec<Json>,
    job_errors: Vec<Json>,
    events: Vec<Json>,
    symbols: Vec<u64>,
    unsupported: bool,
}

impl Trace {
    fn value(&mut self, value: &Value) -> Json {
        match value {
            Value::Undefined => json!({"type":"undefined"}),
            Value::Null => json!({"type":"null"}),
            Value::Boolean(value) => json!({"type":"boolean","value":value}),
            Value::String(value) => json!({"type":"string","utf16":value.code_units()}),
            Value::Number(value) => json!({"type":"number","bits":if value.is_nan() {
                "NaN".into()
            } else {
                format!("{:016x}", value.to_bits())
            }}),
            Value::BigInt(value) => json!({"type":"bigint","decimal":value.to_string()}),
            Value::Symbol(value) => {
                let identity = value.identity();
                let id = self
                    .symbols
                    .iter()
                    .position(|item| *item == identity)
                    .unwrap_or_else(|| {
                        self.symbols.push(identity);
                        self.symbols.len() - 1
                    });
                json!({"type":"symbol","id":id,"description_utf16":value.description().map(|s|s.code_units())})
            }
            Value::Object(_) => {
                // Never silently equate opaque objects. Structured snapshots are
                // a separate protocol extension, not lossy ToString/JSON hooks.
                self.unsupported = true;
                json!({"type":"unsupported","kind":"object"})
            }
        }
    }

    fn runtime_error(&mut self, error: &RuntimeError) -> Json {
        json!({"type":format!("{:?}",error.kind()),
            "thrown":error.thrown_value().map(|value|self.value(value))})
    }

    fn exception(&mut self, error: &Exception) -> Json {
        json!({"type":format!("{:?}",error.kind()),
            "thrown":error.thrown().map(|value|self.value(value.value()))})
    }
}

fn run() -> Result<Json, Box<dyn std::error::Error>> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: Json = serde_json::from_str(&input)?;
    let source = request["source"]
        .as_str()
        .ok_or("source must be a string")?;
    let backend = match request["backend"].as_str().unwrap_or("auto") {
        "auto" => ScriptBackend::Auto,
        "ast" => ScriptBackend::Ast,
        "bytecode" => ScriptBackend::Bytecode,
        _ => return Err("invalid backend".into()),
    };
    let engine = Rc::new(Engine::builder().script_backend(backend).build());
    let realm = Rc::new(engine.create_realm());
    let trace = Rc::new(RefCell::new(Trace::default()));
    let effects = trace.clone();
    engine.register_global_function(&realm, "record", move |context, _, arguments| {
        let mut trace = effects.borrow_mut();
        let values: Vec<_> = arguments
            .iter()
            .map(|value| trace.value(value.value()))
            .collect();
        trace.effects.push(json!(values));
        trace.events.push(json!({"kind":"effect","values":values}));
        Ok(context.value(Value::Undefined))
    })?;
    let (weak_engine, weak_realm) = (Rc::downgrade(&engine), Rc::downgrade(&realm));
    let queue_trace = trace.clone();
    engine.register_global_function(&realm, "queueJob", move |context, _, arguments| {
        if arguments.len() != 2 {
            return Err(HostError::type_error(
                "queueJob requires a label and callback",
            ));
        }
        let Value::String(label) = arguments[0].value() else {
            return Err(HostError::type_error("job label must be a string"));
        };
        let Value::Object(callback) = arguments[1].value() else {
            return Err(HostError::type_error("job callback must be callable"));
        };
        if !callback.is_callable() {
            return Err(HostError::type_error("job callback must be callable"));
        }
        let label = label.code_units().to_vec();
        let callback = arguments[1].clone();
        let engine = weak_engine
            .upgrade()
            .ok_or_else(|| HostError::type_error("worker Engine ended"))?;
        let realm = weak_realm
            .upgrade()
            .ok_or_else(|| HostError::type_error("worker Realm ended"))?;
        let jobs = queue_trace.clone();
        engine
            .enqueue_job(&realm, move || {
                {
                    let mut trace = jobs.borrow_mut();
                    trace.jobs.push(json!(label));
                    trace.events.push(json!({"kind":"job","label_utf16":label}));
                }
                if let Value::Object(callback) = callback.value() {
                    if let Err(error) = callback.call(Value::Undefined, &[]) {
                        let mut trace = jobs.borrow_mut();
                        let error = trace.runtime_error(&error);
                        trace.job_errors.push(error.clone());
                        trace.events.push(json!({"kind":"job-error","error":error}));
                    }
                }
                Ok(())
            })
            .map_err(|error| HostError::type_error(error.to_string()))?;
        Ok(context.value(Value::Undefined))
    })?;
    let unhandled = trace.clone();
    engine.register_job_error_callback(&realm, move |error| {
        let mut trace = unhandled.borrow_mut();
        let error = trace.exception(&error);
        trace.job_errors.push(error.clone());
        trace
            .events
            .push(json!({"kind":"unhandled-rejection","error":error}));
    })?;
    let completion = match engine.compile(&realm, source) {
        Ok(script) => match engine.evaluate(&script) {
            Ok(value) => json!({"kind":"normal","value":trace.borrow_mut().value(value.value())}),
            Err(error) => json!({"kind":"throw","error":trace.borrow_mut().exception(&error)}),
        },
        Err(EmbedError::Compile(error)) => {
            json!({"kind":"compile-error","error":trace.borrow_mut().runtime_error(&error)})
        }
        Err(error) => return Err(Box::new(error)),
    };
    if let Err(error) = engine.drain_jobs(&realm) {
        let mut trace = trace.borrow_mut();
        let error = trace.exception(&error);
        trace.job_errors.push(error.clone());
        trace
            .events
            .push(json!({"kind":"checkpoint-error","error":error}));
    }
    let trace = trace.borrow();
    Ok(
        json!({"schema":1,"kind":"differential-observation","engine":"Temiya",
        "backend":format!("{backend:?}"),"comparison_eligible":!trace.unsupported,
        "completion":completion,"effects":trace.effects,"jobs":trace.jobs,
        "job_errors":trace.job_errors,"events":trace.events}),
    )
}

fn main() {
    match run() {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("differential worker infrastructure error: {error}");
            std::process::exit(2);
        }
    }
}
