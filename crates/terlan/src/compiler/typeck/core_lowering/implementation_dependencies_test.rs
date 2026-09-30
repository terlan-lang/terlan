use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreImportKind};

fn dependencies(source: &str, providers: &[&str]) -> Vec<String> {
    let interfaces = providers
        .iter()
        .map(|source| {
            let syntax = parse_module_as_syntax_output(source).expect("parse provider");
            let interface = syntax_module_output_to_interface(&syntax);
            (interface.module.clone(), interface)
        })
        .collect();
    let syntax = parse_module_as_syntax_output(source).expect("parse caller");
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    lower_syntax_module_output_to_core(&syntax, &resolved)
        .imports
        .into_iter()
        .filter(|import| import.kind == CoreImportKind::Module)
        .map(|import| import.module)
        .collect()
}

#[test]
fn qualified_calls_retain_their_visible_source_provider() {
    assert_eq!(
        dependencies(
            "module consumer. pub check(): Bool -> library.Flags.same(true, true).",
            &["module library.Flags. pub same(a: Bool, b: Bool): Bool -> a == b."],
        ),
        ["library.Flags"]
    );
}

#[test]
fn deferred_receivers_retain_public_positive_trait_providers_only() {
    let providers = [
        "module library.Visible. pub trait Render[T] { render(value: T): String. }. pub impl Render[Bool] for Bool { render(value: Bool): String -> \"visible\". }.",
        "module library.Private. pub trait Render[T] { render(value: T): String. }. impl Render[Bool] for Bool { render(value: Bool): String -> \"private\". }.",
        "module library.Denied. pub trait Render[T] { render(value: T): String. }. pub impl not Render[Bool].",
        "module library.Unrelated. pub trait Render[T] { render(value: T, other: Int): String. }. pub impl Render[Bool] for Bool { render(value: Bool, other: Int): String -> \"other\". }.",
    ];
    assert_eq!(
        dependencies(
            "module consumer. pub deferred[T](value: T): String -> value.render().",
            &providers,
        ),
        ["library.Visible"]
    );
    assert!(dependencies("module consumer. pub check(): Bool -> true.", &providers).is_empty());
}

#[test]
fn deferred_receivers_retain_declared_methods_but_not_free_functions() {
    assert_eq!(dependencies(
        "module consumer. import library.Methods. pub deferred[T](value: T): String -> value.render().",
        &[
            "module library.Methods. pub type Value = Int. pub (value: Value) render(): String -> \"method\".",
            "module library.Free. pub render(value: Bool): String -> \"free\".",
        ],
    ), ["library.Methods"]);
}

#[test]
fn deferred_receivers_do_not_import_unrelated_target_specific_methods() {
    assert_eq!(dependencies(
        "module consumer. import std.collections.List. pub count[T](values: List[T]): Int -> values.length().",
        &[
            "module std.collections.List. pub (values: List[T]) length(): Int -> 0.",
            "module std.js.Array. pub opaque type Array[T]. pub (values: Array[T]) length(): Int -> 0.",
        ],
    ), ["std.collections.List"]);
}
