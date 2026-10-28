//! File and inline-source shell over the public embedding API.
use std::{
    cell::RefCell,
    collections::HashMap,
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
};

use temiya::{
    Engine,
    embedding::{Exception, HostError, RealmHandle},
    runtime::Value,
};

const HELP: &str = "Usage: temiya [--script | --module] [-e SOURCE] [FILE ...]\n\
Execute UTF-8 JavaScript in input order. - reads stdin; no inputs reads stdin.\n\
  -e SOURCE      Add inline source (repeatable, interleaved with files)\n\
  --script       Script mode (default); inputs share one global environment\n\
  --module       Load inputs as one module graph; imports resolve from their file\n\
  --             Treat remaining arguments as filenames\n\
  -h, --help     Show help\n\
  --version      Show version\n\
print(...) and console.log(...) write stdout; console.error(...) writes stderr.\n\
Exit codes: 0 success, 1 source/I/O/runtime/job error, 2 command-line error.\n";
const ROOT: &str = "<modules>";
const BOOTSTRAP: &str = r#"
this.globalThis = this;
(function (out, err, string) {
    function line(values, write) {
        var text = '';
        for (var i = 0; i < values.length; i++) {
            if (i !== 0) text += ' ';
            text += string(values[i]);
        }
        write(text);
    }
    globalThis.print = function () { line(arguments, out); };
    globalThis.console = {
        log: function () { line(arguments, out); },
        error: function () { line(arguments, err); }
    };
})(__temiyaOut, __temiyaErr, String);
delete globalThis.__temiyaOut;
delete globalThis.__temiyaErr;
"#;

struct Failure {
    code: u8,
    message: String,
}
impl Failure {
    fn runtime(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: message.into(),
        }
    }
    fn usage(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
        }
    }
}

enum Input {
    Inline(String),
    File(PathBuf),
    Stdin,
}
impl Input {
    fn label(&self, index: usize) -> String {
        match self {
            Self::Inline(_) => format!("<eval:{}>", index + 1),
            Self::File(path) => path.display().to_string(),
            Self::Stdin => "<stdin>".into(),
        }
    }
    fn read(&self) -> Result<String, Failure> {
        match self {
            Self::Inline(source) => Ok(source.clone()),
            Self::File(path) => fs::read_to_string(path)
                .map_err(|error| Failure::runtime(format!("{}: {error}", path.display()))),
            Self::Stdin => {
                let mut source = String::new();
                io::stdin()
                    .read_to_string(&mut source)
                    .map_err(|error| Failure::runtime(format!("<stdin>: {error}")))?;
                Ok(source)
            }
        }
    }
}

fn diagnostic(error: &Exception) -> String {
    // Do not run user-defined object conversion while reporting an exception.
    match error.thrown().map(|value| value.value()) {
        Some(Value::Object(_)) | None => error.to_string(),
        Some(value) => match value.to_js_string() {
            Ok(text) => format!("{error}: {}", text.to_string_lossy()),
            Err(_) => error.to_string(),
        },
    }
}

fn install_output(engine: &Engine, realm: &RealmHandle) -> Result<(), Failure> {
    for (name, stderr) in [("__temiyaOut", false), ("__temiyaErr", true)] {
        engine
            .register_global_function(realm, name, move |context, _, arguments| {
                let Some(Value::String(text)) = arguments.first().map(|value| value.value()) else {
                    return Err(HostError::type_error("output requires a string"));
                };
                let text = text.to_string_lossy();
                let result = if stderr {
                    writeln!(io::stderr().lock(), "{text}")
                } else {
                    writeln!(io::stdout().lock(), "{text}")
                };
                result.map_err(|error| HostError::type_error(format!("output: {error}")))?;
                Ok(context.value(Value::Undefined))
            })
            .map_err(|error| Failure::runtime(error.to_string()))?;
    }
    let bootstrap = engine
        .compile(realm, BOOTSTRAP)
        .map_err(|error| Failure::runtime(error.to_string()))?;
    engine
        .evaluate(&bootstrap)
        .map_err(|error| Failure::runtime(diagnostic(&error)))?;
    Ok(())
}

fn canonical(path: &Path) -> Result<String, HostError> {
    let path = fs::canonicalize(path)
        .map_err(|error| HostError::reference_error(format!("{}: {error}", path.display())))?;
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| HostError::type_error("module filenames must be valid Unicode"))
}

fn modules(engine: &Engine, realm: &RealmHandle, inputs: &[Input]) -> Result<(), Failure> {
    let cwd = env::current_dir().map_err(|error| Failure::runtime(error.to_string()))?;
    let mut virtual_sources = HashMap::new();
    let mut entry = String::new();
    for (index, input) in inputs.iter().enumerate() {
        let name = match input {
            Input::File(path) => {
                canonical(path).map_err(|error| Failure::runtime(error.to_string()))?
            }
            _ => {
                let name = input.label(index);
                virtual_sources.insert(name.clone(), input.read()?);
                name
            }
        };
        entry.push_str("import ");
        entry.push_str(&serde_json::to_string(&name).expect("string serialization cannot fail"));
        entry.push_str(";\n");
    }
    virtual_sources.insert(ROOT.to_owned(), entry);
    let sources = Rc::new(virtual_sources);
    let identities = sources.clone();
    let module = engine
        .load_module_with_resolver(
            realm,
            ROOT,
            move |referrer, request| {
                if !request.attributes.is_empty() {
                    return Err(HostError::type_error(
                        "CLI modules do not support import attributes",
                    ));
                }
                let specifier = String::from_utf16(&request.specifier).map_err(|_| {
                    HostError::type_error("module specifier contains unpaired surrogates")
                })?;
                if identities.contains_key(&specifier) {
                    return Ok(specifier);
                }
                let path = Path::new(&specifier);
                if referrer.is_some()
                    && !path.is_absolute()
                    && !specifier.starts_with("./")
                    && !specifier.starts_with("../")
                {
                    return Err(HostError::reference_error(format!(
                        "bare module specifier is unsupported: {specifier}"
                    )));
                }
                let base = referrer
                    .filter(|name| !identities.contains_key(*name))
                    .and_then(|name| Path::new(name).parent())
                    .unwrap_or(&cwd);
                canonical(&base.join(path))
            },
            move |name| {
                if let Some(source) = sources.get(name) {
                    return Ok(source.clone());
                }
                fs::read_to_string(name)
                    .map_err(|error| HostError::reference_error(format!("{name}: {error}")))
            },
        )
        .map_err(|error| Failure::runtime(format!("<modules>: {error}")))?;
    engine
        .evaluate_module(&module)
        .map_err(|error| Failure::runtime(format!("<modules>: {}", diagnostic(&error))))
}

fn run() -> Result<(), Failure> {
    let mut args = env::args_os().skip(1);
    let mut module_mode = false;
    let mut inputs = Vec::new();
    let mut positional = false;
    while let Some(arg) = args.next() {
        if !positional {
            match arg.to_str() {
                Some("--") => {
                    positional = true;
                    continue;
                }
                Some("--script") => {
                    module_mode = false;
                    continue;
                }
                Some("--module") => {
                    module_mode = true;
                    continue;
                }
                Some("-e") => {
                    let source = args
                        .next()
                        .ok_or_else(|| Failure::usage("-e requires source text"))?
                        .into_string()
                        .map_err(|_| Failure::usage("-e source must be valid Unicode"))?;
                    inputs.push(Input::Inline(source));
                    continue;
                }
                Some("-h" | "--help") => {
                    return io::stdout()
                        .write_all(HELP.as_bytes())
                        .map_err(|error| Failure::runtime(error.to_string()));
                }
                Some("--version") => {
                    return writeln!(io::stdout(), "temiya {}", env!("CARGO_PKG_VERSION"))
                        .map_err(|error| Failure::runtime(error.to_string()));
                }
                Some(option) if option.starts_with('-') && option != "-" => {
                    return Err(Failure::usage(format!("unknown option: {option}")));
                }
                _ => {}
            }
        }
        if arg == "-" {
            if inputs.iter().any(|input| matches!(input, Input::Stdin)) {
                return Err(Failure::usage("stdin may appear only once"));
            }
            inputs.push(Input::Stdin);
        } else {
            inputs.push(Input::File(arg.into()));
        }
    }
    if inputs.is_empty() {
        inputs.push(Input::Stdin);
    }
    let engine = Engine::default();
    let realm = engine.create_realm();
    install_output(&engine, &realm)?;
    let errors = Rc::new(RefCell::new(Vec::new()));
    let reported = errors.clone();
    engine
        .register_job_error_callback(&realm, move |error| {
            reported.borrow_mut().push(diagnostic(&error))
        })
        .map_err(|error| Failure::runtime(error.to_string()))?;
    if module_mode {
        modules(&engine, &realm, &inputs)?;
    } else {
        for (index, input) in inputs.iter().enumerate() {
            let label = input.label(index);
            let script = engine
                .compile(&realm, &input.read()?)
                .map_err(|error| Failure::runtime(format!("{label}: {error}")))?;
            engine
                .evaluate(&script)
                .map_err(|error| Failure::runtime(format!("{label}: {}", diagnostic(&error))))?;
            engine
                .drain_jobs(&realm)
                .map_err(|error| Failure::runtime(format!("{label}: {}", diagnostic(&error))))?;
            if !errors.borrow().is_empty() {
                return Err(Failure::runtime(format!(
                    "{label}: {}",
                    errors.borrow().join("\n")
                )));
            }
        }
        return Ok(());
    }
    engine
        .drain_jobs(&realm)
        .map_err(|error| Failure::runtime(diagnostic(&error)))?;
    if !errors.borrow().is_empty() {
        return Err(Failure::runtime(errors.borrow().join("\n")));
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "temiya: {}", error.message);
            ExitCode::from(error.code)
        }
    }
}
