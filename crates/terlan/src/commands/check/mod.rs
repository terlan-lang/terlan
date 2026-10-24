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
