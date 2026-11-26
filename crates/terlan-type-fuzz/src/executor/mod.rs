mod artifacts;
mod interface_cache_transaction;
mod interface_roundtrip;
mod javascript;
mod oracle;
mod package_roundtrip;
mod planning;
mod reduce;
mod resources;

use std::path::Path;

use crate::{Action, GeneratedCase, GenerationConfig, RunOptions};

pub(crate) fn run_generated(
    options: &RunOptions,
    compiler: &Path,
    compiler_revision: &str,
) -> Result<(), String> {
    artifacts::generate_run(options, compiler, compiler_revision)
}

pub(crate) fn write_artifact(path: &Path, bytes: &[u8]) -> Result<(), String> {
    artifacts::atomic_write(path, bytes)
}

/// Executes one validated fuzz action with an explicitly selected compiler.
pub fn execute(action: Action, compiler: &Path, compiler_revision: &str) -> Result<(), String> {
    match action {
        Action::Types(options) => artifacts::generate_run(&options, compiler, compiler_revision),
        Action::Replay(path) => artifacts::replay(&path, compiler),
        Action::Reduce(path) => reduce::run(&path, compiler),
        Action::CorpusCheck(path) => artifacts::check_corpus(&path, compiler),
        Action::Profile(request) => crate::profile::run(&request, compiler, compiler_revision),
        Action::VerifyResult(path) => crate::profile::verify(&path, compiler_revision),
    }
}

fn generate_case(index: u32, options: &RunOptions, compiler_revision: &str) -> GeneratedCase {
    crate::generate::generate_selected(
        index,
        &GenerationConfig {
            seed: options.seed,
            max_depth: options.max_depth,
            oracle: options.oracle,
            timeout_ms: options.case_timeout_ms,
            rss_limit_bytes: options.rss_limit_bytes,
            cpu_limit_ms: options.cpu_limit_ms,
            compiler_revision,
        },
        &options.families,
    )
}

fn regenerate_case(
    index: u32,
    options: &RunOptions,
    compiler_revision: &str,
    generator: &str,
    family: &str,
) -> Result<GeneratedCase, String> {
    crate::generate::regenerate(
        index,
        &GenerationConfig {
            seed: options.seed,
            max_depth: options.max_depth,
            oracle: options.oracle,
            timeout_ms: options.case_timeout_ms,
            rss_limit_bytes: options.rss_limit_bytes,
            cpu_limit_ms: options.cpu_limit_ms,
            compiler_revision,
        },
        generator,
        family,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_offline_actions_are_owned_without_a_compiler_process() {
        assert!(execute(
            Action::Replay("missing-case".into()),
            Path::new("missing-compiler"),
            "test",
        )
        .unwrap_err()
        .contains("read case manifest"));
        assert!(execute(
            Action::Reduce("missing-case".into()),
            Path::new("missing-compiler"),
            "test",
        )
        .unwrap_err()
        .contains("read case manifest"));
        assert!(execute(
            Action::CorpusCheck("missing-corpus".into()),
            Path::new("missing-compiler"),
            "test",
        )
        .unwrap_err()
        .contains("read corpus directory"));

        let root = tempfile::tempdir().expect("fuzz root");
        let options = RunOptions {
            cases: 1,
            oracle: crate::Oracle::None,
            out_dir: root.path().to_path_buf(),
            ..RunOptions::default()
        };
        execute(
            Action::Types(options),
            Path::new("missing-compiler"),
            "test",
        )
        .expect("offline generation");

        let profile = root.path().join("profile");
        execute(
            Action::Profile(crate::ProfileRequest {
                profile: crate::FuzzProfile::Impacted,
                out_dir: profile.clone(),
                rotation: 0,
                changed_paths: vec!["docs/README.md".into()],
            }),
            Path::new("missing-compiler"),
            "test",
        )
        .expect("empty impacted profile");
        execute(
            Action::VerifyResult(profile.join("result.json")),
            Path::new("missing-compiler"),
            "test",
        )
        .expect("matching profile result");
    }
}
