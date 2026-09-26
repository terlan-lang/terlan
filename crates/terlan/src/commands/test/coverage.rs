//! Runtime-backed source declaration coverage for native Terlan tests.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::runtime::native_image::debug::{
    inspect_tvm_native_debug, tvm_debug_source_sha256, TvmNativeDebugRecord,
};

/// Source identity shared by all native specializations of one declaration.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct DeclarationIdentity {
    source_file: PathBuf,
    span_start: usize,
    span_end: usize,
}

#[derive(Clone, Debug)]
struct DeclarationCallables {
    module: String,
    function: String,
    arity: usize,
    callable_ids: BTreeSet<u64>,
}

/// Native callable identities accumulated across one test command.
#[derive(Default)]
pub(super) struct DeclarationCoverage {
    declarations: BTreeMap<DeclarationIdentity, DeclarationCallables>,
    covered_callables: BTreeSet<u64>,
}

impl DeclarationCoverage {
    /// Merges one executed native image into this command-level report.
    pub(super) fn record_image(
        &mut self,
        image_path: &Path,
        source_roots: &[PathBuf],
        covered_callables: &BTreeSet<u64>,
    ) -> Result<(), String> {
        let image = fs::read(image_path).map_err(|error| {
            format!(
                "error[test.coverage.image]: failed to read `{}`: {error}",
                image_path.display()
            )
        })?;
        let records = inspect_tvm_native_debug(&image)?;
        self.record_debug_records(records, source_roots)?;
        self.covered_callables.extend(covered_callables);
        Ok(())
    }

    fn record_debug_records(
        &mut self,
        records: Vec<TvmNativeDebugRecord>,
        source_roots: &[PathBuf],
    ) -> Result<(), String> {
        let roots = source_roots
            .iter()
            .map(|root| canonical_path(root))
            .collect::<Result<Vec<_>, _>>()?;
        for record in records {
            let source_file = canonical_path(Path::new(&record.source_file))?;
            if !roots.iter().any(|root| source_file.starts_with(root)) {
                continue;
            }
            let source = fs::read(&source_file).map_err(|error| {
                format!(
                    "error[test.coverage.source]: failed to read `{}`: {error}",
                    source_file.display()
                )
            })?;
            if tvm_debug_source_sha256(&source) != record.source_sha256 {
                return Err(format!(
                    "error[test.coverage.source_changed]: `{}` changed after native compilation",
                    source_file.display()
                ));
            }
            let identity = DeclarationIdentity {
                source_file,
                span_start: record.span_start,
                span_end: record.span_end,
            };
            let declaration =
                self.declarations
                    .entry(identity)
                    .or_insert_with(|| DeclarationCallables {
                        module: record.module.clone(),
                        function: record.function.clone(),
                        arity: record.arity,
                        callable_ids: BTreeSet::new(),
                    });
            if declaration.module.starts_with("$terlan.") && !record.module.starts_with("$terlan.")
            {
                declaration.module.clone_from(&record.module);
                declaration.function.clone_from(&record.function);
                declaration.arity = record.arity;
            }
            declaration.callable_ids.insert(record.callable_id);
        }
        Ok(())
    }

    /// Prints a deterministic report and evaluates an optional percentage gate.
    pub(super) fn finish(&self, threshold: Option<u8>) -> Result<bool, String> {
        if self.declarations.is_empty() {
            return Err(
                "error[test.coverage.empty]: no project source declarations were present in the native test images"
                    .to_string(),
            );
        }
        let mut covered_callables = self.covered_callables.clone();
        if let Some(path) = std::env::var_os("TERLAN_CALLABLE_COVERAGE_FILE")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
        {
            let contents = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "error[test.coverage.hits]: failed to read `{}`: {error}",
                    path.display()
                )
            })?;
            for (index, line) in contents.lines().enumerate() {
                let callable_id = line.parse::<u64>().map_err(|_| {
                    format!(
                        "error[test.coverage.hits]: invalid callable id at `{}:{}`",
                        path.display(),
                        index + 1
                    )
                })?;
                covered_callables.insert(callable_id);
            }
        }
        let uncovered = self
            .declarations
            .iter()
            .filter(|(_, declaration)| declaration.callable_ids.is_disjoint(&covered_callables))
            .collect::<Vec<_>>();
        let total = self.declarations.len();
        let covered = total - uncovered.len();
        let percentage = covered as f64 * 100.0 / total as f64;
        println!("Terlan declaration coverage: {covered}/{total} ({percentage:.2}%)");
        for (identity, declaration) in uncovered {
            println!(
                "uncovered {}:{}: {}.{}/{}",
                identity.source_file.display(),
                source_line(&identity.source_file, identity.span_start)?,
                declaration.module,
                declaration.function,
                declaration.arity
            );
        }
        Ok(threshold.is_none_or(|required| {
            covered.saturating_mul(100) >= total.saturating_mul(usize::from(required))
        }))
    }
}

fn canonical_path(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path).map_err(|error| {
        format!(
            "error[test.coverage.path]: failed to resolve `{}`: {error}",
            path.display()
        )
    })
}

fn source_line(path: &Path, offset: usize) -> Result<usize, String> {
    let source = fs::read(path).map_err(|error| {
        format!(
            "error[test.coverage.source]: failed to read `{}`: {error}",
            path.display()
        )
    })?;
    if offset > source.len() {
        return Err(format!(
            "error[test.coverage.span]: declaration offset {offset} exceeds `{}`",
            path.display()
        ));
    }
    Ok(source[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1)
}
