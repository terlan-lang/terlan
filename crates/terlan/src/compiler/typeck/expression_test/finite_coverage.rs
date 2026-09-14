//! Finite constructor payloads must be checked jointly, without exponential expansion.

use super::*;

fn option_case(branches: &str) -> Vec<Diagnostic> {
    check_syntax_output_with_std_interfaces(
        &format!("module fixture.Coverage.\nimport std.core.Option.{{None, Some}}.\nimport type std.core.Option.\npub value(flag: Option[Bool]): Bool -> case flag {{ {branches} }}.\n"),
        "std/core/Option.terl",
    )
}

/// Both Boolean payloads together cover Some, independently of clause order.
#[test]
fn nested_boolean_constructor_cases_are_exhaustive() {
    for branches in [
        "Some(true) -> true; Some(false) -> false; None -> false",
        "None -> false; Some(false) -> false; Some(true) -> true",
        "Some(_) -> true; None -> false",
    ] {
        let diagnostics = option_case(branches);
        assert!(diagnostics.is_empty(), "{branches}: {diagnostics:?}");
    }
}

/// Missing, repeated, and guarded alternatives cannot certify complete coverage.
#[test]
fn nested_boolean_constructor_cases_reject_incomplete_coverage() {
    for branches in [
        "Some(true) -> true; None -> false",
        "Some(false) -> false; None -> false",
        "Some(true) -> true; Some(true) -> false; None -> false",
        "Some(true) -> true; Some(false) where false -> false; None -> false",
        "Some(true) -> true; Some(false) -> false",
    ] {
        let diagnostics = option_case(branches);
        assert!(
            diagnostics
                .iter()
                .any(|error| error.message.contains("non-exhaustive case")),
            "{branches}: {diagnostics:?}"
        );
    }
}

/// Multiple tested finite fields require every combination, not merely every column.
#[test]
fn boolean_tuple_coverage_tracks_combinations() {
    for (branches, complete) in [
        (
            "{true, _} -> true; {false, true} -> true; {false, false} -> false",
            true,
        ),
        ("{true, true} -> true; {false, false} -> false", false),
    ] {
        let diagnostics = check_syntax_output(&format!("module fixture.TupleCoverage.\npub value(flags: {{Bool, Bool}}): Bool -> case flags {{ {branches} }}.\n"));
        assert_eq!(diagnostics.is_empty(), complete, "{diagnostics:?}");
    }
}

/// Alias spelling cannot make a reachable finite tuple branch appear disjoint.
#[test]
fn boolean_and_atom_alias_tuple_coverage_uses_resolved_atoms() {
    let branches = [
        "{false, BigEndian} -> 0",
        "{false, LittleEndian} -> 1",
        "{true, BigEndian} -> 2",
        "{true, LittleEndian} -> 3",
    ];
    for omitted in 0..=branches.len() {
        let selected = branches
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != omitted)
            .map(|(_, branch)| *branch)
            .collect::<Vec<_>>()
            .join("; ");
        let source = format!(
            "module fixture.AliasTupleCoverage.\n\
             pub type BigEndian = Atom[\"big\"].\n\
             pub type LittleEndian = Atom[\"little\"].\n\
             pub type EndianPolicy = BigEndian | LittleEndian.\n\
             pub value(flag: Bool, endian: EndianPolicy): Int ->\n\
             case {{flag, endian}} {{ {selected} }}.\n"
        );
        let diagnostics = check_syntax_output(&source);
        if omitted == branches.len() {
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
        } else {
            assert!(
                diagnostics
                    .iter()
                    .any(|error| error.message.contains("non-exhaustive case")),
                "omitted branch {omitted}: {diagnostics:?}"
            );
        }
    }
}

/// Refinement only visits tested coordinates of large finite products.
#[test]
fn wide_boolean_tuple_does_not_expand_its_cartesian_product() {
    let types = vec!["Bool"; 40].join(", ");
    let wildcards = vec!["_"; 39].join(", ");
    let source = format!("module fixture.WideCoverage.\npub value(flags: {{{types}}}): Bool -> case flags {{ {{true, {wildcards}}} -> true; {{false, {wildcards}}} -> false }}.\n");
    let diagnostics = check_syntax_output(&source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let all_true = vec!["true"; 40].join(", ");
    let source = format!("module fixture.WideLiteralCoverage.\npub value(flags: {{{types}}}): Bool -> case flags {{ {{{all_true}}} -> true; _ -> false }}.\n");
    let diagnostics = check_syntax_output(&source);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Finite unions nested inside another constructor retain their full payload coverage.
#[test]
fn doubly_nested_boolean_constructor_cases_are_exhaustive() {
    let source = "module fixture.NestedCoverage.\nimport std.core.Option.{None, Some}.\nimport type std.core.Option.\npub value(flag: Option[Option[Bool]]): Bool -> case flag { None -> false; Some(None) -> false; Some(Some(true)) -> true; Some(Some(false)) -> false }.\n";
    let diagnostics = check_syntax_output_with_std_interfaces(source, "std/core/Option.terl");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Constructed nested aliases must inhabit the same type the matcher accepts.
#[test]
fn nested_boolean_constructor_values_can_be_passed_to_typed_functions() {
    let source = "module fixture.NestedCalls.\nimport std.core.Option.{None, Some}.\nimport type std.core.Option.\npub value(flag: Option[Option[Bool]]): Bool -> case flag { None -> false; Some(None) -> false; Some(Some(true)) -> true; Some(Some(false)) -> false }.\npub example(): Bool -> value(Some(Some(true))) and not value(Some(Some(false))) and not value(Some(None)).\n";
    let diagnostics = check_syntax_output_with_std_interfaces(source, "std/core/Option.terl");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
