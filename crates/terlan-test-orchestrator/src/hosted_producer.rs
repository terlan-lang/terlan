//! Workflow context for exported coverage; authentication belongs to its consumer.

use crate::coverage_requests::failure;
use crate::execution_environment::ExecutionEnvironment;
use crate::PhaseFailure;
use serde_json::{json, Value};

/// Records bounded GitHub producer coordinates without claiming that environment values authenticate them.
pub(super) fn context(environment: &ExecutionEnvironment) -> Result<Value, PhaseFailure> {
    if environment.value("GITHUB_ACTIONS").as_deref() != Some(std::ffi::OsStr::new("true")) {
        return Ok(Value::Null);
    }
    let repository = field(environment, "GITHUB_REPOSITORY", 256)?;
    let parts = repository.split('/').collect::<Vec<_>>();
    if parts.len() != 2 || parts.iter().any(|part| !identifier(part)) {
        return Err(failure("invalid GITHUB_REPOSITORY"));
    }
    let revision = field(environment, "GITHUB_SHA", 40)?;
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(failure("invalid GITHUB_SHA"));
    }
    let workflow = field(environment, "GITHUB_WORKFLOW_REF", 512)?;
    let prefix = format!("{repository}/.github/workflows/");
    let (name, reference) = workflow
        .strip_prefix(&prefix)
        .and_then(|value| value.split_once('@'))
        .filter(|(name, reference)| identifier(name) && !reference.is_empty())
        .ok_or_else(|| failure("invalid GITHUB_WORKFLOW_REF"))?;
    if !(name.ends_with(".yml") || name.ends_with(".yaml")) || !reference.starts_with("refs/") {
        return Err(failure("invalid workflow filename or reference"));
    }
    let job = field(environment, "GITHUB_JOB", 128)?;
    if !identifier(&job) {
        return Err(failure("invalid GITHUB_JOB"));
    }
    Ok(
        json!({"provider":"github-actions","repository":repository,"source_revision":revision,
        "run_id":number(environment,"GITHUB_RUN_ID")?,"run_attempt":number(environment,"GITHUB_RUN_ATTEMPT")?,
        "workflow_ref":workflow,"job":job,"host_os":std::env::consts::OS,
        "host_arch":std::env::consts::ARCH,"authentication_required":true}),
    )
}

fn field(
    environment: &ExecutionEnvironment,
    key: &str,
    maximum: usize,
) -> Result<String, PhaseFailure> {
    environment
        .value(key)
        .and_then(|value| value.into_string().ok())
        .filter(|value| {
            !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| failure(format!("missing or invalid hosted producer field: {key}")))
}

fn number(environment: &ExecutionEnvironment, key: &str) -> Result<u64, PhaseFailure> {
    let value = field(environment, key, 20)?;
    value
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0 && number.to_string() == value)
        .ok_or_else(|| failure(format!("invalid hosted producer number: {key}")))
}

fn identifier(value: &str) -> bool {
    !matches!(value, "" | "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

#[cfg(test)]
#[path = "hosted_producer_test.rs"]
mod tests;
