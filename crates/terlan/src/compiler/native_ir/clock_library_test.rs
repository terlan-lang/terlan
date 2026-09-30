use super::source_constructor_test::check_sources;

#[test]
fn clock_names_do_not_override_source_bodies() {
    let provider = r#"
module std.time.Clock.
pub unix_time_ns(): Int -> 41.
pub monotonic_time_ns(): Int -> 42.
"#;
    let caller = r#"
module clock_source_authority.
import std.time.Clock.
pub check(): Bool -> Clock.unix_time_ns() == 41 and Clock.monotonic_time_ns() == 42.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.time.Clock", "app.Clock"),
        &provider.replace("std.time.Clock", "app.Clock"),
    ]);
}

#[test]
fn package_clock_observations_cannot_be_asserted_pure_or_used_in_pure_guards() {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::type_check_syntax_module_output;

    let syntax = parse_module_as_syntax_output(
        r#"
module clock_effects.
import std.time.Clock.
@pure
pub wall(): Int -> Clock.unix_time_ns().
@pure
pub monotonic(): Int -> Clock.monotonic_time_ns().
pub guard(value: Int): Int ->
    case value {
        n where Clock.monotonic_time_ns() >= 0 -> n;
        _ -> 0
    }.
"#,
    )
    .unwrap();
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    for expected in [
        "function wall annotated @pure must be pure",
        "function monotonic annotated @pure must be pure",
        "case guard must be pure",
    ] {
        assert!(
            diagnostics.iter().any(|d| d.message.contains(expected)),
            "{diagnostics:?}"
        );
    }
}
