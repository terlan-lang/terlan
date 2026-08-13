use super::*;

#[test]
fn headerless_script_lowers_declarations_and_top_level_body_to_main() {
    let source = r#"import std.io.Console.{println}.

increment(value: Int): Int ->
    value + 1.

let answer = increment(41);
println("done").
"#;

    let module = parse_script(source, "scripts.Build").expect("parse script");

    assert_eq!(module.name, "scripts.Build");
    assert!(module.declarations.iter().any(|declaration| {
        matches!(declaration, Decl::Import(import) if import.module_name == "std.io.Console")
    }));
    assert!(module.declarations.iter().any(|declaration| {
        matches!(declaration, Decl::Function(function) if function.name == "increment")
    }));
    let Some(Decl::Function(main)) = module.declarations.iter().find(
        |declaration| matches!(declaration, Decl::Function(function) if function.name == "main"),
    ) else {
        panic!("expected synthetic main");
    };
    assert_eq!(main.name, "main");
    assert!(main.is_public);
    assert!(main.params.is_empty());
    assert_eq!(main.return_type.text, "Dynamic");
    assert_eq!(main.clauses.len(), 1);
    assert_eq!(
        main.clauses[0].span.start,
        source.find("let answer").unwrap()
    );
}

#[test]
fn script_shebang_preserves_user_expression_offsets() {
    let source = "#!/usr/bin/env terlc\nlet value = 1;\nUnit.\n";
    let module = parse_script(source, "script.Offset").expect("parse shebang script");
    let Decl::Function(main) = module.declarations.last().expect("script main") else {
        panic!("expected synthetic main");
    };

    assert_eq!(
        main.clauses[0].span.start,
        source.find("let value").unwrap()
    );
}

#[test]
fn script_rejects_explicit_main_instead_of_selecting_between_entries() {
    let source = "pub main(): Unit -> Unit.\n\nUnit.\n";
    let error = parse_script(source, "script.Duplicate").expect_err("duplicate entry must fail");

    assert!(error.message.contains("cannot define `main`"), "{error:?}");
}

#[test]
fn script_rejects_explicit_module_header() {
    let source = "module scripts.Wrong.\n\nUnit.\n";
    let error = parse_script(source, "scripts.Right").expect_err("module header must fail");

    assert!(
        error.message.contains("cannot declare `module`"),
        "{error:?}"
    );
}

#[test]
fn script_requires_a_top_level_executable_expression() {
    let source = "helper(): Unit -> Unit.\n";
    let error = parse_script(source, "script.Empty").expect_err("bodyless script must fail");

    assert!(error
        .message
        .contains("missing its top-level executable expression"));
}

#[test]
fn ordinary_modules_keep_rejecting_headerless_script_source() {
    let source = "let value = 1;\nUnit.\n";

    assert!(parse_module(source).is_err());
}

#[test]
fn script_allows_top_level_binding_without_let() {
    let source = "answer = 40 + 2;\nanswer.\n";
    let module = parse_script(source, "script.Binding").expect("parse implicit script binding");
    let Decl::Function(main) = module.declarations.last().expect("script main") else {
        panic!("expected synthetic main");
    };

    let Expr::Let { bindings, body, .. } = &main.clauses[0].body else {
        panic!("expected script binding to lower as immutable let");
    };
    assert!(matches!(bindings[0].pattern, Pattern::Var(ref name) if name == "answer"));
    assert!(matches!(body.as_deref(), Some(Expr::Var(name)) if name == "answer"));
}

#[test]
fn script_final_expression_remains_the_entrypoint_result() {
    let source = "40 + 2.\n";
    let module = parse_script(source, "script.Result").expect("parse result script");
    let Decl::Function(main) = module.declarations.last().expect("script main") else {
        panic!("expected synthetic main");
    };

    assert_eq!(main.return_type.text, "Dynamic");
    assert!(matches!(main.clauses[0].body, Expr::BinaryOp { .. }));
}

#[test]
fn script_inline_assertion_guards_the_remaining_top_level_sequence() {
    let source = "value = 42;\nassert_equal(value, 42);\nvalue.\n";
    let module = parse_script(source, "script.Assert").expect("parse inline assertion");
    let Some(Decl::Function(main)) = module.declarations.iter().find(
        |declaration| matches!(declaration, Decl::Function(function) if function.name == "main"),
    ) else {
        panic!("expected synthetic main");
    };
    let Expr::Let { body, .. } = &main.clauses[0].body else {
        panic!("expected leading implicit binding");
    };

    assert!(matches!(body.as_deref(), Some(Expr::If { .. })));
    assert!(module.declarations.iter().any(|declaration| {
        matches!(declaration, Decl::Import(import) if import.module_name == "std.test.Test")
    }));
}
