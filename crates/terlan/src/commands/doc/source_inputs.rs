//! Bounded parallel preparation for deterministic documentation validation.

use std::path::{Path, PathBuf};

use crate::terlan_syntax::SyntaxModuleOutput;

/// Source and syntax prepared in the caller's original path order.
#[derive(Debug)]
pub(super) struct Prepared {
    pub(super) path_text: String,
    pub(super) source: String,
    pub(super) syntax_output: SyntaxModuleOutput,
}

/// One deterministic source preparation failure.
#[derive(Debug)]
pub(super) enum Failure {
    Message(String),
    Parse {
        message: String,
        path: String,
        start: usize,
        end: usize,
    },
}

/// Reads and parses independent modules on at most four host workers.
pub(super) fn prepare(files: Vec<PathBuf>) -> Vec<Result<Prepared, Failure>> {
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(4)
        .min(files.len().max(1));
    let mut prepared = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let files = &files;
            handles.push(scope.spawn(move || {
                (worker..files.len())
                    .step_by(workers)
                    .map(|index| (index, prepare_one(&files[index])))
                    .collect::<Vec<_>>()
            }));
        }
        let mut prepared = Vec::with_capacity(files.len());
        for handle in handles {
            prepared.extend(handle.join().expect("documentation source worker"));
        }
        prepared
    });
    prepared.sort_by_key(|(index, _)| *index);
    prepared.into_iter().map(|(_, result)| result).collect()
}

fn prepare_one(file: &Path) -> Result<Prepared, Failure> {
    let path_text = file.to_string_lossy().to_string();
    let source = crate::support::read_file(&path_text)
        .map_err(|error| Failure::Message(error.to_string()))?;
    let syntax_output = crate::formal_pipeline::parse_source_as_syntax_output(&path_text, &source)
        .map_err(|error| match error {
            crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span) => Failure::Parse {
                message,
                path: path_text.clone(),
                start: span.start,
                end: span.end,
            },
            crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message) => {
                Failure::Message(message)
            }
        })?;
    Ok(Prepared {
        path_text,
        source,
        syntax_output,
    })
}

#[cfg(test)]
#[path = "source_inputs_test.rs"]
mod tests;
