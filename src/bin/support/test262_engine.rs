//! Engine-facing synchronous runner. Harness failures are never language errors.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    rc::Rc,
};

use temiya::{
    Engine,
    embedding::{EmbedError, Exception, ExceptionKind, HostError, ScriptBackend},
    parser::parse_module,
    runtime::{ModuleStatus, Object, Value},
};

use temiya::test262::{
    Executor, HarnessError, Manifest, NegativePhase, ObservedFailure, ResultStatus,
    TEST262_REVISION, TestCase, TestResult, TestVariant, run_test,
};

/// Source of tests, fixtures and unchanged harness files.
///
/// Directory mode makes no revision claim. Pinned mode reads every file with
/// `git show` at [`TEST262_REVISION`], independently of checkout modifications.
#[derive(Clone, Debug)]
pub struct Corpus {
    root: PathBuf,
    pinned: bool,
}

impl Corpus {
    /// Reads a local fixture directory without claiming an upstream revision.
    pub fn directory(root: impl AsRef<Path>) -> Result<Self, HarnessError> {
        let root = fs::canonicalize(root).map_err(|e| HarnessError(e.to_string()))?;
        if !root.is_dir() {
            return Err(HarnessError("corpus root must be a directory".into()));
        }
        Ok(Self {
            root,
            pinned: false,
        })
    }

    /// Reads the fixed Test262 Git object; working-tree files are not consumed.
    pub fn pinned(root: impl AsRef<Path>) -> Result<Self, HarnessError> {
        let mut corpus = Self::directory(root)?;
        let output = Command::new("git")
            .arg("-c")
            .arg(format!("safe.directory={}", corpus.root.display()))
            .arg("-C")
            .arg(&corpus.root)
            .args(["cat-file", "-t", TEST262_REVISION])
            .output()
            .map_err(|e| HarnessError(format!("git: {e}")))?;
        if !output.status.success() || output.stdout != b"commit\n" {
            return Err(HarnessError(
                "pinned Test262 commit is unavailable; run scripts/test262".into(),
            ));
        }
        corpus.pinned = true;
        Ok(corpus)
    }

    /// Exact consumed upstream revision, or `None` for local directory mode.
    #[must_use]
    pub fn revision(&self) -> Option<&'static str> {
        self.pinned.then_some(TEST262_REVISION)
    }

    /// Reads a corpus-relative UTF-8 file, rejecting escapes from the root.
    pub fn read(&self, path: &str) -> Result<String, HarnessError> {
        checked_relative(path)?;
        if self.pinned {
            let output = Command::new("git")
                .arg("-c")
                .arg(format!("safe.directory={}", self.root.display()))
                .arg("-C")
                .arg(&self.root)
                .args(["show", &format!("{TEST262_REVISION}:{path}")])
                .output()
                .map_err(|e| HarnessError(format!("git: {e}")))?;
            if !output.status.success() {
                return Err(HarnessError(format!(
                    "cannot read pinned corpus file {path}"
                )));
            }
            return String::from_utf8(output.stdout).map_err(|e| HarnessError(e.to_string()));
        }
        let actual = fs::canonicalize(self.root.join(path))
            .map_err(|e| HarnessError(format!("{path}: {e}")))?;
        if !actual.starts_with(&self.root) || !actual.is_file() {
            return Err(HarnessError(format!("not a corpus file: {path}")));
        }
        fs::read_to_string(actual).map_err(|e| HarnessError(format!("{path}: {e}")))
    }
}

fn checked_relative(path: &str) -> Result<(), HarnessError> {
    if path.is_empty()
        || path.contains('\\')
        || !Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(HarnessError(format!(
            "expected a normalized corpus-relative path: {path}"
        )));
    }
    Ok(())
}

fn resolve_module(referrer: Option<&str>, specifier: &str) -> Result<String, HostError> {
    if referrer.is_none() {
        checked_relative(specifier).map_err(|e| HostError::type_error(e.to_string()))?;
        return Ok(specifier.into());
    }
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return Err(HostError::type_error(
            "only corpus-relative module fixtures are supported",
        ));
    }
    let base = Path::new(referrer.unwrap())
        .parent()
        .unwrap_or(Path::new(""));
    let mut normalized = PathBuf::new();
    for component in base.join(specifier).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir if normalized.pop() => {}
            _ => return Err(HostError::type_error("module fixture escapes corpus root")),
        }
    }
    let path = normalized
        .to_str()
        .ok_or_else(|| HostError::type_error("non-UTF-8 module path"))?;
    checked_relative(path).map_err(|e| HostError::type_error(e.to_string()))?;
    Ok(path.into())
}

/// Concrete [`Executor`] over fresh public Engine/Realm instances per variant.
///
/// Use [`run_with_engine`] to apply manifest policy and metadata capability gates.
pub struct EngineExecutor {
    corpus: Corpus,
    path: String,
    backend: ScriptBackend,
    raw: bool,
    diagnostic: Option<String>,
    harness_cache: BTreeMap<String, String>,
}

impl EngineExecutor {
    /// Selects a corpus, test identity, explicit Script backend and raw policy.
    pub fn new(corpus: Corpus, path: impl Into<String>, backend: ScriptBackend, raw: bool) -> Self {
        Self {
            corpus,
            path: path.into(),
            backend,
            raw,
            diagnostic: None,
            harness_cache: BTreeMap::new(),
        }
    }

    /// Original engine or harness diagnostic from the most recent execution.
    #[must_use]
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }

    fn failure(
        &mut self,
        phase: NegativePhase,
        error_type: &str,
        message: impl ToString,
    ) -> ObservedFailure {
        self.diagnostic = Some(message.to_string());
        ObservedFailure {
            phase,
            error_type: error_type.into(),
        }
    }

    fn compile_failure(&mut self, error: EmbedError) -> ObservedFailure {
        let kind = match &error {
            EmbedError::Compile(error)
                if error.kind() == temiya::runtime::RuntimeErrorKind::SyntaxError =>
            {
                "SyntaxError"
            }
            EmbedError::Bytecode(_) => "UnsupportedBytecode",
            _ => "EngineError",
        };
        self.failure(NegativePhase::Parse, kind, error)
    }
}

fn exception_type(error: &Exception, constructors: &[(String, Object)]) -> String {
    let missing_hook = |message: &str| {
        ["$262", "$DONE", "print"]
            .iter()
            .any(|name| message == format!("{name} is not defined"))
    };
    if missing_hook(error.message()) {
        return "HarnessError".into();
    }
    match error.kind() {
        ExceptionKind::ThrownValue => {
            // Match constructor identity, never a forgeable .name string. A
            // getter failure while inspecting the original thrown value must
            // not replace it with a matching language error.
            if let Some(Value::Object(object)) = error.thrown().map(|v| v.value()) {
                if let Ok(Value::String(message)) = object.get(&"message".into()) {
                    if missing_hook(&message.to_string_lossy()) {
                        return "HarnessError".into();
                    }
                }
                if let Ok(Value::Object(constructor)) = object.get(&"constructor".into()) {
                    if let Some((name, _)) =
                        constructors.iter().find(|(_, known)| *known == constructor)
                    {
                        return name.clone();
                    }
                }
            }
            "ThrownValue".into()
        }
        ExceptionKind::Engine => "EngineError".into(),
        kind => format!("{kind:?}"),
    }
}

fn exception_diagnostic(error: &Exception) -> String {
    let mut diagnostic = error.to_string();
    if let Some(Value::Object(object)) = error.thrown().map(|value| value.value()) {
        // Read data descriptors only; do not call a message getter or ToString.
        if let Ok(Some(descriptor)) = object.try_get_own_property(&"message".into()) {
            if let Some(Value::String(message)) = descriptor.value {
                diagnostic.push_str(": ");
                diagnostic.push_str(&message.to_string_lossy());
            }
        }
    }
    diagnostic
}

impl Executor for EngineExecutor {
    fn execute(
        &mut self,
        source: &str,
        variant: TestVariant,
        includes: &[String],
    ) -> Result<(), ObservedFailure> {
        self.diagnostic = None;
        let engine = Engine::builder().script_backend(self.backend).build();
        let realm = engine.create_realm();
        let mut constructors = Vec::new();
        for name in [
            "Error",
            "TypeError",
            "RangeError",
            "ReferenceError",
            "SyntaxError",
        ] {
            let script = engine
                .compile(&realm, name)
                .map_err(|e| self.failure(NegativePhase::Runtime, "HarnessError", e))?;
            let value = engine
                .evaluate(&script)
                .map_err(|e| self.failure(NegativePhase::Runtime, "HarnessError", e))?;
            if let Value::Object(object) = value.value() {
                constructors.push((name.into(), object.clone()));
            }
        }
        let mut script = None;
        let mut module = None;
        if variant == TestVariant::Module {
            // Record source-read/parser errors independently of load/link errors.
            let failure = Rc::new(RefCell::new(None::<(NegativePhase, String, String)>));
            let root = self.path.clone();
            let root_source = source.to_owned();
            let corpus = self.corpus.clone();
            let captured = failure.clone();
            let module_result = engine.load_module_with_resolver(
                &realm,
                &self.path,
                move |referrer, request| {
                    if !request.attributes.is_empty() {
                        return Err(HostError::type_error(
                            "module attributes are not configured",
                        ));
                    }
                    resolve_module(referrer, &String::from_utf16_lossy(&request.specifier))
                },
                move |path| {
                    let text = if path == root {
                        root_source.clone()
                    } else {
                        match corpus.read(path) {
                            Ok(text) => text,
                            Err(error) => {
                                *captured.borrow_mut() = Some((
                                    NegativePhase::Resolution,
                                    "HarnessError".into(),
                                    error.to_string(),
                                ));
                                return Err(HostError::type_error(error.to_string()));
                            }
                        }
                    };
                    if let Err(error) = parse_module(&text) {
                        *captured.borrow_mut() = Some((
                            if path == root {
                                NegativePhase::Parse
                            } else {
                                NegativePhase::Resolution
                            },
                            "SyntaxError".into(),
                            error.to_string(),
                        ));
                        return Err(HostError::type_error(error.to_string()));
                    }
                    Ok(text)
                },
            );
            let loaded = match module_result {
                Ok(module) => module,
                Err(error) => {
                    if let Some((phase, kind, diagnostic)) = failure.borrow_mut().take() {
                        return Err(self.failure(phase, &kind, diagnostic));
                    }
                    return Err(self.failure(NegativePhase::Resolution, "HarnessError", error));
                }
            };
            loaded.record().link().map_err(|error| {
                let kind = match error.kind() {
                    temiya::runtime::RuntimeErrorKind::InternalError => "EngineError".into(),
                    temiya::runtime::RuntimeErrorKind::ThrownValue => "ThrownValue".into(),
                    kind => format!("{kind:?}"),
                };
                self.failure(NegativePhase::Resolution, &kind, error)
            })?;
            module = Some(loaded);
        } else {
            script = Some(
                engine
                    .compile(&realm, source)
                    .map_err(|e| self.compile_failure(e))?,
            );
        }
        if !self.raw {
            for name in ["assert.js", "sta.js"]
                .into_iter()
                .map(str::to_owned)
                .chain(includes.iter().cloned())
            {
                checked_relative(&name)
                    .map_err(|e| self.failure(NegativePhase::Runtime, "HarnessError", e))?;
                let path = format!("harness/{name}");
                let text = if let Some(text) = self.harness_cache.get(&path) {
                    text.clone()
                } else {
                    let text = self
                        .corpus
                        .read(&path)
                        .map_err(|e| self.failure(NegativePhase::Runtime, "HarnessError", e))?;
                    self.harness_cache.insert(path.clone(), text.clone());
                    text
                };
                let harness = engine.compile(&realm, &text).map_err(|e| {
                    self.failure(
                        NegativePhase::Runtime,
                        "HarnessError",
                        format!("{path}: {e}"),
                    )
                })?;
                engine.evaluate(&harness).map_err(|e| {
                    self.failure(
                        NegativePhase::Runtime,
                        "HarnessError",
                        format!("{path}: {}", exception_diagnostic(&e)),
                    )
                })?;
            }
            let identity = engine
                .compile(
                    &realm,
                    "typeof Test262Error === 'function' ? Test262Error : null;",
                )
                .map_err(|e| self.failure(NegativePhase::Runtime, "HarnessError", e))?;
            let identity = engine.evaluate(&identity).map_err(|e| {
                self.failure(
                    NegativePhase::Runtime,
                    "HarnessError",
                    exception_diagnostic(&e),
                )
            })?;
            if let Value::Object(object) = identity.value() {
                constructors.push(("Test262Error".into(), object.clone()));
            }
        }
        let result = if let Some(script) = script {
            engine.evaluate(&script).map(|_| ())
        } else {
            let module = module.as_ref().expect("prepared module");
            let result = engine.evaluate_module(module);
            if result.is_ok() && module.record().status() != ModuleStatus::Evaluated {
                return Err(self.failure(
                    NegativePhase::Runtime,
                    "PendingModule",
                    "module evaluation did not complete",
                ));
            }
            result
        };
        result.map_err(|error| {
            self.failure(
                NegativePhase::Runtime,
                &exception_type(&error, &constructors),
                exception_diagnostic(&error),
            )
        })?;
        engine.drain_jobs(&realm).map_err(|error| {
            self.failure(
                NegativePhase::Runtime,
                &exception_type(&error, &constructors),
                exception_diagnostic(&error),
            )
        })?;
        Ok(())
    }
}

/// One classified Engine execution and its original diagnostic, if any.
#[derive(Clone, Debug)]
pub struct EngineTestResult {
    /// Existing path/variant/status contract.
    pub result: TestResult,
    /// Original engine or harness diagnostic; never used as an error type.
    pub diagnostic: Option<String>,
    /// Original observed phase/type, including a matching negative expectation.
    pub observed_failure: Option<ObservedFailure>,
}

/// Runs every required variant through a fresh Engine, retaining skips and failures.
///
/// Async/agent completion is deliberately unqualified until #148/#150. Manifest
/// skips never execute. Includes are original corpus files, not replacement stubs.
pub fn run_with_engine(
    test: &TestCase,
    manifest: &Manifest,
    corpus: &Corpus,
    backend: ScriptBackend,
) -> Vec<EngineTestResult> {
    let unsupported = test.metadata.flags.iter().find(|flag| {
        !matches!(
            flag.as_str(),
            "raw" | "module" | "onlyStrict" | "noStrict" | "generated" | "non-deterministic"
        )
    });
    let has = |flag: &str| test.metadata.flags.iter().any(|value| value == flag);
    let conflicting_flags = (has("onlyStrict") && (has("noStrict") || has("raw")))
        || (has("module") && (has("noStrict") || has("raw")));
    let gated = if let Some(skip) = manifest.skip(&test.path) {
        Some(ResultStatus::Skipped {
            reason: skip.reason.clone(),
            detail: skip.detail.clone(),
        })
    } else if let Some(flag) = unsupported {
        Some(ResultStatus::Skipped {
            reason: "unsupported-runner-flag".into(),
            detail: format!("runner does not support flag {flag}"),
        })
    } else if conflicting_flags {
        Some(ResultStatus::Failed("conflicting variant flags".into()))
    } else {
        None
    };
    if let Some(status) = gated {
        return test
            .variants()
            .into_iter()
            .map(|variant| EngineTestResult {
                result: TestResult {
                    path: test.path.clone(),
                    variant,
                    status: status.clone(),
                },
                diagnostic: None,
                observed_failure: None,
            })
            .collect();
    }
    type Observation = (Option<String>, Option<ObservedFailure>);
    struct RecordingExecutor {
        runner: EngineExecutor,
        observations: BTreeMap<TestVariant, Observation>,
    }
    impl Executor for RecordingExecutor {
        fn execute(
            &mut self,
            source: &str,
            variant: TestVariant,
            includes: &[String],
        ) -> Result<(), ObservedFailure> {
            let observed = self.runner.execute(source, variant, includes);
            self.observations.insert(
                variant,
                (
                    self.runner.diagnostic().map(str::to_owned),
                    observed.as_ref().err().cloned(),
                ),
            );
            observed
        }
    }
    let mut executor = RecordingExecutor {
        runner: EngineExecutor::new(corpus.clone(), &test.path, backend, has("raw")),
        observations: BTreeMap::new(),
    };
    // Classification stays in the core's existing public harness API; the
    // concrete Engine/filesystem adapter is compiled only into its CLI client.
    run_test(test, manifest, &mut executor)
        .into_iter()
        .map(|mut result| {
            let Some((diagnostic, observed_failure)) =
                executor.observations.remove(&result.variant)
            else {
                result.status = ResultStatus::Failed("missing Engine execution observation".into());
                return EngineTestResult {
                    result,
                    diagnostic: None,
                    observed_failure: None,
                };
            };
            if let Some(error) = &observed_failure {
                if matches!(
                    error.error_type.as_str(),
                    "HarnessError" | "EngineError" | "UnsupportedBytecode" | "PendingModule"
                ) {
                    result.status = ResultStatus::Failed(format!(
                        "runner/engine failure: {}",
                        error.error_type
                    ));
                }
            }
            EngineTestResult {
                result,
                diagnostic,
                observed_failure,
            }
        })
        .collect()
}

/// Parses original source, extending the foundation's metadata lists in the CLI.
/// The JavaScript source bytes are retained without normalization.
pub fn parse_case(
    path: impl Into<String>,
    source: impl Into<String>,
) -> Result<TestCase, HarnessError> {
    let mut case = TestCase::parse(path, source)?;
    let start = case.source.find("/*---").expect("validated frontmatter") + 5;
    let end = start
        + case.source[start..]
            .find("---*/")
            .expect("validated frontmatter");
    for line in case.source[start..end].lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("includes:") {
            case.metadata.includes = parse_flow_list(value)?;
        }
        if let Some(value) = line.strip_prefix("features:") {
            case.metadata.features = parse_flow_list(value)?;
        }
        if let Some(value) = line.strip_prefix("flags:") {
            case.metadata.flags = parse_flow_list(value)?;
        }
    }
    Ok(case)
}

fn parse_flow_list(source: &str) -> Result<Vec<String>, HarnessError> {
    let source = source.trim();
    let content = source
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| HarnessError("metadata lists must use inline [item, ...] syntax".into()))?;
    fn item(source: &str) -> Result<String, HarnessError> {
        let source = source.trim();
        if source.is_empty() {
            return Err(HarnessError("empty metadata list item".into()));
        }
        if source.starts_with('"') {
            return serde_json::from_str(source)
                .map_err(|e| HarnessError(format!("invalid double-quoted metadata item: {e}")));
        }
        if let Some(inner) = source.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
            if inner.replace("''", "").contains('\'') {
                return Err(HarnessError("invalid single-quoted metadata item".into()));
            }
            return Ok(inner.replace("''", "'"));
        }
        if source.starts_with('\'') {
            return Err(HarnessError("invalid single-quoted metadata item".into()));
        }
        Ok(source.to_owned())
    }
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut chars = content.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if ch == '\\' => escaped = true,
            Some('\'') if ch == '\'' && chars.peek().is_some_and(|(_, next)| *next == '\'') => {
                chars.next();
            }
            Some(delimiter) if ch == delimiter => quote = None,
            Some(_) => {}
            None if matches!(ch, '\'' | '"') && content[start..index].trim().is_empty() => {
                quote = Some(ch)
            }
            None if ch == ',' => {
                values.push(item(&content[start..index])?);
                start = index + 1;
            }
            None => {}
        }
    }
    if quote.is_some() {
        return Err(HarnessError("unterminated quoted metadata item".into()));
    }
    if !content[start..].trim().is_empty() {
        values.push(item(&content[start..])?);
    }
    Ok(values)
}
