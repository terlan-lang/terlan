use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
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

/// Typechecks source with verified database schema evidence.
pub(super) fn check_syntax_output_with_database_schema(
    source: &str,
    database_schema: &crate::database_schema::DatabaseSchemaSnapshot,
) -> Vec<Diagnostic> {
    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&module);
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    type_check_syntax_module_output_with_database_schema(&module, &resolved, Some(database_schema))
}

/// Typechecks source with checked-in std interfaces loaded.
///
/// Inputs:
/// - `source`: Terlan source text to parse and typecheck.
/// - `std_relative_path`: repository-relative std source path whose existence
///   anchors the fixture to a shipped contract.
///
/// Output:
/// - Diagnostics produced by formal syntax-output typechecking.
///
/// Transformation:
/// - Loads the fixture's direct std interfaces and manifest dependency closure,
///   then resolves and typechecks against that minimal checked-in graph.
pub(super) fn check_syntax_output_with_std_interfaces(
    source: &str,
    std_relative_path: &str,
) -> Vec<Diagnostic> {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(std_relative_path);
    assert!(
        fixture_path.is_file(),
        "std interface fixture does not exist: {}",
        fixture_path.display()
    );
    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&module);
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
    check_syntax_output_with_interfaces(source, &[interface_source])
}

/// Typechecks source against multiple ad-hoc provider interfaces.
///
/// Inputs:
/// - `source`: Terlan source text to parse and typecheck.
/// - `interface_sources`: provider modules parsed as interface modules.
///
/// Output:
/// - Diagnostics produced by formal syntax-output typechecking.
///
/// Transformation:
/// - Builds one complete interface graph before resolving the consumer, which
///   lets tests exercise signatures whose types come from another provider.
pub(super) fn check_syntax_output_with_interfaces(
    source: &str,
    interface_sources: &[&str],
) -> Vec<Diagnostic> {
    let mut interfaces = HashMap::new();
    for interface_source in interface_sources {
        let interface_module = parse_interface_module_as_syntax_output(interface_source)
            .unwrap_or_else(|err| panic!("failed to parse syntax interface fixture: {:?}", err));
        interfaces.insert(
            interface_module.module_name.clone(),
            syntax_module_output_to_interface(&interface_module),
        );
    }

    let module = parse_module_as_syntax_output(source)
        .unwrap_or_else(|err| panic!("failed to parse syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    type_check_syntax_module_output(&module, &resolved)
}
