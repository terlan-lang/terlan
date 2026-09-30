//! Library linking must preserve provider behavior and lexical module identity.

use super::*;

#[test]
fn source_library_failures_preserve_phase_and_io_cause() {
    use std::error::Error;
    let root = tempfile::tempdir().unwrap();
    let provider = root.path().join("std/source/Fixture.terl");
    std::fs::create_dir_all(provider.parent().unwrap()).unwrap();
    let cores = checked_cores(&[
        "module source_failures. import std.source.Fixture. pub check(): Bool -> true.",
        "module std.source.Fixture. pub value(): Int -> 1.",
    ]);
    let path = root.path().join("std/consumer.terl");
    std::fs::write(&provider, [0xff]).unwrap();
    let error = link_libraries(&cores[0], &path).unwrap_err();
    assert!(
        matches!(&error, SourceLinkError::Read { source, .. } if source.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(error.source().unwrap().is::<std::io::Error>());
    assert!(error.to_string().contains("Fixture.terl"));

    std::fs::write(&provider, "not a Terlan module").unwrap();
    let error = link_libraries(&cores[0], &path).unwrap_err();
    assert!(
        matches!(&error, SourceLinkError::Compile { module } if module == "std.source.Fixture")
    );
    assert!(error.to_string().contains("cannot compile source library"));
    assert!(error.source().is_none());

    std::fs::write(&provider, "module wrong.Owner. pub value(): Int -> 1.").unwrap();
    let error = link_libraries(&cores[0], &path).unwrap_err();
    assert!(
        matches!(&error, SourceLinkError::ModuleMismatch { expected, actual } if expected == "std.source.Fixture" && actual == "wrong.Owner")
    );
    assert!(error.to_string().contains("declares `wrong.Owner`"));
    let error = SourceLinkError::Analysis("error[native_ir.test]: evidence".into());
    assert!(error
        .source()
        .unwrap()
        .is::<crate::compiler::native_ir::NativeIrError>());
    assert!(error.to_string().contains("evidence"));
}

#[test]
fn links_implicit_core_receivers_through_the_formal_pipeline() {
    let source = r#"
module source_library_smoke.
pub check(): Bool -> "".is_empty() and not "text".is_empty()
    and "a".append("b") == "ab" and "hello".to_string() == "hello".
"#;
    let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "source_library_smoke.terl",
        source,
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::JsShared,
    )
    .expect("compile source module");
    let linked = link_libraries(&compiled.core, Path::new("source_library_smoke.terl"))
        .expect("link real library");
    assert_js_check(&linked);
}

fn assert_js_check(linked: &CoreModule) -> String {
    let js = crate::commands::emit_js::direct_ast::emit_core_module_with_direct_oxc_ast(linked)
        .unwrap_or_else(|| panic!("unsupported linked source: {}", linked.contract_text()));
    let output = std::process::Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            &format!("{js}\nif (!check()) process.exit(1);"),
        ])
        .output()
        .expect("execute linked module");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    js
}

#[test]
fn links_changed_library_bodies_without_name_substitution() {
    let sources = [
        r#"
module consumer.
import std.core.String.
import std.core.String.{append as join, is_empty as empty}.
pub check(): Bool -> "a".append("b") == "ba" and "text".is_empty()
    and join("a", "b") == "ba" and empty("text").
"#,
        r#"
module std.core.String.
pub (left: String) append(right: String): String -> combine(right, left).
pub (value: String) is_empty(): Bool -> value != "".
combine(left: String, right: String): String -> left + right.
pub unused(): Int -> 123.
"#,
    ];
    let linked = link_cores(checked_cores(&sources)).expect("link source bodies");
    assert_eq!(linked.functions.len(), 4);
    let js = assert_js_check(&linked);
    assert!(js.contains("combine$2(right, left)"), "{js}");
    assert!(js.contains("return value !=="), "{js}");
    assert!(!js.contains("unused"), "{js}");
}

#[test]
fn rejects_colliding_javascript_exports_instead_of_overwriting_a_body() {
    let cores = checked_cores(&[
        "module overloaded. pub value(): Int -> 1. pub value(input: Int): Int -> input.",
    ]);
    let error = link_cores(cores).expect_err("JavaScript cannot export two values under one name");
    assert!(
        matches!(&error, SourceLinkError::DuplicateSymbol { function, .. } if function == "overloaded.value")
    );
    assert!(
        error.to_string().contains("unique function identity"),
        "{error}"
    );
}

fn checked_cores(sources: &[&str]) -> Vec<CoreModule> {
    let syntaxes = sources
        .iter()
        .map(|source| {
            crate::terlan_syntax::parse_module_as_syntax_output(source).expect("parse provider")
        })
        .collect::<Vec<_>>();
    let interfaces = syntaxes
        .iter()
        .map(|syntax| {
            let interface = crate::terlan_hir::syntax_module_output_to_interface(syntax);
            (interface.module.clone(), interface)
        })
        .collect();
    syntaxes
        .iter()
        .map(|syntax| {
            let resolved = crate::terlan_hir::resolve_syntax_module_output_with_interfaces(
                syntax,
                &interfaces,
            )
            .module;
            let diagnostics =
                crate::terlan_typeck::type_check_syntax_module_output(syntax, &resolved);
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            crate::terlan_typeck::lower_syntax_module_output_to_core(syntax, &resolved)
        })
        .collect()
}
