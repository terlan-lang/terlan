use super::ts_dom_module_mapping::*;
use super::ts_parser_adapter::parse_ts_declaration_file;

/// Verifies the committed DOM fixture maps to stable DOM modules.
///
/// Inputs:
/// - The committed `document.d.ts` fixture.
///
/// Output:
/// - Test passes when `Document` and `HTMLElement` become `std.js.Dom.*`
///   module plans with deterministic paths.
///
/// Transformation:
/// - Pins the T0.4 module path convention before generated files are emitted.
#[test]
fn maps_committed_fixture_to_dom_modules() {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert!(mapping.skipped.is_empty());
    assert_eq!(mapping.modules.len(), 2);
    assert_eq!(mapping.modules[0].module_path, "std.js.Dom.Document");
    assert_eq!(mapping.modules[0].type_name, "Document");
    assert_eq!(mapping.modules[0].source_path, "std/js/dom/Document.terl");
    assert_eq!(mapping.modules[0].test_path, "std/js/dom/DocumentTest.terl");
    assert_eq!(mapping.modules[1].module_path, "std.js.Dom.HTMLElement");
    assert_eq!(
        mapping.modules[1].source_path,
        "std/js/dom/HTMLElement.terl"
    );
    assert_eq!(
        mapping.modules[1].test_path,
        "std/js/dom/HTMLElementTest.terl"
    );
}

/// Verifies DOM properties and methods preserve JS names and expose Terlan names.
///
/// Inputs:
/// - The committed `Document` fixture interface.
///
/// Output:
/// - Test passes when source names are preserved and Terlan names are
///   deterministic `snake_case`.
///
/// Transformation:
/// - Confirms generated bindings can call JS camelCase while exposing Terlan
///   snake_case APIs.
#[test]
fn maps_document_members_to_terlan_names_and_types() {
    let module = document_module();

    let title = property(&module, "title");
    assert_eq!(title.terlan_name, "title");
    assert_eq!(title.terlan_type, "std.js.String.JsString");
    assert!(title.readonly);

    let get_element_by_id = method(&module, "getElementById");
    assert_eq!(get_element_by_id.terlan_name, "get_element_by_id");
    assert_eq!(get_element_by_id.params.len(), 1);
    assert_eq!(get_element_by_id.params[0].js_name, "elementId");
    assert_eq!(get_element_by_id.params[0].terlan_name, "element_id");
    assert_eq!(
        get_element_by_id.params[0].terlan_type,
        "std.js.String.JsString"
    );
    assert_eq!(get_element_by_id.return_type, "Option[HTMLElement]");
}

/// Verifies nullable DOM properties map through `Option`.
///
/// Inputs:
/// - The committed `HTMLElement.textContent` fixture property.
///
/// Output:
/// - Test passes when `string | null` becomes `Option[std.js.String.JsString]`.
///
/// Transformation:
/// - Confirms T0.3 nullable type mapping is applied during T0.4 module mapping.
#[test]
fn maps_nullable_dom_property_type() {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");
    let mapping = map_ts_declarations_to_dom_modules(&declarations);
    let element = mapping
        .modules
        .iter()
        .find(|module| module.type_name == "HTMLElement")
        .expect("HTMLElement module should exist");

    let text_content = property(element, "textContent");

    assert_eq!(text_content.terlan_name, "text_content");
    assert_eq!(text_content.terlan_type, "Option[std.js.String.JsString]");
}

/// Verifies unsupported member types become skipped declaration diagnostics.
///
/// Inputs:
/// - Inline `.d.ts` source with an `any` property.
///
/// Output:
/// - Test passes when the unsupported member is skipped without dropping the
///   containing module.
///
/// Transformation:
/// - Converts type-level skip diagnostics into generated-manifest-ready DOM
///   skip diagnostics.
#[test]
fn records_skipped_member_diagnostics() {
    let declarations = parse_ts_declaration_file("interface Unsafe { value: any; name: string; }")
        .expect("inline interface should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert_eq!(mapping.modules.len(), 1);
    assert_eq!(mapping.modules[0].members.len(), 1);
    assert_eq!(mapping.skipped.len(), 1);
    assert_eq!(mapping.skipped[0].source, "Unsafe.value");
    assert_eq!(mapping.skipped[0].reason, "ts_bindgen.unsupported_any");
}

/// Verifies TypeScript declaration merging emits one Terlan module.
///
/// Inputs:
/// - Inline `.d.ts` source with two declarations for the same interface.
///
/// Output:
/// - Test passes when both supported members land in one generated module plan.
///
/// Transformation:
/// - Prevents declaration-merged TypeScript interfaces from producing
///   duplicate generated output paths.
#[test]
fn merges_duplicate_interface_declarations() {
    let declarations = parse_ts_declaration_file(
        r#"
        interface DateLike { year: number; }
        interface DateLike { month: number; }
        "#,
    )
    .expect("inline merged interface should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert!(mapping.skipped.is_empty());
    assert_eq!(mapping.modules.len(), 1);
    assert_eq!(mapping.modules[0].source_path, "std/js/dom/DateLike.terl");
    assert_eq!(mapping.modules[0].members.len(), 2);
    assert_eq!(property(&mapping.modules[0], "year").terlan_name, "year");
    assert_eq!(property(&mapping.modules[0], "month").terlan_name, "month");
}

/// Verifies repeated unsupported members get unique skip-manifest sources.
///
/// Inputs:
/// - Inline `.d.ts` source with two call signatures on the same interface.
///
/// Output:
/// - Test passes when the second unsupported member receives a deterministic
///   suffix instead of duplicating the first source label.
///
/// Transformation:
/// - Keeps generated skip manifests machine-checkable even for TypeScript
///   interfaces with repeated call or construct signatures.
#[test]
fn disambiguates_repeated_unsupported_member_sources() {
    let declarations = parse_ts_declaration_file(
        r#"
        interface Callable {
          (value: string): string;
          (value: number): number;
        }
        "#,
    )
    .expect("inline interface should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert_eq!(mapping.modules.len(), 1);
    assert_eq!(mapping.skipped.len(), 2);
    assert_eq!(mapping.skipped[0].source, "Callable.call_signature");
    assert_eq!(mapping.skipped[1].source, "Callable.call_signature#2");
}

/// Verifies namespace type aliases map to generated Terlan modules.
///
/// Inputs:
/// - Inline Angular-style `ng` namespace declarations.
///
/// Output:
/// - Test passes when aliases map to namespace-specific module paths and type
///   alias targets.
///
/// Transformation:
/// - Pins the generated facade surface needed for Angular.ts namespace parity.
#[test]
fn maps_namespace_type_aliases_to_modules() {
    let declarations = parse_ts_declaration_file(
        r#"
        declare global {
          export namespace ng {
            type Angular = TAngular;
            type HttpPromise<T> = Promise<T>;
          }
        }
        "#,
    )
    .expect("namespace aliases should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert_eq!(mapping.modules.len(), 2);
    assert_eq!(mapping.modules[0].module_path, "ng.Angular");
    assert_eq!(mapping.modules[0].source_path, "ng/Angular.terl");
    assert_eq!(mapping.modules[0].alias_target.as_deref(), Some("Dynamic"));
    assert_eq!(mapping.modules[1].module_path, "ng.HttpPromise");
    assert_eq!(mapping.modules[1].type_params, vec!["T"]);
    assert_eq!(
        mapping.modules[1].alias_target.as_deref(),
        Some("std.js.Promise[T]")
    );
}

/// Verifies acronym boundaries normalize predictably.
///
/// Inputs:
/// - Inline source containing `URLValue` and `HTMLElement`.
///
/// Output:
/// - Test passes when acronym boundaries become readable Terlan member names
///   while source filenames keep the declared module segment.
///
/// Transformation:
/// - Pins deterministic naming for source APIs with common DOM acronyms.
#[test]
fn normalizes_acronym_boundaries() {
    let declarations = parse_ts_declaration_file("interface URLThing { URLValue: string; }")
        .expect("inline interface should parse");

    let mapping = map_ts_declarations_to_dom_modules(&declarations);

    assert_eq!(mapping.modules[0].source_path, "std/js/dom/URLThing.terl");
    assert_eq!(
        property(&mapping.modules[0], "URLValue").terlan_name,
        "url_value"
    );
}

/// Returns the planned `Document` module from the committed fixture.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Owned DOM module mapping for `Document`.
///
/// Transformation:
/// - Parses and maps the committed fixture for tests that need a single module.
fn document_module() -> DomModulePlan {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");
    map_ts_declarations_to_dom_modules(&declarations)
        .modules
        .into_iter()
        .find(|module| module.type_name == "Document")
        .expect("Document module should exist")
}

/// Finds a property plan by JavaScript name.
///
/// Inputs:
/// - `module`: mapped DOM module.
/// - `js_name`: source JavaScript property name.
///
/// Output:
/// - Borrowed property plan.
///
/// Transformation:
/// - Filters module members and panics when the expected property is absent.
fn property<'a>(module: &'a DomModulePlan, js_name: &str) -> &'a DomPropertyPlan {
    module
        .members
        .iter()
        .filter_map(|member| match member {
            DomMemberPlan::Property(property) => Some(property),
            DomMemberPlan::Method(_) => None,
        })
        .find(|property| property.js_name == js_name)
        .unwrap_or_else(|| panic!("missing property {js_name}"))
}

/// Finds a method plan by JavaScript name.
///
/// Inputs:
/// - `module`: mapped DOM module.
/// - `js_name`: source JavaScript method name.
///
/// Output:
/// - Borrowed method plan.
///
/// Transformation:
/// - Filters module members and panics when the expected method is absent.
fn method<'a>(module: &'a DomModulePlan, js_name: &str) -> &'a DomMethodPlan {
    module
        .members
        .iter()
        .filter_map(|member| match member {
            DomMemberPlan::Method(method) => Some(method),
            DomMemberPlan::Property(_) => None,
        })
        .find(|method| method.js_name == js_name)
        .unwrap_or_else(|| panic!("missing method {js_name}"))
}
