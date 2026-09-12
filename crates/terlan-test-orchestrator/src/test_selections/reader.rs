//! Read-only historical coverage inspection; never a current-input reuse decision.

use super::*;
use std::ffi::{OsStr, OsString};
use std::process::ExitCode;
use std::time::Duration;

/// Reads a completed report and its fixed-name companion without launching producers.
pub(crate) fn inspect_main(mut arguments: impl Iterator<Item = OsString>) -> ExitCode {
    let result = (|| {
        let report = arguments
            .next()
            .ok_or_else(|| failure("missing suite report"))?;
        if arguments.next().as_deref() != Some(OsStr::new("--")) {
            return Err(failure("expected -- before libtest selectors"));
        }
        let mut selectors = Vec::new();
        let mut bytes = 0_usize;
        for argument in arguments {
            let argument = argument
                .into_string()
                .map_err(|_| failure("selectors must be UTF-8"))?;
            bytes = bytes.saturating_add(argument.len());
            if selectors.len() == 256 || bytes > 64 * 1024 {
                return Err(failure("excessive coverage selectors"));
            }
            selectors.push(argument);
        }
        let shutdown = crate::shutdown::Shutdown::install().map_err(failure)?;
        let control =
            ProcessControl::new(Duration::from_secs(30)).with_cancellation(shutdown.flag());
        let inventory = read(Path::new(&report), control)?;
        let selectors = selectors.iter().map(String::as_str).collect::<Vec<_>>();
        let mut coverage = inventory.inspect_coverage(&selectors)?;
        coverage["suite_run_id"] = json!(inventory.run_id);
        coverage["selection_sha256"] = json!(inventory.digest);
        coverage["current_inputs_verified"] = json!(false);
        serde_json::to_writer(std::io::stdout().lock(), &coverage).map_err(failure)?;
        println!();
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[rust-coverage] {}: {}", error.outcome, error.detail);
            ExitCode::FAILURE
        }
    }
}

fn read(path: &Path, control: ProcessControl<'_>) -> Result<TestSelections, PhaseFailure> {
    Ok(read_completed(path, control)?.1)
}

/// Shares validated historical documents with current-input verification without rereading.
pub(crate) fn read_completed(
    path: &Path,
    control: ProcessControl<'_>,
) -> Result<(Value, TestSelections, String), PhaseFailure> {
    let (report, report_digest) = document(path, 1024 * 1024, control)?;
    let mut companion = path.as_os_str().to_owned();
    companion.push(".selections.json");
    let companion = PathBuf::from(companion);
    let (document, digest) = document(&companion, MAX_BYTES, control)?;
    let inventory = restore(&report, &document, &digest, &companion)?;
    Ok((report, inventory, report_digest))
}

/// Reads and hashes a bounded historical JSON file without following a final symlink.
pub(crate) fn document(
    path: &Path,
    limit: u64,
    control: ProcessControl<'_>,
) -> Result<(Value, String), PhaseFailure> {
    if !std::fs::symlink_metadata(path).map_err(failure)?.is_file() {
        return Err(failure("coverage document is not a regular file"));
    }
    let bytes = crate::file_identity::read_hashed_file(
        path,
        &mut Sha256::new(),
        limit,
        control,
        Instant::now(),
    )?;
    let digest = hex(Sha256::new_with_prefix(&bytes));
    Ok((serde_json::from_slice(&bytes).map_err(failure)?, digest))
}

fn restore(
    report: &Value,
    document: &Value,
    digest: &str,
    path: &Path,
) -> Result<TestSelections, PhaseFailure> {
    let binding = &report["test_selection_binding"];
    let run_id = report["run_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| failure("missing suite run ID"))?;
    if report["schema"] != "terlan.rust-test-suite.v4"
        || report["decision"] != "pass"
        || document["schema"] != "terlan.rust-test-selections.v2"
        || document["scope"] != "admitted-main-harness-selections-v1"
        || document["package"] != "terlan"
        || document["target"] != "lib"
        || document["features"] != crate::VALIDATION_FEATURES
        || document["suite_run_id"] != run_id
        || binding["suite_run_id"] != run_id
        || binding["scope"] != document["scope"]
        || binding["sha256"] != digest
        || binding["verified"] != true
        || binding["reusable"] != false
        || document["reusable"] != false
    {
        return Err(failure(
            "incomplete or mismatched historical coverage documents",
        ));
    }
    let executables = &report["executable_binding"];
    let harnesses = array(&executables["before"])?
        .iter()
        .filter(|row| row["role"] == "terlan-library-harness")
        .collect::<Vec<_>>();
    if executables["verified"] != true
        || executables["before"] != executables["after"]
        || harnesses.len() != 1
        || harnesses[0] != &document["harness"]
        || document["harness"]["identity_sha256"]
            .as_str()
            .is_none_or(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    {
        return Err(failure(
            "coverage documents disagree on the executed harness",
        ));
    }
    let mut budget = NameBudget::default();
    let compiled = budget.read(&document["compiled"]["all"])?;
    let original_ignored = budget.read(&document["compiled"]["ignored"])?;
    if compiled.is_empty() || !original_ignored.is_subset(&compiled) {
        return Err(failure("invalid complete compiled inventory"));
    }
    let rows = array(&document["phases"])?;
    if rows.is_empty() || rows.len() > 256 {
        return Err(failure("empty or excessive phase inventory"));
    }
    let mut selections = BTreeMap::new();
    let mut covered_names = BTreeSet::new();
    for row in rows {
        let name = row["phase"]
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| failure("missing phase name"))?;
        let selected = ExpectedTests {
            passed: budget.read(&row["passed"])?,
            ignored: budget.read(&row["ignored"])?,
            filtered: count(&row["filtered"])?,
        };
        if selected.passed.is_empty()
            || !selected.passed.is_disjoint(&selected.ignored)
            || !selected.passed.is_subset(&compiled)
            || !selected.ignored.is_subset(&original_ignored)
            || selected
                .passed
                .len()
                .checked_add(selected.ignored.len())
                .and_then(|n| n.checked_add(selected.filtered))
                != Some(compiled.len())
            || row["selection_identity_sha256"] != selected.identity()
            || selected
                .passed
                .iter()
                .any(|name| !covered_names.insert(name.clone()))
            || selections.insert(name.to_owned(), selected).is_some()
        {
            return Err(failure("invalid or duplicate test selection ownership"));
        }
    }
    let mut seen = BTreeSet::new();
    let phases = array(&report["phases"])?;
    if phases.is_empty()
        || phases.len() > 1024
        || phases.iter().any(|row| row["outcome"] != "passed")
    {
        return Err(failure(
            "suite includes an unfinished or unsuccessful phase",
        ));
    }
    for phase in phases
        .iter()
        .filter(|row| row["executor"] == "direct-terlan-harness")
    {
        let name = phase["name"]
            .as_str()
            .ok_or_else(|| failure("missing completed phase name"))?;
        let selected = selections
            .get(name)
            .ok_or_else(|| failure("unadmitted completed phase"))?;
        if !seen.insert(name) {
            return Err(failure("duplicate completed phase"));
        }
        validate_completion(
            "passed",
            phase["child_pid"]
                .as_u64()
                .and_then(|pid| u32::try_from(pid).ok()),
            &phase["test_execution"],
            selected,
        )?;
    }
    if seen.len() != selections.len()
        || binding["phases"] != selections.len()
        || binding["tests"] != covered_names.len()
        || binding["compiled_tests"] != compiled.len()
        || binding["originally_ignored"] != original_ignored.len()
    {
        return Err(failure("incomplete coverage phase accounting"));
    }
    let mut inventory = TestSelections {
        path: path.to_owned(),
        digest: digest.into(),
        run_id: run_id.into(),
        selections,
        compiled,
        original_ignored,
        covered_names,
        coverage: Value::Null,
        verified: true,
    };
    inventory.coverage = json!({"normal":inventory.inspect_coverage(&[])?, "ignored":inventory.inspect_coverage(&["--ignored"])?});
    if binding["coverage"] != inventory.coverage {
        return Err(failure(
            "coverage summary disagrees with exact completed names",
        ));
    }
    Ok(inventory)
}

#[derive(Default)]
struct NameBudget {
    count: usize,
    bytes: usize,
}

impl NameBudget {
    fn read(&mut self, value: &Value) -> Result<BTreeSet<String>, PhaseFailure> {
        let mut names = BTreeSet::new();
        for value in array(value)? {
            let name = value
                .as_str()
                .ok_or_else(|| failure("test name is not a string"))?;
            self.count = self.count.saturating_add(1);
            self.bytes = self.bytes.saturating_add(name.len());
            if name.is_empty()
                || name.chars().any(char::is_whitespace)
                || self.count > MAX_NAMES
                || self.bytes > MAX_BYTES as usize / 2
                || !names.insert(name.to_owned())
            {
                return Err(failure("invalid, duplicate or excessive test names"));
            }
        }
        Ok(names)
    }
}

fn array(value: &Value) -> Result<&Vec<Value>, PhaseFailure> {
    value
        .as_array()
        .ok_or_else(|| failure("missing coverage array"))
}

fn count(value: &Value) -> Result<usize, PhaseFailure> {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| failure("invalid coverage count"))
}

#[cfg(test)]
#[path = "reader_test.rs"]
mod tests;
