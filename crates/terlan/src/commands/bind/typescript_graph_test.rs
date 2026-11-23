use super::*;
use serde_json::Value;

fn fixture(sources: &[(&str, &str)]) -> (tempfile::TempDir, Options, Value) {
    let root = tempfile::tempdir().unwrap();
    for (name, text) in sources {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let options = Options {
        input: "Root.d.ts".into(),
        namespace: Some("fixture.sdk".into()),
        output: root.path().join("generated"),
    };
    generate(root.path(), &options).unwrap();
    let bindings = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("std/js/manifests/std_js_bindings.json"))
            .unwrap(),
    )
    .unwrap();
    (root, options, bindings)
}

fn module<'a>(bindings: &'a Value, name: &str) -> &'a Value {
    bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["module"] == name)
        .unwrap_or_else(|| panic!("missing {name}"))
}
fn member<'a>(module: &'a Value, name: &str) -> &'a Value {
    module["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("missing {name}: {module}"))
}

#[test]
fn imported_class_alias_exposes_inherited_generic_callbacks_and_private_members_stay_private() {
    let (_root, options, bindings) = fixture(&[
        ("Root.d.ts", "import type { Widget as Remote } from './barrel.js'; declare global { namespace sdk { type Widget<T> = Remote<T>; } }"),
        ("barrel.d.ts", "export * from './widget';"),
        ("widget.d.ts", "import { Base } from './base.ts'; export declare class Widget<T> extends Base<T> { own: boolean; private hidden: string; protected secret(): string; chain(): this; }"),
        ("base.d.ts", "export declare class Base<T> { name: string; value: T; transform<U>(callback: (value: T) => U): U; }"),
    ]);
    let widget = module(&bindings, "fixture.sdk.sdk.Widget");
    assert_eq!(
        member(widget, "name")["return_type"],
        "std.js.String.JsString"
    );
    assert_eq!(member(widget, "value")["return_type"], "T");
    assert_eq!(
        member(widget, "transform")["type_parameters"],
        serde_json::json!(["U"])
    );
    assert_eq!(
        member(widget, "transform")["parameters"][0]["type"],
        "(T) -> U"
    );
    assert!(!widget["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "hidden" || entry["name"] == "secret"));
    let source =
        std::fs::read_to_string(options.output.join("fixture/sdk/sdk/Widget.terl")).unwrap();
    assert!(!source.contains("= Dynamic"));
    assert!(source.contains("transform[U]"), "{source}");
    assert_eq!(bindings["inputs"].as_array().unwrap().len(), 4);
}

#[test]
fn merged_global_interfaces_and_method_generic_shadowing_keep_their_scopes() {
    let (_root, options, bindings) = fixture(&[
        ("Root.d.ts", "import './globals'; export interface Pair { value: Shared; } export interface Box<T> { map<T>(value:T):T; } export interface Merged { left:string; } export interface Merged { right:number; }"),
        ("globals.d.ts", "interface Shared { title:string; }"),
    ]);
    member(module(&bindings, "fixture.sdk.Pair"), "value");
    let merged = module(&bindings, "fixture.sdk.Merged");
    member(merged, "left");
    member(merged, "right");
    let source = std::fs::read_to_string(options.output.join("fixture/sdk/Box.terl")).unwrap();
    assert!(source.contains("map[TMethod]"), "{source}");
    assert_eq!(
        member(module(&bindings, "fixture.sdk.Box"), "map")["return_type"],
        "TMethod"
    );
}

#[test]
fn missing_import_is_incomplete_and_never_a_dynamic_public_alias() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "import type { Missing } from './absent'; export type Public = Missing;",
    )]);
    assert!(!bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["module"] == "fixture.sdk.Public"));
    assert_eq!(bindings["coverage"]["complete"], false);
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["reason"] == "ts_bindgen.unresolved_type_reference"));
}

#[test]
fn distinct_sources_with_the_same_type_name_keep_distinct_identities() {
    let (_, _, bindings) = fixture(&[
        ("Root.d.ts", "import { Item as Left } from './a-b'; import { Item as Right } from './a_b'; export interface Pair { left: Left; right: Right; }"),
        ("a-b.d.ts", "export interface Item { left: string; }"),
        ("a_b.d.ts", "export interface Item { right: number; }"),
    ]);
    let pair = module(&bindings, "fixture.sdk.Pair");
    assert_ne!(
        member(pair, "left")["return_type"],
        member(pair, "right")["return_type"]
    );
}

#[test]
fn declaration_cycles_are_loadable_and_inheritance_cycles_are_explicit() {
    let (_, _, bindings) = fixture(&[
        ("Root.d.ts", "import { B } from './B'; export interface A { peer?: B; } export interface Cycle extends Cycle {}"),
        ("B.d.ts", "import { A } from './Root'; export interface B { peer?: A; }"),
    ]);
    assert!(
        member(module(&bindings, "fixture.sdk.A"), "peer")["return_type"]
            .as_str()
            .unwrap()
            .contains("Option[")
    );
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["reason"] == "ts_bindgen.unresolved_heritage"));
}

#[test]
fn transitive_input_hashes_are_pinned_and_checked_on_reuse() {
    let (root, options, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import { Item } from './Item'; export type Public = Item;",
        ),
        ("Item.d.ts", "export interface Item { title: string; }"),
    ]);
    assert_eq!(bindings["inputs"].as_array().unwrap().len(), 2);
    std::fs::write(
        root.path().join("Item.d.ts"),
        "export interface Item { changed: boolean; }",
    )
    .unwrap();
    let options = Options {
        input: options.output.join("typescript-inputs.json"),
        namespace: None,
        output: root.path().join("changed"),
    };
    assert!(generate(root.path(), &options)
        .unwrap_err()
        .contains("sha256_mismatch"));
    assert!(!options.output.exists());
}

#[test]
fn named_reexports_namespace_imports_and_defaults_resolve() {
    let (_, _, bindings) = fixture(&[
        ("Root.d.ts", "import * as bundle from './barrel'; import Default from './default'; export type First = bundle.Public; export type Second = Default;"),
        ("barrel.d.ts", "export { Item as Public } from './item';"),
        ("item.d.ts", "export interface Item { name: string; }"),
        ("default.d.ts", "export default class Default { id: number; }"),
    ]);
    member(module(&bindings, "fixture.sdk.First"), "name");
    member(module(&bindings, "fixture.sdk.Second"), "id");
}

#[test]
fn ambiguous_star_exports_do_not_resolve_by_file_order() {
    let (_, _, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import { Item } from './barrel'; export type Public = Item;",
        ),
        (
            "barrel.d.ts",
            "export * from './one'; export * from './two';",
        ),
        ("one.d.ts", "export interface Item { one: string; }"),
        ("two.d.ts", "export interface Item { two: string; }"),
    ]);
    assert!(!bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["module"] == "fixture.sdk.Public"));
    assert_eq!(bindings["coverage"]["complete"], false);
}

#[test]
fn inherited_method_generics_do_not_capture_derived_type_arguments() {
    let (_,_,bindings) = fixture(&[
        ("Root.d.ts", "export interface Base<T> { pair<U>(first:T, second:U):U; } export interface Derived<U> extends Base<U> {}"),
    ]);
    let method = member(module(&bindings, "fixture.sdk.Derived"), "pair");
    assert_eq!(method["parameters"][0]["type"], "U");
    assert_eq!(method["parameters"][1]["type"], "UMethod");
    assert_eq!(method["return_type"], "UMethod");
}

#[test]
fn unapplied_generic_aliases_are_reported_instead_of_emitting_invalid_types() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "export interface Generic<T> { value:T; } export type Invalid = Generic;",
    )]);
    assert!(!bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["module"] == "fixture.sdk.Invalid"));
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["reason"] == "ts_bindgen.unresolved_generic_arguments"));
}

#[test]
fn public_generic_reexports_keep_their_type_parameters() {
    let (_, _, bindings) = fixture(&[
        ("Root.d.ts", "export * from './barrel';"),
        ("barrel.d.ts", "export { Item as Public } from './Item';"),
        ("Item.d.ts", "export interface Item<T> { value:T; }"),
    ]);
    let public = module(&bindings, "fixture.sdk.Public");
    assert_eq!(member(public, "value")["return_type"], "T");
    assert_eq!(bindings["coverage"]["complete"], true);
}

#[test]
fn unsupported_heritage_and_constraints_are_explicit_without_losing_own_members() {
    let (_,_,bindings) = fixture(&[
        ("Root.d.ts", "export declare class Parent<T> { data:T; } export declare class Child<T> extends Parent<T & object> { name:string; restricted<U extends string>(value:U):U; }"),
    ]);
    let child = module(&bindings, "fixture.sdk.Child");
    member(child, "name");
    assert!(!child["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "restricted"));
    assert_eq!(bindings["coverage"]["complete"], false);
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["reason"] == "ts_bindgen.unsupported_heritage"));
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["reason"] == "ts_bindgen.unsupported_generic_constraint"));
}
