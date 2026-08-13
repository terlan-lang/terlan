mod command;
mod directory;

#[cfg(test)]
#[path = "mod_test.rs"]
#[cfg(test)]
mod mod_test;

#[cfg(test)]
use command::parse_check_args;
pub(crate) use command::run;
#[cfg(test)]
use directory::check_dir_phase_manifest_path;
pub(crate) use directory::run_check_dir;
