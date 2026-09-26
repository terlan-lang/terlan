use super::{parse_module, parse_script, parse_terlan_expr};
use crate::terlan_syntax::lalrpop_boundary::{
    parse_lalrpop_expression, parse_lalrpop_pattern, parse_lalrpop_type,
};

#[test]
fn retired_atom_literals_are_rejected_at_every_entry_point() {
    for atom in [":ok", ":\"ok\"", ":'ok'", "'ok'", "Atom['ok']"] {
        assert!(
            parse_terlan_expr(atom).is_err(),
            "expression accepted {atom}"
        );
        assert!(
            parse_lalrpop_expression(atom).is_err(),
            "generated expression accepted {atom}"
        );
        assert!(
            parse_lalrpop_pattern(atom).is_err(),
            "pattern accepted {atom}"
        );
        assert!(parse_lalrpop_type(atom).is_err(), "type accepted {atom}");
        let source = format!("module retired. pub value() -> {atom}.");
        assert!(parse_module(&source).is_err(), "module accepted {atom}");
        assert!(
            parse_script(&format!("{atom}."), "retired").is_err(),
            "script accepted {atom}"
        );
    }
}

#[test]
fn canonical_atoms_and_colons_remain_valid() {
    for source in [
        r#"Atom["ok"]"#,
        r#"{status: Atom["ok"]}"#,
        r#"":ok is text, not syntax""#,
    ] {
        parse_terlan_expr(source).expect(source);
    }
    parse_module(r#"module atoms. pub type Ok. pub value(x: Int): Int -> x."#)
        .expect("type annotations and shorthand atoms");
    parse_lalrpop_pattern(r#"Atom["ok"]"#).expect("canonical atom pattern");
    parse_lalrpop_type(r#"Atom["ok"]"#).expect("canonical atom type");
    parse_terlan_expr("sql[Row] {select 'ready' as status}").expect("SQL owns single-quoted text");
}

#[test]
fn dot_calls_cannot_bypass_module_diagnostics() {
    for source in ["f.(1)", "(f).(1)", "f. (1)", "f.\n(1)"] {
        assert!(
            parse_lalrpop_expression(source).is_err(),
            "accepted {source}"
        );
        assert!(
            parse_script(&format!("{source}."), "retired").is_err(),
            "script accepted {source}"
        );
    }
    for source in ["f(1)", "(f)(1)", "f[Int](1)"] {
        parse_terlan_expr(source).expect(source);
    }
}

#[test]
fn implications_are_only_generic_parameter_constraints() {
    for source in ["T => {title: String}", "List[T => {title: String}]"] {
        assert!(
            parse_lalrpop_type(source).is_err(),
            "type accepted {source}"
        );
    }
    for declaration in [
        "pub display[T](value: T): String where T => {title: String} -> value.title.",
        "pub display[T](value: T => {title: String}): String -> value.title.",
        "pub type Display = T => {title: String}.",
        "pub struct Display {value: T => {title: String}}.",
        "pub shape Display(value) => value.",
        "pub display(value) where value => {title: String} -> value.title.",
    ] {
        assert!(
            parse_module(&format!("module retired. {declaration}")).is_err(),
            "accepted {declaration}"
        );
    }
    parse_module(
        "module current. pub display[T => {title: String}](value: T): String -> value.title.",
    )
    .expect("generic implication");
    parse_module("module current. pub positive(value) where value > 0 -> value.")
        .expect("runtime guard");
    parse_module("module current. pub impl Render[T => {title: String}] for T { render(value: T): String -> value.title. }.")
        .expect("implementation generic implication");
}

#[test]
fn legacy_when_guards_are_rejected_by_fragment_and_script_parsers() {
    let expression = "case value { x when x > 0 -> x; _ -> 0 }";
    assert!(parse_lalrpop_expression(expression).is_err());
    assert!(parse_script(&format!("{expression}."), "retired").is_err());
    parse_lalrpop_expression("case value { x where x > 0 -> x; _ -> 0 }")
        .expect("canonical where guard");
}

#[test]
fn retired_syntax_in_strings_and_comments_is_not_rejected() {
    parse_module(
        r#"
module quoted.
// f.(1) and :ok are examples, not executable syntax.
pub type Arrow = Atom["=>"].
pub struct Arrows {value: Atom["=>"]}.
pub text(): String -> "f.(1) :ok T => Shape".
"#,
    )
    .expect("punctuation in data and comments");
}

#[test]
fn retired_equality_spellings_are_allowed_as_data() {
    for operator in ["=:=", "=/=", "/="] {
        for source in [
            format!("\"{operator}\""),
            format!("Atom[\"{operator}\"]"),
            format!("// {operator}\n1 == 1"),
            format!("/* {operator} */ 1 != 2"),
            format!("[\"{operator}\"]"),
        ] {
            parse_terlan_expr(&source).expect(&source);
        }
    }
}

#[test]
fn retired_equality_diagnostics_point_to_the_first_real_operator() {
    for operator in ["=:=", "=/=", "/="] {
        let prefix = "/* =:= */ \"=/=\" ";
        let source = format!("{prefix}{operator} \"/=\"");
        let error = parse_terlan_expr(&source).expect_err("retired operator");
        assert!(error.message.contains("deprecated equality operator"));
        assert_eq!(error.span.start, prefix.len());
        assert_eq!(error.span.end, prefix.len() + operator.len());
    }
    let error = parse_terlan_expr("left /= middle =:= right").expect_err("retired operators");
    assert_eq!(error.span.start, 5);
    assert_eq!(error.span.end, 7);
}
