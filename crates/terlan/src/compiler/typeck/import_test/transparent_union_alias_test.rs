/// Verifies provider-qualified struct fields remain assignable to a selected
/// transparent union alias from the same provider.
///
/// Inputs:
/// - A provider interface with a struct field typed as a transparent union and
///   another provider exporting a colliding nominal short name.
/// - A consumer that imports both the struct and union, then passes the field
///   to a local function expecting the selected union alias.
///
/// Output:
/// - No type diagnostics.
///
/// Transformation:
/// - Locks provider-local nominal members in imported alias bodies to their
///   qualified identity so field inference and local call inference agree.
#[test]
fn syntax_output_accepts_imported_transparent_union_struct_field_as_selected_alias() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Accepted = Atom[\"accepted\"].\n\
pub struct OutOfOrder {\n\
    expected: Int\n\
}.\n\
pub type Outcome = Accepted | OutOfOrder.\n\
pub struct Result {\n\
    outcome: Outcome\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let colliding_provider = parse_interface_module_as_syntax_output(
        "\
module other_provider.\n\
\n\
pub struct OutOfOrder {\n\
    marker: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse colliding interface fixture: {:?}", err));
    interfaces.insert(
        colliding_provider.module_name.clone(),
        syntax_module_output_to_interface(&colliding_provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import type provider.{Outcome, Result}.\n\
\n\
pub accepts(outcome: Outcome): Bool ->\n\
    outcome == outcome.\n\
\n\
pub inspect(result: Result): Bool ->\n\
    accepts(result.outcome).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
