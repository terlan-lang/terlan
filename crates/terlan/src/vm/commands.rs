/// Minimal command-helper surface required by the standalone VM frontend.
///
/// Inputs:
/// - Compiler and validation modules that still refer to command-owned helper
///   modules for JSON, artifact collection, and static template constants.
///
/// Output:
/// - A narrow compatibility module for `terlan-vm` builds.
///
/// Transformation:
/// - Reuses the production helper implementations without importing the full
///   `terlc` command dispatcher into the standalone VM artifact.
#[path = "../commands/artifacts.rs"]
pub(crate) mod artifacts;

#[path = "../commands/json.rs"]
pub(crate) mod json;

pub(crate) mod static_site {
    /// Reserved template prop name used for component children.
    pub(crate) const TEMPLATE_CHILDREN_SLOT: &str = "children";
}
