use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Verifies qualified records returned through another module remain readable.
///
/// Inputs:
/// - A types interface exporting `CameraPose`.
/// - An API interface returning `types.CameraPose` without re-exporting its
///   type under a consumer-local alias.
/// - A consumer importing both modules and reading a field from `Api.pose()`.
///
/// Output:
/// - Test passes when field inference resolves the qualified nominal receiver
///   through the types interface and preserves its `Float` field type.
///
/// Transformation:
/// - Reproduces generated native-package signatures whose operation module
///   returns a copied value record owned by a separate types module.
#[test]
fn syntax_output_accepts_field_access_on_qualified_cross_module_return() {
    let types = parse_interface_module_as_syntax_output(
        "\
module provider.Types.\n\
\n\
pub struct CameraPose {\n\
    tz: Float\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse types interface fixture: {:?}", err));
    let api = parse_interface_module_as_syntax_output(
        "\
module provider.Api.\n\
\n\
pub pose(): provider.Types.CameraPose.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse API interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        types.module_name.clone(),
        syntax_module_output_to_interface(&types),
    );
    interfaces.insert(
        api.module_name.clone(),
        syntax_module_output_to_interface(&api),
    );
    let module = parse_module_as_syntax_output(
        "\
module qualified_record_consumer.\n\
\n\
import provider.Api.\n\
import provider.Types.\n\
\n\
pub depth(): Float ->\n\
    Api.pose().tz.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
