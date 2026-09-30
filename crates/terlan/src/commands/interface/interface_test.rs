use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn interface_retains_included_public_fields_and_private_visibility() {
    let parent = parse_module_as_syntax_output(
        "module base.Errors. pub struct Error { code: Atom, message: String }.",
    )
    .unwrap();
    let interfaces = HashMap::from([(
        "base.Errors".into(),
        syntax_module_output_to_interface(&parent),
    )]);
    let child = parse_module_as_syntax_output(
        "module package.Parser. import type base.Errors.Error. pub struct ParseError includes Error { offset: Int, #source: String }.",
    ).unwrap();
    let interface = expanded_interface(&child, &interfaces).unwrap();
    let fields = &interface.struct_fields["ParseError"];
    assert_eq!(
        fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["code", "message", "offset", "source"]
    );
    assert!(fields[3].is_private);
    let rendered = interface.to_terlan_interface_text();
    let reparsed = parse_module_as_syntax_output(&rendered).unwrap();
    assert_eq!(
        syntax_module_output_to_interface(&reparsed).struct_fields,
        interface.struct_fields
    );
}

#[test]
fn interface_rejects_conflicting_or_unknown_included_fields() {
    for source in [
        "module bad. pub struct Base { code: Atom }. pub struct Child includes Base { code: String }.",
        "module bad. pub struct Child includes Missing { offset: Int }.",
    ] {
        let syntax = parse_module_as_syntax_output(source).unwrap();
        let diagnostics = expanded_interface(&syntax, &HashMap::new()).unwrap_err();
        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().all(|diagnostic| matches!(diagnostic.severity, crate::terlan_typeck::DiagSeverity::Error)));
        assert!(diagnostics.iter().all(|diagnostic| !diagnostic.message.is_empty()));
    }
}
