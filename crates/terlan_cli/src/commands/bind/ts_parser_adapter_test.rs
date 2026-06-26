use super::ts_parser_adapter::*;
use super::ts_type_mapping::{TsPrimitiveType, TsTypeRef};

/// Verifies the committed DOM fixture parses into the neutral declaration model.
///
/// Inputs:
/// - `std/js/dom/fixtures/document.d.ts`.
///
/// Output:
/// - Test passes when Oxc accepts the fixture and the adapter extracts
///   `Document` plus `HTMLElement` interfaces.
///
/// Transformation:
/// - Pins T0.2 to real `.d.ts` parsing without introducing the public generator
///   command before T0.5.
#[test]
fn parses_committed_dom_fixture_interfaces() {
    let source = include_str!("../../../../../std/js/dom/fixtures/document.d.ts");

    let declarations = parse_ts_declaration_file(source).expect("fixture should parse");

    assert_eq!(declarations.declarations.len(), 2);
    assert_eq!(
        interface_names(&declarations),
        vec!["Document", "HTMLElement"]
    );
}

/// Verifies readonly properties are preserved by the adapter.
///
/// Inputs:
/// - `Document.title` from the committed DOM fixture.
///
/// Output:
/// - Test passes when `title` is readonly, non-optional, and typed as string.
///
/// Transformation:
/// - Proves interface property metadata survives the Oxc-to-neutral conversion.
#[test]
fn parses_dom_fixture_readonly_property() {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");
    let document = interface(&declarations, "Document");

    let property = property(document, "title");

    assert!(property.readonly);
    assert!(!property.optional);
    assert_eq!(property.ty, TsTypeRef::Primitive(TsPrimitiveType::String));
}

/// Verifies method parameters and nullable return types are preserved.
///
/// Inputs:
/// - `Document.getElementById` from the committed DOM fixture.
///
/// Output:
/// - Test passes when the parameter and `HTMLElement | null` return shape are
///   represented in the neutral model.
///
/// Transformation:
/// - Pins the first DOM method-contract shape before wrapper emission is added.
#[test]
fn parses_dom_fixture_method_signature() {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");
    let document = interface(&declarations, "Document");

    let method = method(document, "getElementById");

    assert_eq!(method.params.len(), 1);
    assert_eq!(method.params[0].name, "elementId");
    assert_eq!(
        method.params[0].ty,
        TsTypeRef::Primitive(TsPrimitiveType::String)
    );
    assert_eq!(
        method.return_type,
        TsTypeRef::Union(vec![
            TsTypeRef::Named("HTMLElement".to_string()),
            TsTypeRef::Null
        ])
    );
}

/// Verifies nullable mutable properties are preserved by the adapter.
///
/// Inputs:
/// - `HTMLElement.textContent` from the committed DOM fixture.
///
/// Output:
/// - Test passes when `textContent` is mutable and typed as `string | null`.
///
/// Transformation:
/// - Confirms nullable property typing is represented before Terlan `Option`
///   mapping is applied.
#[test]
fn parses_dom_fixture_nullable_property() {
    let declarations = parse_ts_declaration_file(include_str!(
        "../../../../../std/js/dom/fixtures/document.d.ts"
    ))
    .expect("fixture should parse");
    let element = interface(&declarations, "HTMLElement");

    let property = property(element, "textContent");

    assert!(!property.readonly);
    assert_eq!(
        property.ty,
        TsTypeRef::Union(vec![
            TsTypeRef::Primitive(TsPrimitiveType::String),
            TsTypeRef::Null
        ])
    );
}

/// Verifies Oxc parser diagnostics become stable adapter errors.
///
/// Inputs:
/// - Invalid TypeScript declaration text.
///
/// Output:
/// - Test passes when the adapter reports `ts_bindgen.parse_failed`.
///
/// Transformation:
/// - Keeps syntax failures distinct from supported-Oxc but unsupported-generator
///   shapes.
#[test]
fn reports_parse_failure_with_stable_reason() {
    let err = parse_ts_declaration_file("interface Broken { title: }")
        .expect_err("invalid TypeScript should fail");

    assert_eq!(err.reason, "ts_bindgen.parse_failed");
    assert!(!err.message.is_empty());
}

/// Verifies parser support for generic, callback, and record type shapes.
///
/// Inputs:
/// - Inline `.d.ts` source containing `Promise<string>`, a callback field, and
///   an object type literal.
///
/// Output:
/// - Test passes when all three shapes lower into the neutral mapper model.
///
/// Transformation:
/// - Pins the parser side of the T0.3 mapping contract without running the
///   public binding generator command.
#[test]
fn parses_generic_callback_and_record_type_shapes() {
    let declarations = parse_ts_declaration_file(
        r#"
        interface AsyncThing {
          ready: Promise<string>;
          onReady: (value: string) => void;
          meta: { id: string; count?: number };
        }
        "#,
    )
    .expect("inline declarations should parse");
    let async_thing = interface(&declarations, "AsyncThing");

    assert_eq!(
        property(async_thing, "ready").ty,
        TsTypeRef::Generic {
            name: "Promise".to_string(),
            args: vec![TsTypeRef::Primitive(TsPrimitiveType::String)]
        }
    );
    assert_eq!(
        property(async_thing, "onReady").ty,
        TsTypeRef::Callback {
            params: vec![TsTypeRef::Primitive(TsPrimitiveType::String)],
            return_type: Box::new(TsTypeRef::Primitive(TsPrimitiveType::Void))
        }
    );
    assert_eq!(
        property(async_thing, "meta").ty,
        TsTypeRef::Record(vec![
            super::ts_type_mapping::TsRecordField {
                name: "id".to_string(),
                optional: false,
                ty: TsTypeRef::Primitive(TsPrimitiveType::String),
            },
            super::ts_type_mapping::TsRecordField {
                name: "count".to_string(),
                optional: true,
                ty: TsTypeRef::Primitive(TsPrimitiveType::Number),
            },
        ])
    );
}

/// Verifies unsupported top-level declarations retain concrete source names.
///
/// Inputs:
/// - Inline `.d.ts` source with named variable, type alias, function, class,
///   enum, and namespace declarations.
///
/// Output:
/// - Test passes when skip rows use declaration names instead of coarse kind
///   buckets.
///
/// Transformation:
/// - Pins generated skip-manifest provenance for broad TypeScript standard
///   library generation.
#[test]
fn labels_named_unsupported_top_level_declarations() {
    let declarations = parse_ts_declaration_file(
        r#"
        declare var MapConstructor: MapConstructor;
        type ArrayLike<T> = { length: number };
        declare function parseInt(value: string): number;
        declare class URLSearchParams {}
        declare enum ReadyState { Done }
        declare namespace Intl {}
        "#,
    )
    .expect("unsupported declarations should become skip rows");

    let skipped = unsupported_sources(&declarations);

    assert_eq!(
        skipped,
        vec![
            "MapConstructor",
            "ArrayLike",
            "parseInt",
            "URLSearchParams",
            "ReadyState",
            "Intl"
        ]
    );
}

/// Verifies complex top-level variable declarations stay conservative.
///
/// Inputs:
/// - Inline `.d.ts` source with one multi-binding variable declaration.
///
/// Output:
/// - Test passes when the adapter keeps the stable coarse `variable` label.
///
/// Transformation:
/// - Avoids assigning one misleading source symbol to unsupported declarations
///   that contain multiple source bindings.
#[test]
fn labels_multi_binding_top_level_variables_conservatively() {
    let declarations = parse_ts_declaration_file("declare var first: string, second: string;")
        .expect("unsupported variables should become skip rows");

    assert_eq!(unsupported_sources(&declarations), vec!["variable"]);
}

/// Verifies leading TypeScript JSDoc is preserved by the neutral model.
///
/// Inputs:
/// - Inline `.d.ts` source with interface, property, and method JSDoc blocks.
///
/// Output:
/// - Test passes when generated declarations carry normalized doc bodies.
///
/// Transformation:
/// - Uses Oxc comment attachment metadata so docs remain tied to the parsed
///   TypeScript declaration rather than recovered by ad hoc string matching.
#[test]
fn preserves_leading_jsdoc_for_interfaces_and_members() {
    let declarations = parse_ts_declaration_file(
        r#"
        /**
         * Stores values by key.
         */
        interface Dictionary {
          /**
           * Number of stored values.
           * @returns the current size.
           */
          readonly size: number;
          /**
           * Reads one value.
           * @param key lookup key.
           * @returns the associated value.
           */
          get(key: string): string | undefined;
        }
        "#,
    )
    .expect("documented declarations should parse");
    let dictionary = interface(&declarations, "Dictionary");

    assert_eq!(dictionary.doc.as_deref(), Some("Stores values by key."));
    assert_eq!(
        property(dictionary, "size").doc.as_deref(),
        Some("Number of stored values.\n@returns the current size.")
    );
    assert_eq!(
        method(dictionary, "get").doc.as_deref(),
        Some("Reads one value.\n@param key lookup key.\n@returns the associated value.")
    );
}

/// Returns interface names in declaration order.
///
/// Inputs:
/// - `declarations`: parsed neutral declaration file.
///
/// Output:
/// - Ordered interface names.
///
/// Transformation:
/// - Filters the current declaration enum into labels used by focused tests.
fn interface_names(declarations: &TsDeclarationFile) -> Vec<&str> {
    declarations
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            TsDeclaration::Interface(interface) => Some(interface.name.as_str()),
            TsDeclaration::Unsupported(_) => None,
        })
        .collect()
}

/// Returns unsupported top-level declaration source labels in source order.
///
/// Inputs:
/// - `declarations`: parsed neutral declaration file.
///
/// Output:
/// - Ordered unsupported source labels.
///
/// Transformation:
/// - Filters parser output into stable labels used by skip-manifest tests.
fn unsupported_sources(declarations: &TsDeclarationFile) -> Vec<&str> {
    declarations
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            TsDeclaration::Interface(_) => None,
            TsDeclaration::Unsupported(unsupported) => Some(unsupported.source.as_str()),
        })
        .collect()
}

/// Finds an interface by name.
///
/// Inputs:
/// - `declarations`: parsed neutral declaration file.
/// - `name`: interface name to find.
///
/// Output:
/// - Borrowed interface declaration.
///
/// Transformation:
/// - Panics in tests when the expected interface is absent.
fn interface<'a>(declarations: &'a TsDeclarationFile, name: &str) -> &'a TsInterfaceDeclaration {
    declarations
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            TsDeclaration::Interface(interface) => Some(interface),
            TsDeclaration::Unsupported(_) => None,
        })
        .find(|interface| interface.name == name)
        .unwrap_or_else(|| panic!("missing interface {name}"))
}

/// Finds a property by name.
///
/// Inputs:
/// - `interface`: parsed neutral interface.
/// - `name`: property name to find.
///
/// Output:
/// - Borrowed property declaration.
///
/// Transformation:
/// - Filters interface members and panics when the expected property is absent.
fn property<'a>(interface: &'a TsInterfaceDeclaration, name: &str) -> &'a TsPropertyDeclaration {
    interface
        .members
        .iter()
        .filter_map(|member| match member {
            TsInterfaceMember::Property(property) => Some(property),
            TsInterfaceMember::Method(_) => None,
            TsInterfaceMember::Unsupported(_) => None,
        })
        .find(|property| property.name == name)
        .unwrap_or_else(|| panic!("missing property {name}"))
}

/// Finds a method by name.
///
/// Inputs:
/// - `interface`: parsed neutral interface.
/// - `name`: method name to find.
///
/// Output:
/// - Borrowed method declaration.
///
/// Transformation:
/// - Filters interface members and panics when the expected method is absent.
fn method<'a>(interface: &'a TsInterfaceDeclaration, name: &str) -> &'a TsMethodDeclaration {
    interface
        .members
        .iter()
        .filter_map(|member| match member {
            TsInterfaceMember::Method(method) => Some(method),
            TsInterfaceMember::Property(_) => None,
            TsInterfaceMember::Unsupported(_) => None,
        })
        .find(|method| method.name == name)
        .unwrap_or_else(|| panic!("missing method {name}"))
}
