mod command;
mod dependencies;
mod directory;
#[cfg(test)]
mod interface_first_test;
mod project;

#[cfg(test)]
#[path = "mod_test.rs"]
#[cfg(test)]
mod mod_test;

pub(crate) use command::effective_check_target_profile;
#[cfg(test)]
use command::parse_check_args;
pub(crate) use command::run;
#[cfg(test)]
use directory::check_dir_phase_manifest_path;
/// Shared directory entry used by checks, tests, and JavaScript builds.
pub(crate) fn run_check_dir(
    path: &str,
    state: crate::CliState,
    manifest: Option<&std::path::Path>,
) -> std::process::ExitCode {
    project::run(path, state, manifest, false)
}

/// Checks a source root in a graph resolved against the consumer's lockfile.
pub(crate) fn run_check_resolved_root(
    root: &std::path::Path,
    roots: &[std::path::PathBuf],
    state: crate::CliState,
) -> std::process::ExitCode {
    let dependencies = roots
        .iter()
        .filter(|candidate| candidate.as_path() != root)
        .cloned()
        .collect::<Vec<_>>();
    project::run_with_roots(
        &root.to_string_lossy(),
        state,
        None,
        false,
        Some(&dependencies),
    )
}
