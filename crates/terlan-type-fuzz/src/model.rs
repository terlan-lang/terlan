use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const CASE_SCHEMA: &str = "terlan.type-fuzz-case.v1";
pub const VALUE_SCHEMA: &str = "terlan.typed-value.v1";

/// Optional semantic oracle applied after a generated case type-checks.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Oracle {
    Check,
    JavaScript,
    Native,
    None,
}

impl Oracle {
    /// Parses the stable command-line spelling of an oracle.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "check" => Ok(Self::Check),
            "js" => Ok(Self::JavaScript),
            "native" => Ok(Self::Native),
            "none" => Ok(Self::None),
            _ => Err("--oracle must be `check`, `js`, `native`, or `none`".into()),
        }
    }
}

/// Bounded resources and generation controls for one fuzzing run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunOptions {
    pub seed: u64,
    pub cases: u32,
    pub max_depth: u8,
    pub oracle: Oracle,
    pub out_dir: PathBuf,
    pub case_timeout_ms: u64,
    pub total_timeout_ms: u64,
    pub rss_limit_bytes: u64,
    pub cpu_limit_ms: u64,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            cases: 100,
            max_depth: 8,
            oracle: Oracle::Check,
            out_dir: PathBuf::from("target/fuzz/cases"),
            case_timeout_ms: 2_000,
            total_timeout_ms: 30_000,
            rss_limit_bytes: 1024 * 1024 * 1024,
            cpu_limit_ms: 2_000,
        }
    }
}

impl RunOptions {
    /// Rejects zero, excessive, or otherwise unsafe run budgets.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=10_000).contains(&self.cases) {
            return Err("--cases must be between 1 and 10000".into());
        }
        if !(1..=32).contains(&self.max_depth) {
            return Err("--max-depth must be between 1 and 32".into());
        }
        if self.case_timeout_ms == 0 || self.case_timeout_ms > 60_000 {
            return Err("--case-timeout-ms must be between 1 and 60000".into());
        }
        if self.total_timeout_ms == 0 || self.total_timeout_ms > 300_000 {
            return Err("--total-timeout-ms must be between 1 and 300000".into());
        }
        if self.rss_limit_bytes == 0 || self.rss_limit_bytes > 64 * 1024 * 1024 * 1024 {
            return Err("--rss-limit-mib must be between 1 and 65536".into());
        }
        if self.cpu_limit_ms == 0 || self.cpu_limit_ms > 300_000 {
            return Err("--cpu-limit-ms must be between 1 and 300000".into());
        }
        Ok(())
    }
}

/// Stable replay metadata persisted beside each generated case.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CaseManifest {
    pub schema: String,
    pub generator: String,
    pub compiler_version: String,
    pub compiler_revision: String,
    pub case_id: String,
    pub seed: u64,
    pub case_index: u32,
    pub max_depth: u8,
    pub family: String,
    pub expected_type: String,
    pub oracle: Oracle,
    pub case_timeout_ms: u64,
    pub rss_limit_bytes: u64,
    pub cpu_limit_ms: u64,
}

/// Expected typed value used by checked semantic oracles.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TypedValue {
    pub schema: &'static str,
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: serde_json::Value,
}

/// Complete generated project, source, manifest, and expected result.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedCase {
    pub manifest: CaseManifest,
    pub project: String,
    pub source: String,
    pub support_sources: Vec<GeneratedSource>,
    pub expected: TypedValue,
}

/// One additional module required by a generated multi-module case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedSource {
    pub path: PathBuf,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid(mut options: RunOptions, update: impl FnOnce(&mut RunOptions)) -> String {
        update(&mut options);
        options.validate().expect_err("options must be rejected")
    }

    #[test]
    fn oracle_names_are_exact() {
        assert_eq!(Oracle::parse("check"), Ok(Oracle::Check));
        assert_eq!(Oracle::parse("js"), Ok(Oracle::JavaScript));
        assert_eq!(Oracle::parse("native"), Ok(Oracle::Native));
        assert_eq!(Oracle::parse("none"), Ok(Oracle::None));
        assert_eq!(
            Oracle::parse("Check"),
            Err("--oracle must be `check`, `js`, `native`, or `none`".into())
        );
    }

    #[test]
    fn default_options_are_valid() {
        let options = RunOptions::default();
        assert_eq!(options.validate(), Ok(()));
        assert_eq!(options.seed, 1);
        assert_eq!(options.oracle, Oracle::Check);
        assert_eq!(options.out_dir, PathBuf::from("target/fuzz/cases"));
    }

    #[test]
    fn every_numeric_budget_rejects_both_invalid_boundaries() {
        let cases_error = "--cases must be between 1 and 10000";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.cases = 0),
            cases_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| value.cases = 10_001),
            cases_error
        );

        let depth_error = "--max-depth must be between 1 and 32";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.max_depth = 0),
            depth_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| value.max_depth = 33),
            depth_error
        );

        let case_timeout_error = "--case-timeout-ms must be between 1 and 60000";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.case_timeout_ms = 0),
            case_timeout_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| value.case_timeout_ms =
                60_001),
            case_timeout_error
        );

        let total_timeout_error = "--total-timeout-ms must be between 1 and 300000";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.total_timeout_ms = 0),
            total_timeout_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| value.total_timeout_ms =
                300_001),
            total_timeout_error
        );

        let rss_error = "--rss-limit-mib must be between 1 and 65536";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.rss_limit_bytes = 0),
            rss_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| {
                value.rss_limit_bytes = 64 * 1024 * 1024 * 1024 + 1;
            }),
            rss_error
        );

        let cpu_error = "--cpu-limit-ms must be between 1 and 300000";
        assert_eq!(
            invalid(RunOptions::default(), |value| value.cpu_limit_ms = 0),
            cpu_error
        );
        assert_eq!(
            invalid(RunOptions::default(), |value| value.cpu_limit_ms = 300_001),
            cpu_error
        );
    }
}
