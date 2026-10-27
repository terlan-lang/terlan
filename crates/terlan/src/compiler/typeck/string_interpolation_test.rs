fn diagnostics(source: &str) -> Vec<super::Diagnostic> {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    use crate::terlan_syntax::parse_module_as_syntax_output;
    let module = parse_module_as_syntax_output(source).unwrap();
    let provider =
        parse_module_as_syntax_output("module interpolation_dependencies. import std.core.String.")
            .unwrap();
    // The production loader discovers the qualified calls introduced by
    // interpolation; this unit harness supplies their standard interface.
    let mut interfaces = checked_in_std_interfaces_for_module(&provider);
    interfaces.extend(checked_in_std_interfaces_for_module(&module));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    super::type_check_syntax_module_output(&module, &resolved)
}

#[test]
fn string_interpolation_needs_no_explicit_show_import() {
    let errors = diagnostics(
        r#"module greeting.
import std.io.Console.{println}.
pub main(): Unit -> println("answer: ${42}").
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn string_interpolation_bound_matching_preserves_universal_implementations() {
    let errors = super::test_support::check_syntax_output(
        r#"module blanket.
pub trait Render[T] { render(value: T): String. }.
pub impl Render[T] for T { render(value: T): String -> "value". }.
render[T](value: T)[Render[T]]: String -> Render.render(value).
pub generic[V](value: V): String -> render(value).
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn string_interpolation_accepts_primitive_and_bound_generic_values() {
    let errors = diagnostics(
        r#"module interpolate.
import std.core.String.{Show}.
pub render[T](value: T)[Show[T]]: String -> "value: ${value}".
pub demo(): String -> "${42} ${true} ${render(7)}".
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn string_interpolation_requires_show_for_custom_and_unbound_generic_values() {
    for source in [
        r#"module missing. import std.core.String. struct Value { x: Int }. pub render(value: Value): String -> "${value}"."#,
        r#"module missing. import std.core.String. pub render[T](value: T): String -> "${value}"."#,
    ] {
        let errors = diagnostics(source);
        assert!(
            errors.iter().any(|e| e.message.contains("Show")),
            "{errors:?}"
        );
    }
}

#[test]
fn string_interpolation_honors_explicit_negative_show() {
    let errors = diagnostics(
        r#"module denied.
import std.core.Secret.{Secret}.
import std.core.String.{Show}.
pub render(value: Secret): String -> "${value}".
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("Show") && e.message.contains("denied")),
        "{errors:?}"
    );
}

#[test]
fn string_interpolation_uses_custom_show_implementation() {
    let errors = diagnostics(
        r#"module custom.
import std.core.String.{Show}.
struct Value { x: Int }.
impl Show[Value] for Value { to_string(value: Value): String -> "${value.x}". }.
pub render(value: Value): String -> "${value}".
"#,
    );
    assert!(errors.is_empty(), "{errors:?}");
}
