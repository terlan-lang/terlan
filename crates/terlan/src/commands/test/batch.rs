//! Explicit multi-path test orchestration.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use super::arguments::{TestArgs, TestTarget, TEST_SOURCE_PATTERN_DESCRIPTION};
use super::execution::{effective_js_test_profile, run_path};
use super::project_context::{collect_test_files, discover_test_project_root, is_test_source_path};
use crate::commands::dev_dependencies;
use crate::CliState;

/// Executes explicit multi-path tests as shared project suites.
///
/// Coverage is accumulated across every project image and evaluated once after
/// all selected owners have run. This prevents one early shard from failing a
/// source that is covered by a later shard in the same command.
pub(super) fn run_path_batch(args: &TestArgs, state: CliState, paths: &[String]) -> ExitCode {
    if args.emit_test_manifest.is_some() || args.emit_test_result_manifest.is_some() {
        eprintln!(
            "test manifest output is only supported for a single {TEST_SOURCE_PATTERN_DESCRIPTION} file"
        );
        return ExitCode::from(1);
    }
    let mut files = Vec::new();
    for path in paths {
        let path = Path::new(path);
        if path.is_dir() {
            if let Err(message) = collect_test_files(path, &mut files) {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        } else if is_test_source_path(&path.to_string_lossy()) {
            files.push(path.to_path_buf());
        } else {
            eprintln!(
                "terlc test requires a {TEST_SOURCE_PATTERN_DESCRIPTION} source file: {}",
                path.display()
            );
            return ExitCode::from(1);
        }
    }
    files.sort();
    files.dedup();
    if files.is_empty() {
        eprintln!("no {TEST_SOURCE_PATTERN_DESCRIPTION} files found");
        return ExitCode::from(1);
    }

    let mut sessions = Vec::new();
    if !matches!(args.target, TestTarget::Wasm) {
        let project_roots = files
            .iter()
            .filter_map(|file| discover_test_project_root(file))
            .collect::<BTreeSet<_>>();
        for project_root in project_roots {
            match dev_dependencies::start_project_dependencies(&project_root) {
                Ok(session) => sessions.push(session),
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        }
    }

    let exhaustive_project = paths.iter().any(|path| Path::new(path).is_dir());
    let mut outcome = match args.target {
        TestTarget::TerlanVm => {
            super::suite::run_terlan_vm_test_files(args, state.clone(), files, exhaustive_project)
        }
        TestTarget::Js => match effective_js_test_profile(state.target_profile) {
            Ok(profile) => super::suite::run_js_test_files(args, state.clone(), profile, files),
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        TestTarget::Wasm => {
            let mut failed = false;
            for path in paths {
                let mut path_args = args.clone();
                path_args.path.clone_from(path);
                path_args.additional_paths.clear();
                if run_path(&path_args, state.clone()) != ExitCode::SUCCESS {
                    failed = true;
                }
            }
            if failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
    };
    for session in sessions.into_iter().rev() {
        outcome = dev_dependencies::finish_dependency_session(Some(session), outcome);
    }
    outcome
}
