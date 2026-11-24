//! Regression coverage for DI-style factories across declaration files.

use super::*;
use serde_json::Value;

#[test]
fn registration_preserves_imported_factory_constructor_and_annotation_union() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Root.d.ts"),
        "import type { Recipe } from './recipes.js';
         export interface Registry { service(name: string, recipe: Recipe): this; }",
    )
    .unwrap();
    std::fs::write(
        root.path().join("recipes.d.ts"),
        "type Dynamic = ReturnType<typeof JSON.parse>;
         type Constructor<T = unknown> = new (...args: Dynamic[]) => T;
         type Annotation<F extends (...args: Dynamic[]) => unknown> = [...string[], F];
         type Factory<T> = T extends abstract new (...args: Dynamic[]) => unknown
             ? (...args: ConstructorParameters<T>) => InstanceType<T> : T;
         type Injectable<T extends ((...args: Dynamic[]) => unknown) |
             (abstract new (...args: Dynamic[]) => unknown)> =
             Annotation<Factory<T>> |
             (T extends abstract new (...args: Dynamic[]) => unknown
                 ? Constructor<InstanceType<T>> : never) | T;
         type CallbackRecipe = Injectable<(...args: never[]) => unknown>;
         type ConstructorRecipe = Injectable<Constructor>;
         export type Recipe = ConstructorRecipe | CallbackRecipe;",
    )
    .unwrap();
    let options = Options {
        input: "Root.d.ts".into(),
        namespace: Some("fixture.registration".into()),
        output: root.path().join("generated"),
    };
    generate(root.path(), &options).unwrap();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("std/js/manifests/std_js_bindings.json"))
            .unwrap(),
    )
    .unwrap();
    let registry = manifest["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["module"] == "fixture.registration.Registry")
        .unwrap();
    let service = registry["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["name"] == "service")
        .unwrap_or_else(|| panic!("registration skipped: {}", manifest["skipped"]));
    assert_eq!(service["parameters"].as_array().unwrap().len(), 2);
    let recipe = service["parameters"][1]["type"].as_str().unwrap();
    for shape in ["VariadicCallable", "VariadicConstructor", "Annotation"] {
        assert!(recipe.contains(shape), "lost {shape} in {recipe}");
    }
    assert!(!recipe.contains("Dynamic"), "{recipe}");
    assert_eq!(service["return_type"], "fixture.registration.Registry");
    assert!(
        !manifest["skipped"].as_array().unwrap().iter().any(|entry| {
            entry["source"] == "Registry.service"
                || entry["source"].as_str().is_some_and(|name| {
                    name.ends_with(".Recipe")
                        || name.ends_with(".CallbackRecipe")
                        || name.ends_with(".ConstructorRecipe")
                })
        })
    );
    let source =
        std::fs::read_to_string(options.output.join("fixture/registration/Registry.terl")).unwrap();
    assert!(!source.contains("Dynamic"), "{source}");
}

#[test]
fn generic_union_alias_does_not_inherit_one_alternatives_members() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Root.d.ts"),
        "export interface Box<T> { value: T; replace(value: T): T; }
         export type Choice = Box<string> | Box<number>;
         export type Nested = Box<Box<string | number>>;",
    )
    .unwrap();
    let options = Options {
        input: "Root.d.ts".into(),
        namespace: Some("fixture.registration".into()),
        output: root.path().join("generated"),
    };
    generate(root.path(), &options).unwrap();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("std/js/manifests/std_js_bindings.json"))
            .unwrap(),
    )
    .unwrap();
    let outputs = manifest["outputs"].as_array().unwrap();
    let choice = outputs
        .iter()
        .find(|module| module["module"] == "fixture.registration.Choice")
        .unwrap();
    assert!(choice["members"].as_array().unwrap().is_empty());
    let nested = outputs
        .iter()
        .find(|module| module["module"] == "fixture.registration.Nested")
        .unwrap();
    assert!(nested["members"]
        .as_array()
        .unwrap()
        .iter()
        .any(|member| member["name"] == "replace"));
}
