//! Reproducible Test262 metadata, variant planning, and result classification.

use std::{collections::BTreeMap, fmt};

mod engine;
pub use engine::{Corpus, EngineExecutor, EngineTestResult, run_with_engine};

/// Test262 upstream revision consumed by the repository harness.
pub const TEST262_REVISION: &str = "d53ebc8b1931d9f1541e43d59650776e4b766248";

/// One source variant required by Test262 flags.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TestVariant {
    /// Non-strict Script source.
    Sloppy,
    /// Script source prefixed with a strict directive.
    Strict,
    /// Module source, which is inherently strict.
    Module,
}

/// Phase in which a negative test expects an error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NegativePhase {
    /// Parsing must fail.
    Parse,
    /// Module resolution/linking must fail.
    Resolution,
    /// Evaluation must fail.
    Runtime,
}

/// Expected failure declared by Test262 frontmatter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NegativeExpectation {
    /// Expected execution phase.
    pub phase: NegativePhase,
    /// ECMAScript error constructor name.
    pub error_type: String,
}

/// Parsed subset of Test262 YAML frontmatter used by this harness.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Metadata {
    /// Harness files requested by `includes`.
    pub includes: Vec<String>,
    /// Feature names declared by the test.
    pub features: Vec<String>,
    /// Test262 flags such as `onlyStrict`, `noStrict`, `module`, and `raw`.
    pub flags: Vec<String>,
    /// Optional expected failure.
    pub negative: Option<NegativeExpectation>,
}

/// One Test262 test and its source body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestCase {
    /// Repository-relative path used by manifests and reports.
    pub path: String,
    /// Parsed frontmatter.
    pub metadata: Metadata,
    /// Source including its original frontmatter comment.
    pub source: String,
}

/// A metadata or manifest validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessError(pub String);

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for HarnessError {}

impl TestCase {
    /// Parses a test and its delimited `/*--- ... ---*/` frontmatter.
    pub fn parse(path: impl Into<String>, source: impl Into<String>) -> Result<Self, HarnessError> {
        let source = source.into();
        let start = source
            .find("/*---")
            .ok_or_else(|| HarnessError("missing Test262 frontmatter start".into()))?;
        let body_start = start + "/*---".len();
        let relative_end = source[body_start..]
            .find("---*/")
            .ok_or_else(|| HarnessError("missing Test262 frontmatter end".into()))?;
        let metadata = parse_metadata(&source[body_start..body_start + relative_end])?;
        Ok(Self {
            path: path.into(),
            metadata,
            source,
        })
    }

    /// Returns execution variants in deterministic order.
    #[must_use]
    pub fn variants(&self) -> Vec<TestVariant> {
        if self.metadata.flags.iter().any(|flag| flag == "module") {
            return vec![TestVariant::Module];
        }
        if self.metadata.flags.iter().any(|flag| flag == "raw") {
            return vec![TestVariant::Sloppy];
        }
        if self.metadata.flags.iter().any(|flag| flag == "onlyStrict") {
            return vec![TestVariant::Strict];
        }
        if self.metadata.flags.iter().any(|flag| flag == "noStrict") {
            return vec![TestVariant::Sloppy];
        }
        vec![TestVariant::Sloppy, TestVariant::Strict]
    }

    /// Produces source for a variant, including the strict directive when needed.
    #[must_use]
    pub fn source_for(&self, variant: TestVariant) -> String {
        match variant {
            TestVariant::Strict => format!("\"use strict\";\n{}", self.source),
            TestVariant::Sloppy | TestVariant::Module => self.source.clone(),
        }
    }
}

/// A machine-readable reason for excluding a known unsupported test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkipEntry {
    /// Exact repository-relative test path.
    pub path: String,
    /// Stable reason identifier suitable for aggregation.
    pub reason: String,
    /// Human-readable explanation.
    pub detail: String,
}

/// Pinned Test262 revision and deterministic skip policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    /// Exact upstream Git commit.
    pub revision: String,
    /// Skip entries indexed by test path.
    pub skips: BTreeMap<String, SkipEntry>,
}

impl Manifest {
    /// Parses and validates the repository's JSON manifest.
    pub fn from_json(source: &str) -> Result<Self, HarnessError> {
        let value: serde_json::Value =
            serde_json::from_str(source).map_err(|error| HarnessError(error.to_string()))?;
        let revision = string_field(&value, "revision")?.to_owned();
        if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(HarnessError(
                "manifest revision must be a 40-digit Git SHA".into(),
            ));
        }
        let entries = value
            .get("skips")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| HarnessError("manifest skips must be an array".into()))?;
        let mut skips = BTreeMap::new();
        for entry in entries {
            let skip = SkipEntry {
                path: string_field(entry, "path")?.to_owned(),
                reason: string_field(entry, "reason")?.to_owned(),
                detail: string_field(entry, "detail")?.to_owned(),
            };
            if skip.reason.is_empty() || skip.detail.is_empty() {
                return Err(HarnessError(
                    "skip reason and detail cannot be empty".into(),
                ));
            }
            if skips.insert(skip.path.clone(), skip).is_some() {
                return Err(HarnessError("duplicate skip path".into()));
            }
        }
        Ok(Self { revision, skips })
    }

    /// Returns the skip policy for an exact test path.
    #[must_use]
    pub fn skip(&self, path: &str) -> Option<&SkipEntry> {
        self.skips.get(path)
    }
}

/// Failure observed by an engine adapter while running one variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedFailure {
    /// Phase that produced the failure.
    pub phase: NegativePhase,
    /// Error constructor name reported by the adapter.
    pub error_type: String,
}

/// Adapter implemented by an engine-facing Test262 runner.
pub trait Executor {
    /// Loads requested harness includes, parses, links when needed, and evaluates source.
    fn execute(
        &mut self,
        source: &str,
        variant: TestVariant,
        includes: &[String],
    ) -> Result<(), ObservedFailure>;
}

/// Stable status of one path/variant result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResultStatus {
    /// Execution matched its positive or negative expectation.
    Passed,
    /// Execution differed from the declared expectation.
    Failed(String),
    /// Manifest policy excluded this path.
    Skipped { reason: String, detail: String },
}

/// One deterministic harness result record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestResult {
    /// Test repository path.
    pub path: String,
    /// Executed source variant.
    pub variant: TestVariant,
    /// Classified status.
    pub status: ResultStatus,
}

/// Runs all required variants and classifies positive and negative expectations.
pub fn run_test(
    test: &TestCase,
    manifest: &Manifest,
    executor: &mut impl Executor,
) -> Vec<TestResult> {
    test.variants()
        .into_iter()
        .map(|variant| {
            let status = if let Some(skip) = manifest.skip(&test.path) {
                ResultStatus::Skipped {
                    reason: skip.reason.clone(),
                    detail: skip.detail.clone(),
                }
            } else {
                classify(
                    test.metadata.negative.as_ref(),
                    executor.execute(&test.source_for(variant), variant, &test.metadata.includes),
                )
            };
            TestResult {
                path: test.path.clone(),
                variant,
                status,
            }
        })
        .collect()
}

fn classify(
    expected: Option<&NegativeExpectation>,
    observed: Result<(), ObservedFailure>,
) -> ResultStatus {
    match (expected, observed) {
        (None, Ok(())) => ResultStatus::Passed,
        (None, Err(error)) => {
            ResultStatus::Failed(format!("unexpected {:?} {}", error.phase, error.error_type))
        }
        (Some(expected), Err(observed))
            if expected.phase == observed.phase && expected.error_type == observed.error_type =>
        {
            ResultStatus::Passed
        }
        (Some(expected), Ok(())) => ResultStatus::Failed(format!(
            "expected {:?} {} but execution succeeded",
            expected.phase, expected.error_type
        )),
        (Some(expected), Err(observed)) => ResultStatus::Failed(format!(
            "expected {:?} {}, observed {:?} {}",
            expected.phase, expected.error_type, observed.phase, observed.error_type
        )),
    }
}

fn parse_metadata(source: &str) -> Result<Metadata, HarnessError> {
    let mut metadata = Metadata::default();
    let mut negative = false;
    let mut phase = None;
    let mut error_type = None;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "negative:" {
            negative = true;
            continue;
        }
        if negative && raw.starts_with(char::is_whitespace) {
            if let Some(value) = line.strip_prefix("phase:") {
                phase = Some(match value.trim() {
                    "parse" => NegativePhase::Parse,
                    "resolution" => NegativePhase::Resolution,
                    "runtime" => NegativePhase::Runtime,
                    value => return Err(HarnessError(format!("unknown negative phase: {value}"))),
                });
            } else if let Some(value) = line.strip_prefix("type:") {
                error_type = Some(value.trim().to_owned());
            }
            continue;
        }
        negative = false;
        if let Some(value) = line.strip_prefix("includes:") {
            metadata.includes = parse_inline_list(value)?;
        } else if let Some(value) = line.strip_prefix("features:") {
            metadata.features = parse_inline_list(value)?;
        } else if let Some(value) = line.strip_prefix("flags:") {
            metadata.flags = parse_inline_list(value)?;
        }
    }
    if phase.is_some() || error_type.is_some() {
        metadata.negative = Some(NegativeExpectation {
            phase: phase.ok_or_else(|| HarnessError("negative phase is required".into()))?,
            error_type: error_type
                .filter(|value| !value.is_empty())
                .ok_or_else(|| HarnessError("negative type is required".into()))?,
        });
    }
    Ok(metadata)
}

fn parse_inline_list(source: &str) -> Result<Vec<String>, HarnessError> {
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

fn string_field<'a>(value: &'a serde_json::Value, name: &str) -> Result<&'a str, HarnessError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| HarnessError(format!("manifest {name} must be a string")))
}
