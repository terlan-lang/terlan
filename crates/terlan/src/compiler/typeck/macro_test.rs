use super::test_support::*;
use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_checks_macro_expr_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_macro_expr.\n\
pub module_name(): Dynamic ->\n\
    ?MODULE.\n\
pub compare(a: Int, b: Int): Dynamic ->\n\
    ?assert_equal(a, b).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_macro_expr_with_declared_return_type() {
    let diagnostics = check_syntax_output(
        "\
module syntax_macro_return_type.
pub macro to_bool(X: Int): Ast[Bool] ->
    quote X.

pub bad(X: Int): Int ->
    ?to_bool(X).
",
    );

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("expected Int") && diag.message.contains("found Bool")
        ),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_config_declaration_placeholders() {
    let diagnostics = check_syntax_output(
        "\
	module syntax_config_declaration_placeholders.
target erlang.
machine linux.
static site.
	",
    );

    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("unsupported raw declaration kind")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn expands_syntax_raw_macros_no_ops_without_raw_macros() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_raw_macro_expansion_ok.\n\
    pub query(): Dynamic ->\n    42.\n\
",
    )
    .expect("parse syntax-output module");

    let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    assert_eq!(
        expanded, module,
        "non-macro modules must pass through unchanged"
    );
}

#[test]
fn syntax_output_checks_macro_signatures_on_formal_path() {
    let module = parse_module_as_syntax_output(
        "\
module bad_macro_return.\n\
pub macro bad(X: Int): Int ->\n\
    X.\n\
",
    )
    .expect("parse syntax output macro fixture");

    let diagnostics = check_syntax_macro_decl_signatures(&module);

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("macro `bad` must return Ast[T]")
                && diag.message.contains("found Int")
        ),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn expression_macros_expand_quote_unquote_before_runtime_lowering() {
    let module = parse_module_as_syntax_output(
        r#"
module expression_macro_expansion.
pub macro add_one(X: Expr): Ast[Int] ->
    quote (unquote(X) + 1).
pub result(): Int ->
    ?add_one(41).
"#,
    )
    .expect("parse expression macro fixture");

    let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let SyntaxDeclarationPayload::Function { clauses, .. } = &expanded.declarations[1].payload
    else {
        panic!("expected expanded result function");
    };
    assert_eq!(clauses[0].body.kind, SyntaxExprKind::BinaryOp);
    assert_eq!(clauses[0].body.operator.as_deref(), Some("+"));
    assert_eq!(clauses[0].body.children[0].text.as_deref(), Some("41"));

    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    assert!(core
        .functions
        .iter()
        .all(|function| function.name != "add_one"));
    assert!(core
        .functions
        .iter()
        .any(|function| function.name == "result"));
}

#[test]
fn expression_macro_introduced_bindings_are_hygienic() {
    let module = parse_module_as_syntax_output(
        r#"
module expression_macro_hygiene.
pub macro with_local(X: Expr): Ast[Int] ->
    quote (let value = 1; unquote(X) + value).
pub result(value: Int): Int ->
    ?with_local(value).
"#,
    )
    .expect("parse hygienic macro fixture");

    let (expanded, diagnostics) = expand_syntax_raw_macros(module);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let SyntaxDeclarationPayload::Function { clauses, .. } = &expanded.declarations[1].payload
    else {
        panic!("expected expanded result function");
    };
    let body = &clauses[0].body;
    assert_eq!(body.kind, SyntaxExprKind::Let);
    let introduced = body.patterns[0]
        .text
        .as_deref()
        .expect("introduced binding name");
    assert!(introduced.starts_with("__macro_1_value"), "{introduced}");
    let result = body.children.last().expect("let result expression");
    assert_eq!(result.children[0].text.as_deref(), Some("value"));
    assert_eq!(result.children[1].text.as_deref(), Some(introduced));
}

#[test]
fn expression_macro_rejects_non_parameter_unquote_stably() {
    let module = parse_module_as_syntax_output(
        r#"
module expression_macro_bad_unquote.
pub macro invalid(X: Expr): Ast[Int] ->
    quote unquote(MISSING).
pub result(): Int ->
    ?invalid(1).
"#,
    )
    .expect("parse invalid macro fixture");

    let (_, diagnostics) = expand_syntax_raw_macros(module);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unquote` that is not one of its syntax parameters")));
}

#[test]
fn public_expression_macro_round_trips_and_expands_through_an_import_alias() {
    let provider = parse_module_as_syntax_output(
        r#"
module provider.macros.
pub macro increment(X: Expr): Ast[Int] ->
    quote (unquote(X) + 1).
"#,
    )
    .expect("parse macro provider");
    let interface = crate::terlan_hir::syntax_module_output_to_interface(&provider);
    assert!(interface
        .expression_macros
        .contains_key(&("increment".to_string(), 1)));
    let interface_text = interface.to_terlan_interface_text();
    let reparsed = crate::terlan_syntax::parse_interface_module_as_syntax_output(&interface_text)
        .expect("macro interface round trip");
    let reparsed_interface = crate::terlan_hir::syntax_module_output_to_interface(&reparsed);
    assert!(
        reparsed_interface
            .expression_macros
            .contains_key(&("increment".to_string(), 1)),
        "interface={interface_text}\nmacros={:?}",
        reparsed_interface
            .expression_macros
            .keys()
            .collect::<Vec<_>>()
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module consumer.macros.
import provider.macros.{increment as bump}.
pub result(): Int ->
    ?bump(2).
"#,
    )
    .expect("parse macro consumer");
    let SyntaxDeclarationPayload::Import {
        module_name,
        items,
        is_selected,
        ..
    } = &consumer.declarations[0].payload
    else {
        panic!("expected selected macro import");
    };
    assert_eq!(module_name, "provider.macros");
    assert!(*is_selected, "items={items:?}");
    assert_eq!(items[0].name, "increment", "items={items:?}");
    assert_eq!(
        items[0].as_alias.as_deref(),
        Some("bump"),
        "items={items:?}"
    );
    let interfaces =
        std::collections::HashMap::from([("provider.macros".to_string(), reparsed_interface)]);
    let (expanded, diagnostics) = expand_syntax_macros_with_interfaces(consumer, &interfaces);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let SyntaxDeclarationPayload::Function { clauses, .. } = &expanded.declarations[1].payload
    else {
        panic!("expected consumer result function");
    };
    assert_eq!(clauses[0].body.kind, SyntaxExprKind::BinaryOp);
    assert_eq!(clauses[0].body.children[0].text.as_deref(), Some("2"));
}

#[test]
fn expression_macro_limits_recursion_and_reports_wrong_arity_stably() {
    let recursive = parse_module_as_syntax_output(
        r#"
module expression_macro_recursion.
pub macro forever(X: Expr): Ast[Int] ->
    quote ?forever(unquote(X)).
pub result(): Int ->
    ?forever(1).
"#,
    )
    .expect("parse recursive macro fixture");
    let (_, diagnostics) = expand_syntax_raw_macros(recursive);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("macro expansion exceeded recursion depth limit 64")));

    let wrong_arity = parse_module_as_syntax_output(
        r#"
module expression_macro_wrong_arity.
pub macro one(X: Expr): Ast[Int] -> quote unquote(X).
pub result(): Int -> ?one(1, 2).
"#,
    )
    .expect("parse wrong-arity macro fixture");
    let (_, diagnostics) = expand_syntax_raw_macros(wrong_arity);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
        == "wrong arity for macro `one`: expected one of [1], found 2"));
}

#[test]
fn expression_macro_evaluator_is_capability_free_and_caps_diagnostics() {
    let effectful = parse_module_as_syntax_output(
        r#"
module expression_macro_effectful.
pub macro invalid(X: Expr): Ast[Int] -> X.
pub result(): Int -> ?invalid(1).
"#,
    )
    .expect("parse effectful macro fixture");
    let (_, diagnostics) = expand_syntax_raw_macros(effectful);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("macro `invalid` must return one `quote` expression")));

    let mut source = String::from(
        "module expression_macro_diagnostic_limit.\n\
         pub macro one(X: Expr): Ast[Int] -> quote unquote(X).\n\
         pub result(): Dynamic -> [",
    );
    for index in 0..140 {
        if index > 0 {
            source.push_str(", ");
        }
        source.push_str("?one(1, 2)");
    }
    source.push_str("].\n");
    let module = parse_module_as_syntax_output(&source).expect("parse diagnostic-limit fixture");
    let (_, diagnostics) = expand_syntax_raw_macros(module);
    assert_eq!(diagnostics.len(), 128);
    assert_eq!(
        diagnostics
            .last()
            .map(|diagnostic| diagnostic.message.as_str()),
        Some("macro expansion diagnostic limit 128 exceeded")
    );
}

#[test]
fn public_expression_macro_template_changes_interface_fingerprint() {
    let interface_for = |increment: i64| {
        let source = format!(
            "module expression_macro_fingerprint.\n\
             pub macro increment(X: Expr): Ast[Int] ->\n\
                 quote (unquote(X) + {increment}).\n"
        );
        let module =
            parse_module_as_syntax_output(&source).expect("parse macro fingerprint fixture");
        crate::terlan_hir::syntax_module_output_to_interface(&module)
    };
    let first = interface_for(1);
    let second = interface_for(2);
    let key = ("increment".to_string(), 1);
    assert_ne!(
        first.expression_macros[&key].fingerprint,
        second.expression_macros[&key].fingerprint
    );
    assert_ne!(
        first.to_terlan_interface_type_text(),
        second.to_terlan_interface_type_text()
    );
}
