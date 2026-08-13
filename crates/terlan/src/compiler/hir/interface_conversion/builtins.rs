use super::*;

/// Returns compiler-provided interfaces available without project files.
///
/// Inputs: none. Output: built-in interface map. Transformation: currently
/// returns an empty map because release std interfaces are loaded from source
/// summaries rather than hard-coded HIR metadata.
pub(in super::super) fn builtin_interfaces() -> HashMap<String, ModuleInterface> {
    HashMap::new()
}
