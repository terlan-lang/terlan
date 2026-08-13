use std::collections::HashMap;

use super::super::*;
use super::public_function_interface;
use crate::terlan_hir::resolve_syntax_module_output_with_interfaces;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies struct field context resolves imported module-member function values.
///
/// Inputs:

/// - A provider interface for `provider.Users` with overloaded public
///   `index` functions.
/// - A consumer config struct whose `list` field expects `(Int) -> Int`.
///
/// Output:
/// - Test passes when `#Actions { list = Users.index }` selects the unary
///   overload from the field's declared function type.
///
/// Transformation:
/// - Exercises the route/resource configuration shape required by 0.0.5
///   without adding framework-specific syntax.
#[test]
fn syntax_output_resolves_imported_module_member_function_value_from_struct_field_context() {
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Users".to_string(),
        public_function_interface(
            "provider.Users",
            &[
                ("index", vec![("value", "Int")], "Int"),
                ("index", vec![("value", "Int"), ("step", "Int")], "Int"),
            ],
        ),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import provider.Users.\n\
\n\
pub type Indexer = (Int) -> Int.\n\
\n\
pub struct Actions {\n\
  list: Indexer\n\
}.\n\
\n\
pub actions(): Actions ->\n\
    Actions { list: Users.index }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
