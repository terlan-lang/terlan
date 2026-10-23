pub(crate) use crate::support::library_paths::installed_share_root;
#[cfg(test)]
use crate::support::library_paths::share_root_for_executable;

#[cfg(test)]
#[path = "release_layout_test.rs"]
mod release_layout_test;
