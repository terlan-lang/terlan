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
fn generic_union_alias_does_not_inherit_one_alternatives_members() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "export interface Box<T> { value: T; replace(value: T): T; }
         export type Choice = Box<string> | Box<number>;
         export type Nested = Box<Box<string | number>>;",
    )]);
    assert!(module(&bindings, "fixture.sdk.Choice")["members"]
        .as_array()
        .unwrap()
        .is_empty());
    member(module(&bindings, "fixture.sdk.Nested"), "replace");
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

#[test]
fn unavailable_optional_suffix_keeps_callable_prefix_and_reports_incomplete_coverage() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "import { Missing } from './absent';
         export interface Runtime {
           create(name: string, dependencies?: string[], options?: Missing, extra?: boolean): Runtime;
           configure(name: string, options?: string | { flag: boolean }): Runtime;
           createWithRest(name: string, options?: string | { flag: boolean }, ...flags: boolean[]): Runtime;
           requires(value: Missing, fallback?: string): void;
           returns(name?: Missing): Missing;
         }
         export interface Callback { invoke(name: string, options?: Missing): void; }",
    )]);
    let create = member(module(&bindings, "fixture.sdk.Runtime"), "create");
    assert_eq!(create["partial"], true);
    let params = create["parameters"].as_array().unwrap();
    assert_eq!(params.len(), 2);
    assert_eq!(params[0]["type"], "std.js.String.JsString");
    assert_eq!(params[1]["optional"], true);
    let configure = member(module(&bindings, "fixture.sdk.Runtime"), "configure");
    assert_eq!(configure["partial"], true);
    assert_eq!(configure["parameters"].as_array().unwrap().len(), 1);
    for name in ["requires", "returns", "create_with_rest"] {
        assert!(!module(&bindings, "fixture.sdk.Runtime")["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == name));
    }
    let callback = module(&bindings, "fixture.sdk.Callback");
    member(callback, "invoke");
    assert!(!callback["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["kind"] == "mixin"));
    assert_eq!(bindings["coverage"]["complete"], false);
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["source"] == "Runtime.create"
            && entry["reason"] == "ts_bindgen.unresolved_type_reference"));
}

#[test]
fn fills_generic_defaults_for_imported_aliases_heritage_and_nested_signatures() {
    let (_, _, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import { Base, Pair, Unsupported, Recursive } from './types';
        export type Public = Pair<number>;
        export interface Derived extends Base { items: string[]; }
        export interface Usage {
          load(): Promise<Pair<boolean>>;
          explicit(): Unsupported<string>;
          missing(): Unsupported;
          recursive(): Recursive;
        }",
        ),
        (
            "types.d.ts",
            "export interface Base<T = boolean> { value: T; }
        export type Pair<T = string, U = T[]> = PairRecord<T, U>;
        interface PairRecord<T, U> { first: T; second: U; }
        export interface Unsupported<T = keyof Pair> { value: T; }
        export interface Recursive<T = Recursive> { value: T; }",
        ),
    ]);
    let public = module(&bindings, "fixture.sdk.Public");
    assert_eq!(
        member(public, "first")["return_type"],
        "std.js.Number.JsNumber"
    );
    assert_eq!(
        member(public, "second")["return_type"],
        "std.js.Array[std.js.Number.JsNumber]"
    );
    assert_eq!(
        member(module(&bindings, "fixture.sdk.Derived"), "value")["return_type"],
        "Bool"
    );
    let usage = module(&bindings, "fixture.sdk.Usage");
    assert!(member(usage, "load")["return_type"]
        .as_str()
        .unwrap()
        .ends_with(".Pair[Bool, std.js.Array[Bool]]]"));
    member(usage, "explicit");
    for name in ["missing", "recursive"] {
        assert!(!usage["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == name));
        assert!(bindings["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["source"]
                .as_str()
                .unwrap()
                .starts_with(&format!("Usage.{name}"))
                && entry["reason"] == "ts_bindgen.unresolved_generic_arguments"));
    }
}

#[test]
fn bundled_browser_types_resolve_without_overriding_package_bindings() {
    let (_, _, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import type { HTMLElement } from './absent';
        import type { Target as EventTarget } from './local';
        type Document = keyof Event;
        export interface Host {
          root: Element;
          listen(callback: (event: Event) => void): void;
          receiver: EventTarget;
          unavailableRoot: HTMLElement;
          unavailableDocument: Document;
        }",
        ),
        ("local.d.ts", "export interface Target { name: string; }"),
    ]);
    let host = module(&bindings, "fixture.sdk.Host");
    assert_eq!(member(host, "root")["return_type"], "std.js.Dom.Element");
    assert_eq!(
        member(host, "listen")["parameters"][0]["type"],
        "(std.js.Dom.Event) -> std.core.Unit"
    );
    assert!(member(host, "receiver")["return_type"]
        .as_str()
        .unwrap()
        .starts_with("fixture.sdk."));
    for name in ["unavailable_root", "unavailable_document"] {
        assert!(!host["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == name));
    }
}

#[test]
fn callable_interfaces_and_aliases_preserve_optional_parameters_and_named_call_methods() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "export interface Callable {
            <T>(value: T, separator?: string): T;
            call(count: number): boolean;
         }
         export type OptionalCallback = (name: string, flag?: boolean) => number;
         export type PublicCallback = OptionalCallback;
         export interface ExplicitContext { (this: { count: number }): number; }
         export interface Restricted { <T extends string>(value: T): T; }",
    )]);
    let callable = module(&bindings, "fixture.sdk.Callable");
    let call = member(callable, "call");
    assert_eq!(call["kind"], "call");
    assert_eq!(call["return_type"], "T");
    assert_eq!(call["parameters"][1]["optional"], true);
    let named = member(callable, "call_overload_2");
    assert_eq!(named["kind"], "method");
    assert_eq!(named["js_name"], "call");
    let callback = module(&bindings, "fixture.sdk.PublicCallback");
    assert_eq!(member(callback, "call")["kind"], "call");
    for name in ["ExplicitContext", "Restricted"] {
        assert!(module(&bindings, &format!("fixture.sdk.{name}"))["members"]
            .as_array()
            .unwrap()
            .is_empty());
    }
}

#[test]
fn closed_method_constraints_survive_inheritance_and_alias_specialization() {
    let (_root, options, bindings) = fixture(&[(
        "Root.d.ts",
        "export interface Base<T> {
            declare_value<U extends { key: T }>(value: U): U;
            optional<U extends { key?: T }>(value: U): U;
            camel_case<U extends { theKey: T }>(value: U): U;
         }
         export interface Derived extends Base<string> {}
         export type Public = Derived;",
    )]);
    let public = module(&bindings, "fixture.sdk.Public");
    let declaration = member(public, "declare_value");
    assert_eq!(
        declaration["type_constraints"],
        serde_json::json!(["{key: std.js.String.JsString}"])
    );
    assert_eq!(declaration["parameters"][0]["type"], "U");
    assert_eq!(declaration["return_type"], "U");
    let source = std::fs::read_to_string(
        options
            .output
            .join(public["source"].as_str().expect("generated source path")),
    )
    .unwrap();
    assert!(
        source.contains("declare_value[U => {key: std.js.String.JsString}]"),
        "{source}"
    );
    assert!(public["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["name"] != "optional" && entry["name"] != "camel_case"));
}

#[test]
fn constructor_signatures_remain_distinct_from_calls_and_skip_abstract_construction() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "export interface Instance { readonly name: string; }
         export interface Hybrid { new (name: string): Instance; (x: number): string; }
         export type Constructor<T = Instance> = new (name: string, ...flags: boolean[]) => T;
         export type Public = Constructor;
         export type Abstract = abstract new (name: string) => Instance;",
    )]);
    let hybrid = module(&bindings, "fixture.sdk.Hybrid");
    assert_eq!(member(hybrid, "call")["kind"], "call");
    assert_eq!(member(hybrid, "new")["kind"], "construct");
    let constructor = member(module(&bindings, "fixture.sdk.Public"), "new");
    assert_eq!(constructor["kind"], "construct");
    assert_eq!(constructor["parameters"][1]["rest"], true);
    assert_eq!(constructor["return_type"], "fixture.sdk.Instance");
    assert!(!bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["module"] == "fixture.sdk.Abstract"));
}

#[test]
fn conditional_specialization_preserves_callable_branch_and_distributes_named_unions() {
    let (_root, _, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import type { FactoryFunction, ResultFor, Bounded } from './types';
        type Argument = string | boolean;
        export interface Runtime {
          factory(): FactoryFunction<(value: string) => boolean>;
          label(): ResultFor<'hit'>;
          outcomes(): ResultFor<Argument>;
          impossible(): ResultFor<never>;
          generic<T>(value: T): ResultFor<T>;
          constraint(): Bounded<number>;
        }",
        ),
        (
            "types.d.ts",
            "export type FactoryFunction<T> = T extends abstract new (...args: any[]) => unknown
            ? (...args: ConstructorParameters<T>) => InstanceType<T> : T;
         export type ResultFor<T> = T extends string ? number : boolean;
         export type Bounded<T extends string> = T extends string ? number : boolean;",
        ),
    ]);
    let runtime = module(&bindings, "fixture.sdk.Runtime");
    assert_eq!(
        member(runtime, "factory")["return_type"],
        "(std.js.String.JsString) -> Bool"
    );
    assert_eq!(
        member(runtime, "label")["return_type"],
        "std.js.Number.JsNumber"
    );
    assert_eq!(
        member(runtime, "outcomes")["return_type"],
        "Bool | std.js.Number.JsNumber"
    );
    assert_eq!(member(runtime, "impossible")["return_type"], "Never");
    assert!(runtime["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["name"] != "generic" && entry["name"] != "constraint"));
    assert_eq!(bindings["coverage"]["complete"], false);
}

#[test]
fn constructor_utilities_preserve_arguments_instance_and_abstractness() {
    let (_root, options, bindings) = fixture(&[
        (
            "Root.d.ts",
            "import type { Concrete, Abstract, Tokens } from './constructors';
             type Factory<T> = T extends abstract new (...args: any[]) => unknown
                 ? (...args: ConstructorParameters<T>) => InstanceType<T> : T;
             type Kind<T> = T extends new (...args: any[]) => unknown ? string : boolean;
             export interface Runtime {
                concrete(): Factory<Concrete>;
                abstracted(): Factory<Abstract>;
                inline(): Factory<new (flag: boolean) => string>;
                kind(): Kind<Abstract>;
             }
             export type Arguments = ConstructorParameters<Tokens>;
             export type Parameters<T> = ConstructorParameters<T>;
             export type Fixed = Parameters<Concrete>;",
        ),
        (
            "constructors.d.ts",
            "export type Concrete<T = boolean> = new (input: string, flag: T) => number;
             export type Abstract = abstract new (code: number) => boolean;
             export type Tokens = new (...tokens: [name: string, enabled: boolean]) => boolean;",
        ),
    ]);
    let runtime = module(&bindings, "fixture.sdk.Runtime");
    assert_eq!(
        member(runtime, "concrete")["return_type"],
        "(std.js.String.JsString, Bool) -> std.js.Number.JsNumber"
    );
    assert_eq!(
        member(runtime, "abstracted")["return_type"],
        "(std.js.Number.JsNumber) -> Bool"
    );
    assert_eq!(
        member(runtime, "inline")["return_type"],
        "(Bool) -> std.js.String.JsString"
    );
    assert_eq!(member(runtime, "kind")["return_type"], "Bool");
    for name in ["Arguments", "Fixed"] {
        let path = module(&bindings, &format!("fixture.sdk.{name}"))["source"]
            .as_str()
            .unwrap();
        let source = std::fs::read_to_string(options.output.join(path)).unwrap();
        assert!(
            source.contains("{std.js.String.JsString, Bool}"),
            "{source}"
        );
    }
}

#[test]
fn constructor_utility_names_resolve_to_local_aliases_when_shadowed() {
    let (_root, _, bindings) = fixture(&[(
        "Root.d.ts",
        "type InstanceType<T> = boolean;
         type ConstructorParameters<T> = number;
         type Ctor = new (a: string) => string;
         export interface Runtime {
           instance(): InstanceType<Ctor>;
           parameters(): ConstructorParameters<Ctor>;
         }",
    )]);
    let runtime = module(&bindings, "fixture.sdk.Runtime");
    assert_eq!(
        member(runtime, "instance")["return_type"],
        "fixture.sdk.InstanceType[fixture.sdk.Ctor]"
    );
    assert_eq!(
        member(runtime, "parameters")["return_type"],
        "fixture.sdk.ConstructorParameters[fixture.sdk.Ctor]"
    );
}

#[test]
fn computed_alias_expansion_checks_constructor_and_callback_bounds() {
    let (_, _, bindings) = fixture(&[(
        "Root.d.ts",
        "type Domain = ((...args: any[]) => unknown) | (abstract new (...args: any[]) => unknown);
         type Computed<T extends Domain> =
             boolean | (T extends abstract new (...args: any[]) => unknown ? InstanceType<T> : never);
         type Ctor = new (value: string) => number;
         export interface Runtime {
             constructable(): Computed<Ctor>;
             callable(): Computed<(...args: never[]) => unknown>;
             invalid(): Computed<number>;
         }",
    )]);
    let runtime = module(&bindings, "fixture.sdk.Runtime");
    assert!(
        runtime["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "constructable"),
        "computed constructor member was skipped: {}",
        bindings["skipped"]
    );
    assert_eq!(
        member(runtime, "constructable")["return_type"],
        "Bool | std.js.Number.JsNumber"
    );
    assert_eq!(member(runtime, "callable")["return_type"], "Bool");
    assert!(runtime["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["name"] != "invalid"));
}

#[test]
fn variadic_union_preserves_callable_domains_and_constructor_identity() {
    let (_root, options, bindings) = fixture(&[(
        "Root.d.ts",
        "type Ctor<T = unknown> = new (...args: any[]) => T;
         type Annotation<T extends (...args: any[]) => unknown> = [...string[], T];
         type Allowed = Ctor | Annotation<(...args: never[]) => unknown> | ((...args: any[]) => unknown);
         export interface Registry {
             register(name: string, value: Allowed): this;
             restricted(name: string, value: Annotation<(...args: string[]) => unknown>): this;
         }",
    )]);
    let registry = module(&bindings, "fixture.sdk.Registry");
    let registration = member(registry, "register");
    assert_eq!(registration["arity"], 3);
    assert_eq!(registration["parameters"][1]["type"], "fixture.sdk.Allowed");
    let allowed = module(&bindings, "fixture.sdk.Allowed");
    let source =
        std::fs::read_to_string(options.output.join(allowed["summary"].as_str().unwrap())).unwrap();
    for alternative in [
        "fixture.sdk.Annotation[std.js.Callable.VariadicCallable[std.js.Callable.NeverArguments]]",
        "std.js.Callable.VariadicCallable[std.js.Callable.AnyArguments]",
        "std.js.Callable.VariadicConstructor[std.js.Callable.AnyArguments]",
    ] {
        assert!(source.contains(alternative), "{source}");
    }
    assert_eq!(registration["return_type"], "fixture.sdk.Registry");
    assert!(registry["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["name"] != "restricted"));
}

#[test]
fn variadic_tuple_aliases_pack_checked_slots_and_reexports_specialize_them() {
    let (_root, options, bindings) = fixture(&[(
        "Root.d.ts",
        "export type Recipe<T> = readonly [id: number, ...tokens: string[], factory: (data: T) => boolean];
         export type StringRecipe = Recipe<string>;
         export type Fixed = [x: string, y: boolean];
         export type Empty = [];
         export type Optional = [x?: string];
         export type Unresolved<T extends unknown[]> = [...T, boolean];",
    )]);
    let pack = member(module(&bindings, "fixture.sdk.Recipe"), "pack");
    assert_eq!(pack["kind"], "tuple_pack");
    assert_eq!(pack["arity"], 3);
    assert_eq!(pack["type_parameters"], serde_json::json!(["T"]));
    assert_eq!(pack["parameters"][1]["rest"], true);
    assert_eq!(
        pack["parameters"][1]["type"],
        "std.js.Array[std.js.String.JsString]"
    );
    let string_pack = member(module(&bindings, "fixture.sdk.StringRecipe"), "pack");
    assert_eq!(
        string_pack["parameters"][2]["type"],
        "(std.js.String.JsString) -> Bool"
    );
    assert_eq!(
        member(module(&bindings, "fixture.sdk.Empty"), "pack")["arity"],
        0
    );
    let fixed = std::fs::read_to_string(options.output.join("fixture/sdk/Fixed.terl")).unwrap();
    assert!(
        fixed.contains("{std.js.String.JsString, Bool}") || fixed.contains("{JsString, Bool}"),
        "{fixed}"
    );
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["source"] == "fixture.sdk.Unresolved"
            && entry["reason"] == "ts_bindgen.unsupported_tuple_shape"));
    assert!(!bindings["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["module"] == "fixture.sdk.Optional"));
}

#[test]
fn callable_tuple_bounds_are_preserved_through_alias_specialization() {
    let (_root, options, bindings) = fixture(&[(
        "Root.d.ts",
        "type JsonValue = ReturnType<typeof JSON.parse>;
         export type Annotation<T extends (...args: JsonValue[]) => unknown> = [...string[], T];
         export type Fixed = Annotation<(value: string) => boolean>;
         export type StringsOnly<T extends (...args: string[]) => unknown> = [...string[], T];",
    )]);
    let pack = member(module(&bindings, "fixture.sdk.Annotation"), "pack");
    assert_eq!(pack["callable_bounds"], serde_json::json!(["T"]));
    let source =
        std::fs::read_to_string(options.output.join("fixture/sdk/Annotation.terl")).unwrap();
    assert!(source.contains("[std.js.Callable.Callable[T]]"), "{source}");
    let fixed = member(module(&bindings, "fixture.sdk.Fixed"), "pack");
    assert_eq!(
        fixed["callable_bounds"],
        serde_json::json!(["(std.js.String.JsString) -> Bool"])
    );
    assert_eq!(fixed["parameters"][1]["type"], fixed["callable_bounds"][0]);
    assert!(bindings["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["source"] == "fixture.sdk.StringsOnly"));
}

#[test]
fn shadowed_json_utility_does_not_prove_callable_bounds() {
    for declarations in [
        "declare const JSON: { parse(text: string): string };",
        "type ReturnType<T> = string;",
        "declare global { interface JSON { parse(text: string): string; } }",
    ] {
        let source = format!(
            "{declarations}
             type JsonValue = ReturnType<typeof JSON.parse>;
             export type Annotation<T extends (...args: JsonValue[]) => unknown> = [...string[], T];"
        );
        let (_, _, bindings) = fixture(&[("Root.d.ts", &source)]);
        assert!(bindings["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["source"] == "fixture.sdk.Annotation"));
    }
}
