use super::*;

fn signed_interfaces() -> HashMap<String, crate::terlan_hir::ModuleInterface> {
    let provider = parse_module_as_syntax_output(
        r#"
module lifecycle.signed.
pub type Direction: Int = FORWARD = 1 | BACKWARD = -1.
pub type Mode: Int = FAST = 1 | SAFE = 3.
pub const NEGATIVE: Int = -7.
pub const NEGATIVE_LIMIT: Int = -9223372036854775807.
pub (current: Direction) matches(other: Direction): Bool -> current == other.
"#,
    )
    .expect("parse signed provider");
    let summary = syntax_module_output_to_interface(&provider).to_terlan_interface_text();
    let (name, interface) =
        crate::terlan_hir::parse_interface_text(&summary).expect("reload generated summary");
    HashMap::from([(name, interface)])
}

/// Generated summaries must preserve signed arms, selected aliases and bounds.
#[test]
fn imported_signed_values_survive_interface_round_trip() {
    let interfaces = signed_interfaces();
    for import_prefix in ["import", "import type"] {
        let consumer = parse_module_as_syntax_output(&format!(
            r#"
module lifecycle.signed_consumer.
{import_prefix} lifecycle.signed.{{Direction as Heading, Mode}}.
import lifecycle.signed.{{NEGATIVE, NEGATIVE_LIMIT}}.
pub sample(): {{Heading, Heading, Mode, Int, Int}} ->
    {{Heading.FORWARD, Heading.BACKWARD, Mode.FAST, NEGATIVE, NEGATIVE_LIMIT}}.
"#,
        ))
        .expect("parse signed consumer");
        let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
        assert!(
            resolved.diagnostics.is_empty(),
            "{:#?}",
            resolved.diagnostics
        );
        let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
        assert!(diagnostics.is_empty(), "{import_prefix}: {diagnostics:#?}");
        let (prepared, diagnostics) =
            super::super::prepare_syntax_constants_with_interfaces(&consumer, &interfaces);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        let body = prepared
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Function { name, clauses, .. } if name == "sample" => {
                    Some(&clauses[0].body)
                }
                _ => None,
            })
            .expect("prepared sample body");
        assert_eq!(
            body.children
                .iter()
                .map(|child| child.text.as_deref())
                .collect::<Vec<_>>(),
            vec![
                Some("1"),
                Some("-1"),
                Some("1"),
                Some("-7"),
                Some("-9223372036854775807")
            ]
        );
        assert_eq!(
            body.children[1].raw.as_deref(),
            Some("const_union:lifecycle.signed.Direction.BACKWARD")
        );
    }
}

/// Decoding an imported arm must retain its provider's nominal union identity.
#[test]
fn imported_signed_union_still_rejects_wrong_nominal_type() {
    let interfaces = signed_interfaces();
    let consumer = parse_module_as_syntax_output(
        r#"
module lifecycle.signed_wrong_type.
import type lifecycle.signed.{Direction, Mode}.
pub invalid(): Mode -> Direction.BACKWARD.
"#,
    )
    .expect("parse wrong union consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("Direction") && diagnostic.message.contains("Mode")
        }),
        "{diagnostics:#?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("found Dynamic")),
        "{diagnostics:#?}"
    );
}

/// Parentheses and formatting must not turn constant receivers into modules.
#[test]
fn imported_signed_union_constant_receiver_calls_resolve() {
    let interfaces = signed_interfaces();
    for receiver in [
        "Heading.BACKWARD",
        "(Heading.BACKWARD)",
        "lifecycle.signed.Direction.FORWARD",
    ] {
        let source = format!(
            "module lifecycle.receiver_consumer.\n\
             import lifecycle.signed.{{matches}}.\n\
             import type lifecycle.signed.{{Direction as Heading}}.\n\
             pub sample(): Bool -> {receiver}.matches(Heading.BACKWARD).\n"
        );
        for source in [
            source.clone(),
            format_source_module(&source).expect("format receiver"),
        ] {
            let consumer = parse_module_as_syntax_output(&source).expect("parse constant receiver");
            let resolved =
                resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
            let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:#?}");
            let core = lower_syntax_module_output_to_core(&consumer, &resolved);
            let rendered = format!("{core:#?}");
            assert!(
                !rendered.contains("module: \"Heading.BACKWARD\""),
                "{rendered}"
            );
            assert!(
                !rendered.contains("module: \"lifecycle.signed.Direction.FORWARD\""),
                "{rendered}"
            );
        }
    }
}

/// Reclassification must still check the method's receiver and argument types.
#[test]
fn imported_signed_union_receiver_rejects_wrong_union() {
    let interfaces = signed_interfaces();
    for call in [
        "Mode.FAST.matches(Heading.BACKWARD)",
        "Heading.BACKWARD.matches(Mode.FAST)",
    ] {
        let consumer = parse_module_as_syntax_output(&format!(
            "module lifecycle.receiver_wrong_type.\n\
             import lifecycle.signed.{{matches}}.\n\
             import type lifecycle.signed.{{Direction as Heading, Mode}}.\n\
             pub invalid(): Bool -> {call}.\n"
        ))
        .expect("parse invalid constant receiver");
        let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
        assert!(
            !diagnostics.is_empty(),
            "wrong nominal call accepted: {call}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("cannot resolve module")),
            "{diagnostics:#?}"
        );
    }
}
