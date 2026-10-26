//! Reproducible benchmark summaries and noise-aware regression classification.

use std::fmt;

/// Raw timing samples for one stable benchmark/input identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Measurement {
    /// Stable benchmark name.
    pub name: String,
    /// Stable input/corpus identifier.
    pub input: String,
    /// Independent wall-clock samples in nanoseconds.
    pub samples_ns: Vec<u64>,
}

/// Invalid benchmark data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BenchmarkError(pub String);

impl fmt::Display for BenchmarkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BenchmarkError {}

/// Robust summary using median and median absolute deviation (MAD).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    /// Median sample duration in nanoseconds.
    pub median_ns: u64,
    /// Median absolute deviation in nanoseconds.
    pub mad_ns: u64,
    /// MAD divided by median; zero for a zero median.
    pub noise_ratio: f64,
}

impl Measurement {
    /// Creates a measurement, rejecting absent samples and empty identifiers.
    pub fn new(
        name: impl Into<String>,
        input: impl Into<String>,
        samples_ns: Vec<u64>,
    ) -> Result<Self, BenchmarkError> {
        let measurement = Self {
            name: name.into(),
            input: input.into(),
            samples_ns,
        };
        if measurement.name.is_empty() || measurement.input.is_empty() {
            return Err(BenchmarkError(
                "benchmark name and input are required".into(),
            ));
        }
        if measurement.samples_ns.is_empty() {
            return Err(BenchmarkError(
                "at least one timing sample is required".into(),
            ));
        }
        Ok(measurement)
    }

    /// Computes a deterministic robust summary independent of sample ordering.
    #[must_use]
    pub fn summary(&self) -> Summary {
        let median_ns = median(self.samples_ns.clone());
        let deviations = self
            .samples_ns
            .iter()
            .map(|sample| sample.abs_diff(median_ns))
            .collect();
        let mad_ns = median(deviations);
        Summary {
            median_ns,
            mad_ns,
            noise_ratio: if median_ns == 0 {
                0.0
            } else {
                mad_ns as f64 / median_ns as f64
            },
        }
    }
}

/// Noise-aware policy for comparing equivalent benchmark measurements.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegressionPolicy {
    /// Minimum relative slowdown that may be classified as a regression.
    pub minimum_slowdown: f64,
    /// Multiplier applied to the combined baseline/current noise ratios.
    pub noise_multiplier: f64,
}

/// Result of comparing one current measurement with its baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comparison {
    /// Positive values indicate slower current measurements.
    pub relative_change: f64,
    /// Effective slowdown allowance after accounting for noise.
    pub allowed_slowdown: f64,
    /// Whether the slowdown exceeds the effective allowance.
    pub regression: bool,
}

impl RegressionPolicy {
    /// Compares matching benchmark/input pairs.
    pub fn compare(
        &self,
        baseline: &Measurement,
        current: &Measurement,
    ) -> Result<Comparison, BenchmarkError> {
        if baseline.name != current.name || baseline.input != current.input {
            return Err(BenchmarkError(
                "baseline and current measurement identifiers differ".into(),
            ));
        }
        let baseline = baseline.summary();
        let current = current.summary();
        let relative_change = if baseline.median_ns == 0 {
            0.0
        } else {
            current.median_ns as f64 / baseline.median_ns as f64 - 1.0
        };
        let allowed_slowdown = self
            .minimum_slowdown
            .max((baseline.noise_ratio + current.noise_ratio) * self.noise_multiplier);
        Ok(Comparison {
            relative_change,
            allowed_slowdown,
            regression: relative_change > allowed_slowdown,
        })
    }
}

fn median(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

/// Reproducible conformance counts for one corpus revision and selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConformanceBaseline {
    /// Corpus revision or immutable selection identifier.
    pub revision: String,
    /// Tests that matched expectations.
    pub passed: u64,
    /// Tests that unexpectedly differed.
    pub failed: u64,
    /// Tests excluded by the machine-readable manifest.
    pub skipped: u64,
}

impl ConformanceBaseline {
    /// Creates a snapshot, requiring an immutable non-empty revision identifier.
    pub fn new(
        revision: impl Into<String>,
        passed: u64,
        failed: u64,
        skipped: u64,
    ) -> Result<Self, BenchmarkError> {
        let revision = revision.into();
        if revision.is_empty() {
            return Err(BenchmarkError("conformance revision is required".into()));
        }
        Ok(Self {
            revision,
            passed,
            failed,
            skipped,
        })
    }

    /// Returns the number of classified tests.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.passed + self.failed + self.skipped
    }

    /// Computes signed pass/fail/skip changes for the same corpus revision.
    pub fn compare(&self, current: &Self) -> Result<ConformanceDelta, BenchmarkError> {
        if self.revision != current.revision {
            return Err(BenchmarkError(
                "conformance revisions differ; rebaseline explicitly".into(),
            ));
        }
        Ok(ConformanceDelta {
            passed: signed_delta(self.passed, current.passed),
            failed: signed_delta(self.failed, current.failed),
            skipped: signed_delta(self.skipped, current.skipped),
        })
    }
}

/// Signed conformance count changes from baseline to current results.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConformanceDelta {
    /// Change in passed tests.
    pub passed: i128,
    /// Change in failed tests.
    pub failed: i128,
    /// Change in skipped tests.
    pub skipped: i128,
}

fn signed_delta(baseline: u64, current: u64) -> i128 {
    i128::from(current) - i128::from(baseline)
}
