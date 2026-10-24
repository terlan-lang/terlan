use super::*;
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn default_keyword_imports_are_qualified_in_aliases_and_signatures() {
    let syntax = parse_module_as_syntax_output(
        "module sample.Consumer. import sample.keywords.Auto. pub type Css = Auto. pub identity(value: Auto): Auto -> value.",
    ).unwrap();
    let interface = syntax_module_output_to_interface(&syntax);
    assert_eq!(interface.type_bodies["Css"], ["sample.keywords.Auto"]);
    assert_eq!(
        interface.functions[&("identity".into(), 1)].return_type,
        "sample.keywords.Auto"
    );
}

#[test]
fn grouped_keyword_imports_keep_their_provider_identity() {
    let syntax = parse_module_as_syntax_output(
        "module sample.Consumer. import sample.keywords.{Auto, Normal}. pub type Css = Auto | Normal.",
    ).unwrap();
    let interface = syntax_module_output_to_interface(&syntax);
    assert_eq!(
        interface.type_bodies["Css"],
        ["sample.keywords.Auto", "sample.keywords.Normal"]
    );
}

#[test]
fn imported_names_inside_atom_literals_are_preserved() {
    let syntax = parse_module_as_syntax_output(
        r#"module sample.Consumer. import sample.keywords.Auto. pub type Label = Atom["Auto"]."#,
    )
    .unwrap();
    let interface = syntax_module_output_to_interface(&syntax);
    assert_eq!(interface.type_bodies["Label"], [r#"Atom["Auto"]"#]);
    let names = HashMap::from([("Auto".into(), "sample.keywords.Auto".into())]);
    assert_eq!(
        qualify_syntax_type_text(r#"Atom["escaped \"Auto\""]"#, &names),
        r#"Atom["escaped \"Auto\""]"#
    );
}

#[test]
fn importing_helper_modules_preserves_prelude_type_heads() {
    let syntax = parse_module_as_syntax_output(
        "module sample.Consumer. import std.core.{Atom, Int}. import std.collections.List. pub type Count = Int. pub identity(values: List[Int]): List[Int] -> values.",
    ).unwrap();
    let interface = syntax_module_output_to_interface(&syntax);
    assert_eq!(interface.type_bodies["Count"], ["Int"]);
    assert_eq!(
        interface.functions[&("identity".into(), 1)].return_type,
        "List[Int]"
    );
}
