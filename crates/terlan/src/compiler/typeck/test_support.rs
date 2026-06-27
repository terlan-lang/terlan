use super::*;
use crate::terlan_hir::{
    load_interfaces_from_file_set, resolve_syntax_module_output,
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Typechecks source through the formal syntax-output path.
///
/// Inputs:
/// - `source`: Terlan source text to parse, resolve, and typecheck.
///
/// Output:
/// - Diagnostics produced by formal syntax-output typechecking.
///
/// Transformation:
/// - Parses the source into syntax output, resolves the module without
///   external interfaces, and invokes the typechecker entrypoint.
pub(super) fn check_syntax_output(source: &str) -> Vec<Diagnostic> {
    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output(&module).module;
    type_check_syntax_module_output(&module, &resolved)
}

/// Typechecks source with checked-in std interfaces loaded.
///
/// Inputs:
/// - `source`: Terlan source text to parse and typecheck.
/// - `std_relative_path`: repository-relative std source path used as the
///   anchor for std summary discovery.
///
/// Output:
/// - Diagnostics produced by formal syntax-output typechecking.
///
/// Transformation:
/// - Loads interfaces through `load_interfaces_from_file_set`, resolves the
///   parsed source against those interfaces, and typechecks with the same
///   std summary visibility used by external compiler commands.
pub(super) fn check_syntax_output_with_std_interfaces(
    source: &str,
    std_relative_path: &str,
) -> Vec<Diagnostic> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(std_relative_path);
    let interfaces = load_interfaces_from_file_set(&fixture_path.to_string_lossy());
    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    type_check_syntax_module_output(&module, &resolved)
}

/// Typechecks source against one ad-hoc provider interface.
///
/// Inputs:
/// - `source`: Terlan source text to parse and typecheck.
/// - `interface_source`: provider module source parsed as an interface module.
///
/// Output:
/// - Diagnostics produced by formal syntax-output typechecking.
///
/// Transformation:
/// - Parses the provider interface, converts it into `ModuleInterface`, resolves
///   the source module with that interface map, and invokes the typechecker.
pub(super) fn check_syntax_output_with_interface(
    source: &str,
    interface_source: &str,
) -> Vec<Diagnostic> {
    let interface_module = parse_interface_module_as_syntax_output(interface_source)
        .unwrap_or_else(|err| panic!("failed to parse syntax interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        interface_module.module_name.clone(),
        syntax_module_output_to_interface(&interface_module),
    );

    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    type_check_syntax_module_output(&module, &resolved)
}
