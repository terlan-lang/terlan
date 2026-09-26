//! Interpret explicit Cargo test requests against the canonical suite's build policy.

use crate::PhaseFailure;
use serde::Deserialize;
use std::collections::BTreeSet;

/// One gate's original arguments, never a shell command to execute.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    /// Unique gate/request identity preserved in the batch result.
    pub(super) id: String,
    /// Arguments following `cargo test` or the exact-test wrapper.
    pub(super) arguments: Vec<String>,
}

/// Target and libtest selectors admitted under the canonical debug/union policy.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Selection {
    /// Exact package name; wildcard and default workspace selection are rejected.
    pub(super) package: String,
    /// Declared library, binary, or integration-test target kind.
    pub(super) kind: &'static str,
    /// Named binary/integration target; a library is unique within its package.
    pub(super) target: Option<String>,
    /// Shared libtest selectors with presentation-only arguments omitted.
    pub(super) selectors: Vec<String>,
}

/// Validates the complete request set before any input scan or test producer launch.
pub(super) fn parse_batch(bytes: &[u8]) -> Result<Vec<(Request, Selection)>, PhaseFailure> {
    if bytes.len() > 1024 * 1024 {
        return Err(failure("coverage request document exceeds its byte budget"));
    }
    let requests: Vec<Request> = serde_json::from_slice(bytes).map_err(failure)?;
    if requests.is_empty() || requests.len() > 4096 {
        return Err(failure("empty or excessive coverage requests"));
    }
    let mut ids = BTreeSet::new();
    requests
        .into_iter()
        .map(|request| {
            if request.id.is_empty()
                || request.id.len() > 256
                || request.id.chars().any(char::is_control)
                || !ids.insert(request.id.clone())
            {
                return Err(failure("invalid or duplicate coverage request ID"));
            }
            let selection = parse(&request.arguments)?;
            Ok((request, selection))
        })
        .collect()
}

fn parse(arguments: &[String]) -> Result<Selection, PhaseFailure> {
    if arguments.len() > 256 || arguments.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err(failure("excessive Cargo test arguments"));
    }
    let mut args = arguments.iter().map(String::as_str);
    let mut package = None;
    let mut target = None;
    let mut features = BTreeSet::new();
    let mut selectors = Vec::new();
    let mut filter = false;
    while let Some(argument) = args.next() {
        match argument {
            "--" => {
                selectors.extend(args.map(str::to_owned));
                break;
            }
            "--locked" | "--offline" | "--frozen" => {}
            "-p" | "--package" => {
                if package
                    .replace(value(&mut args, argument)?.to_owned())
                    .is_some()
                {
                    return Err(failure(
                        "multiple package selections require a separate request",
                    ));
                }
            }
            "--lib" | "--test" | "--bin" => {
                let selected = match argument {
                    "--lib" => ("lib", None),
                    "--test" => ("test", Some(value(&mut args, argument)?.to_owned())),
                    _ => ("bin", Some(value(&mut args, argument)?.to_owned())),
                };
                if target.replace(selected).is_some() {
                    return Err(failure(
                        "multiple target selections require separate requests",
                    ));
                }
            }
            "--features" | "-F" => {
                features.extend(
                    value(&mut args, argument)?
                        .split([',', ' '])
                        .filter(|item| !item.is_empty())
                        .map(str::to_owned),
                );
            }
            other if other.starts_with('-') => {
                return Err(failure(format!(
                    "Cargo option needs its own execution policy: {other}"
                )));
            }
            other => {
                if filter {
                    return Err(failure("Cargo accepts only one pre-separator test filter"));
                }
                filter = true;
                selectors.push(other.to_owned());
            }
        }
    }
    let package = package.ok_or_else(|| failure("coverage requires an explicit package"))?;
    let (kind, target) =
        target.ok_or_else(|| failure("coverage requires an explicit test target"))?;
    if !identifier(&package) || target.as_deref().is_some_and(|name| !identifier(name)) {
        return Err(failure("package and target must be literal Cargo names"));
    }
    for feature in features {
        let canonical = feature.strip_prefix("terlan/").unwrap_or(&feature);
        if package != "terlan"
            || !crate::VALIDATION_FEATURES
                .split(',')
                .any(|value| value == canonical)
        {
            return Err(failure(format!(
                "feature is not covered by canonical suite policy: {feature}"
            )));
        }
    }
    // Reuse libtest's selector parser; do not invent a second matching algorithm.
    let refs = selectors.iter().map(String::as_str).collect::<Vec<_>>();
    crate::test_inventory::select(&BTreeSet::new(), &refs)?;
    Ok(Selection {
        package,
        kind,
        target,
        selectors,
    })
}

fn value<'a>(
    args: &mut impl Iterator<Item = &'a str>,
    option: &str,
) -> Result<&'a str, PhaseFailure> {
    args.next()
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| failure(format!("missing value for {option}")))
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Keeps Cargo request failures distinct from successful historical coverage observations.
pub(super) fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-coverage-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "coverage_requests_test.rs"]
mod tests;
