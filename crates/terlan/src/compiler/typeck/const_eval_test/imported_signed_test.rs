use super::*;

fn signed_interfaces() -> HashMap<String, crate::terlan_hir::ModuleInterface> {
    let provider = parse_module_as_syntax_output(
        r#"
module lifecycle.signed.
pub type OrderSide: Int = BUY = 1 | SELL = -1.
pub type TimeInForce: Int = DAY = 1 | GOOD_TIL_CANCELLED = 3.
pub const NEGATIVE: Int = -7.
pub const NEGATIVE_LIMIT: Int = -9223372036854775807.
pub (current: OrderSide) matches(other: OrderSide): Bool -> current == other.
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
{import_prefix} lifecycle.signed.{{OrderSide as Side, TimeInForce}}.
import lifecycle.signed.{{NEGATIVE, NEGATIVE_LIMIT}}.
pub sample(): {{Side, Side, TimeInForce, Int, Int}} ->
    {{Side.BUY, Side.SELL, TimeInForce.DAY, NEGATIVE, NEGATIVE_LIMIT}}.
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
            Some("const_union:lifecycle.signed.OrderSide.SELL")
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
import type lifecycle.signed.{OrderSide, TimeInForce}.
pub invalid(): TimeInForce -> OrderSide.SELL.
"#,
    )
    .expect("parse wrong union consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("OrderSide") && diagnostic.message.contains("TimeInForce")
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
    for receiver in ["Side.SELL", "(Side.SELL)", "lifecycle.signed.OrderSide.BUY"] {
        let source = format!(
            "module lifecycle.receiver_consumer.\n\
             import lifecycle.signed.{{matches}}.\n\
             import type lifecycle.signed.{{OrderSide as Side}}.\n\
             pub sample(): Bool -> {receiver}.matches(Side.SELL).\n"
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
            assert!(!rendered.contains("module: \"Side.SELL\""), "{rendered}");
            assert!(
                !rendered.contains("module: \"lifecycle.signed.OrderSide.BUY\""),
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
        "TimeInForce.DAY.matches(Side.SELL)",
        "Side.SELL.matches(TimeInForce.DAY)",
    ] {
        let consumer = parse_module_as_syntax_output(&format!(
            "module lifecycle.receiver_wrong_type.\n\
             import lifecycle.signed.{{matches}}.\n\
             import type lifecycle.signed.{{OrderSide as Side, TimeInForce}}.\n\
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
