//! Engine-independent differential JavaScript testing primitives.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use regex::Regex;

/// Canonical externally observable result of evaluating a generated program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation {
    /// Successful evaluation output.
    Success(String),
    /// Failed evaluation output or error class.
    Error(String),
    /// The configured wall-clock deadline expired.
    Timeout,
}

/// A JavaScript engine boundary used by the differential runner.
pub trait DifferentialEngine {
    /// Stable engine name included in reports.
    fn name(&self) -> &str;
    /// Evaluates one isolated source with a deadline.
    fn evaluate(&mut self, source: &str, timeout: Duration) -> Observation;
}

/// A CLI engine adapter that communicates only through stdin/stdout/stderr.
pub struct CommandEngine {
    name: String,
    program: String,
    arguments: Vec<String>,
}

impl CommandEngine {
    /// Configures an external command without relying on engine-internal APIs.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        program: impl Into<String>,
        arguments: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            program: program.into(),
            arguments: arguments.into_iter().map(Into::into).collect(),
        }
    }
}

impl DifferentialEngine for CommandEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn evaluate(&mut self, source: &str, timeout: Duration) -> Observation {
        let mut child = match Command::new(&self.program)
            .args(&self.arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return Observation::Error(format!("spawn: {error}")),
        };
        if child
            .stdin
            .take()
            .and_then(|mut input| input.write_all(source.as_bytes()).err())
            .is_some()
        {
            let _ = child.kill();
            return Observation::Error("failed to write engine input".into());
        }
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(1)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Observation::Timeout;
                }
                Err(error) => {
                    let _ = child.kill();
                    return Observation::Error(format!("wait: {error}"));
                }
            }
        }
        match child.wait_with_output() {
            Ok(output) if output.status.success() => {
                Observation::Success(String::from_utf8_lossy(&output.stdout).into_owned())
            }
            Ok(output) => Observation::Error(String::from_utf8_lossy(&output.stderr).into_owned()),
            Err(error) => Observation::Error(format!("output: {error}")),
        }
    }
}

/// Ordered replacement rules for non-deterministic output fragments.
pub struct Normalizer {
    rules: Vec<(Regex, String)>,
}

impl Normalizer {
    /// Creates a normalizer with no implicit transformations.
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Adds a regex replacement applied after line-ending and trailing-space normalization.
    pub fn add_rule(
        &mut self,
        pattern: &str,
        replacement: impl Into<String>,
    ) -> Result<(), regex::Error> {
        self.rules.push((Regex::new(pattern)?, replacement.into()));
        Ok(())
    }

    /// Canonicalizes one observation deterministically.
    #[must_use]
    pub fn normalize(&self, observation: Observation) -> Observation {
        match observation {
            Observation::Success(value) => Observation::Success(self.text(&value)),
            Observation::Error(value) => Observation::Error(self.text(&value)),
            Observation::Timeout => Observation::Timeout,
        }
    }

    fn text(&self, value: &str) -> String {
        let mut value = value.replace("\r\n", "\n");
        value = value
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        for (pattern, replacement) in &self.rules {
            value = pattern.replace_all(&value, replacement).into_owned();
        }
        value
    }
}

impl Default for Normalizer {
    fn default() -> Self {
        Self::new()
    }
}

/// A reviewed mismatch pattern that may be reported without failing a run.
pub struct KnownDifference {
    /// Stable identifier for dashboards and manifests.
    pub id: String,
    source: Regex,
}

impl KnownDifference {
    /// Creates a known-difference rule matched against the generated source.
    pub fn new(id: impl Into<String>, source_pattern: &str) -> Result<Self, regex::Error> {
        Ok(Self {
            id: id.into(),
            source: Regex::new(source_pattern)?,
        })
    }
}

/// Per-engine observation captured for a differential run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineObservation {
    /// Engine adapter name.
    pub engine: String,
    /// Normalized observation.
    pub observation: Observation,
}

/// Classification of one differential input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DifferentialResult {
    /// Every engine produced the same normalized observation.
    Match(Vec<EngineObservation>),
    /// A reviewed known-difference rule matched.
    KnownDifference {
        /// Stable rule identifier.
        id: String,
        /// All normalized engine observations.
        observations: Vec<EngineObservation>,
    },
    /// Engines disagreed and no known rule matched.
    Mismatch(Vec<EngineObservation>),
}

/// Runs isolated sources across multiple engine adapters.
pub struct DifferentialRunner {
    engines: Vec<Box<dyn DifferentialEngine>>,
    normalizer: Normalizer,
    known: Vec<KnownDifference>,
    timeout: Duration,
}

impl DifferentialRunner {
    /// Creates a runner with an explicit per-engine timeout.
    #[must_use]
    pub fn new(timeout: Duration) -> Self {
        Self {
            engines: Vec::new(),
            normalizer: Normalizer::new(),
            known: Vec::new(),
            timeout,
        }
    }

    /// Adds an engine in deterministic execution/report order.
    pub fn add_engine(&mut self, engine: impl DifferentialEngine + 'static) {
        self.engines.push(Box::new(engine));
    }

    /// Replaces the output normalization policy.
    pub fn set_normalizer(&mut self, normalizer: Normalizer) {
        self.normalizer = normalizer;
    }

    /// Adds a reviewed known-difference rule.
    pub fn add_known_difference(&mut self, difference: KnownDifference) {
        self.known.push(difference);
    }

    /// Evaluates and compares one source across all configured engines.
    #[must_use]
    pub fn run(&mut self, source: &str) -> DifferentialResult {
        let observations = self
            .engines
            .iter_mut()
            .map(|engine| EngineObservation {
                engine: engine.name().to_owned(),
                observation: self
                    .normalizer
                    .normalize(engine.evaluate(source, self.timeout)),
            })
            .collect::<Vec<_>>();
        let matches = observations.first().is_none_or(|first| {
            observations
                .iter()
                .all(|item| item.observation == first.observation)
        });
        if matches {
            DifferentialResult::Match(observations)
        } else if let Some(rule) = self.known.iter().find(|rule| rule.source.is_match(source)) {
            DifferentialResult::KnownDifference {
                id: rule.id.clone(),
                observations,
            }
        } else {
            DifferentialResult::Mismatch(observations)
        }
    }
}

/// Deterministically removes source lines while a mismatch predicate remains true.
#[must_use]
pub fn minimize_source(source: &str, mut still_fails: impl FnMut(&str) -> bool) -> String {
    let mut lines = source.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    let mut index = 0;
    while index < lines.len() && lines.len() > 1 {
        let mut candidate = lines.clone();
        candidate.remove(index);
        let candidate_source = candidate.join("\n");
        if still_fails(&candidate_source) {
            lines = candidate;
        } else {
            index += 1;
        }
    }
    lines.join("\n")
}

/// Saves a minimized reproducer, creating its parent directory when necessary.
pub fn save_reproducer(path: &Path, source: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, source)
}

/// Deterministic generator restricted to side-effect-free arithmetic expressions.
pub struct SafeInputGenerator {
    state: u64,
}

impl SafeInputGenerator {
    /// Creates a generator whose seed completely determines its output sequence.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Produces one terminating program without host access or unbounded loops.
    pub fn generate(&mut self) -> String {
        let left = self.next() % 1_000;
        let right = self.next() % 1_000;
        let operators = ["+", "-", "*", "%"];
        let operator = operators[(self.next() % operators.len() as u64) as usize];
        format!("print(({left} {operator} {right}));")
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }
}
