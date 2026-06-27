use std::collections::HashMap;

use crate::terlan_hir::ModuleInterface;

/// Builds a readable diagnostic for a missing selected-import provider module.
///
/// Inputs:
/// - `module`: resolved module path named by the selected import.
/// - `function`: function selected from that module.
/// - `interfaces`: loaded provider interfaces available to the current compile.
///
/// Output:
/// - Human-facing diagnostic message.
///
/// Transformation:
/// - Reports the missing module precisely and, when another loaded interface
///   exports the same function with the same leaf module name, appends a
///   concrete import suggestion.
pub(super) fn missing_imported_function_interface_message(
    module: &str,
    function: &str,
    interfaces: &HashMap<String, ModuleInterface>,
) -> String {
    let mut message = format!(
        "cannot find module `{}` for imported function `{}`; no interface for `{}` is loaded",
        module, function, module
    );
    if let Some(suggestion) = imported_function_module_suggestion(module, function, interfaces) {
        message.push_str(&format!(
            "; did you mean `{}.{{{}}}`?",
            suggestion, function
        ));
    }
    message
}

/// Builds a readable diagnostic for a missing selected function in an interface.
///
/// Inputs:
/// - `interface`: loaded module interface.
/// - `function`: selected function name.
/// - `arity`: call arity used at the source call site.
///
/// Output:
/// - Human-facing diagnostic text listing available public functions.
///
/// Transformation:
/// - Names the missing function as `module.function/arity` and appends a compact
///   sorted list of importable functions from the provider module.
pub(super) fn missing_imported_function_message(
    interface: &ModuleInterface,
    function: &str,
    arity: usize,
) -> String {
    let mut message = format!(
        "module `{}` has no imported function `{}/{}`",
        interface.module, function, arity
    );
    let available = available_interface_functions(interface);
    if !available.is_empty() {
        message.push_str(&format!("; available imports: {}", available.join(", ")));
    }
    message
}

/// Finds a likely loaded module for a selected function import typo.
///
/// Inputs:
/// - `module`: missing module from the source import.
/// - `function`: selected function name.
/// - `interfaces`: loaded provider interfaces.
///
/// Output:
/// - Suggested module path when a deterministic candidate is available.
///
/// Transformation:
/// - Prefers modules with the same final path segment, then falls back to any
///   loaded module exporting the selected function; ties sort lexicographically.
fn imported_function_module_suggestion(
    module: &str,
    function: &str,
    interfaces: &HashMap<String, ModuleInterface>,
) -> Option<String> {
    let leaf = module.rsplit('.').next().unwrap_or(module);
    let mut same_leaf = interfaces
        .iter()
        .filter(|(candidate, interface)| {
            candidate.rsplit('.').next().unwrap_or(candidate.as_str()) == leaf
                && interface
                    .functions
                    .keys()
                    .any(|(name, _arity)| name == function)
        })
        .map(|(candidate, _interface)| candidate.clone())
        .collect::<Vec<_>>();
    same_leaf.sort();
    if let Some(candidate) = same_leaf.into_iter().next() {
        return Some(candidate);
    }

    let mut by_function = interfaces
        .iter()
        .filter(|(_candidate, interface)| {
            interface
                .functions
                .keys()
                .any(|(name, _arity)| name == function)
        })
        .map(|(candidate, _interface)| candidate.clone())
        .collect::<Vec<_>>();
    by_function.sort();
    by_function.into_iter().next()
}

/// Lists public functions exported by an interface for diagnostics.
///
/// Inputs:
/// - `interface`: provider interface loaded for an import.
///
/// Output:
/// - Sorted `name/arity` strings.
///
/// Transformation:
/// - Reads interface function keys and formats them deterministically for
///   concise diagnostics.
fn available_interface_functions(interface: &ModuleInterface) -> Vec<String> {
    let mut names = interface
        .functions
        .keys()
        .map(|(name, arity)| format!("{}/{}", name, arity))
        .collect::<Vec<_>>();
    names.sort();
    names
}
