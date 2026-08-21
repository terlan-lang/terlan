//! Regression coverage for native-package boundary type canonicalization.

use std::collections::{HashMap, HashSet};

use crate::runtime::native_image::managed::decode_aggregate_layout;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};
use crate::terlan_typeck::{CoreStructTypeField, CoreType};

use super::native_packages::{
    native_handle_layouts, native_package_aliases, native_transparent_record_layouts,
    resolve_imported_native_package_type,
};

#[test]
fn imported_alias_resolution_can_canonicalize_a_nested_result_field() {
    let error = CoreType::Struct {
        name: "std.core.Error.Error".to_string(),
        fields: vec![
            CoreStructTypeField {
                name: "code".to_string(),
                ty: CoreType::Atom,
                is_private: false,
            },
            CoreStructTypeField {
                name: "message".to_string(),
                ty: CoreType::String,
                is_private: false,
            },
        ],
    };
    let aliases = HashMap::from([(
        "std.core.Error.Error".to_string(),
        ("std.core.Error".to_string(), error.clone()),
    )]);
    let boundary = CoreType::Apply {
        constructor: "Result".to_string(),
        args: vec![CoreType::Int, CoreType::Named("Error".to_string())],
    };

    let resolved = resolve_imported_native_package_type(
        &boundary,
        "package.Adapter",
        &["std.core.Error".to_string()],
        &aliases,
        &mut HashSet::new(),
    )
    .expect("resolve imported Error");

    assert_eq!(
        resolved,
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![CoreType::Int, error],
        }
    );
}

#[test]
fn transparent_package_record_admits_its_named_compatibility_layout() {
    let syntax = parse_module_as_syntax_output(
        "module package.Record.\n\npub struct Row { name: String }.\n",
    )
    .expect("parse record package");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    super::super::nominal_identity::qualify_local_nominal_types(&mut core);

    let layouts = native_transparent_record_layouts(&core).expect("nominal record layouts");
    let canonicals = layouts
        .iter()
        .map(|layout| {
            decode_aggregate_layout(layout)
                .expect("decode")
                .canonical_type()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert!(canonicals.contains(&"Named(package.Record.Row)".to_string()));
}

#[test]
fn opaque_value_alias_keeps_storage_while_bodyless_opaque_uses_handle() {
    let syntax = parse_module_as_syntax_output(
        "module package.Values.\n\
         pub opaque type Token = String.\n\
         pub opaque type Resource.\n",
    )
    .expect("parse opaque package types");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    super::super::nominal_identity::qualify_local_nominal_types(&mut core);

    let aliases = native_package_aliases(std::slice::from_ref(&core));
    assert_eq!(
        aliases["package.Values.Token"].1,
        CoreType::String,
        "opaque aliases with bodies are private value representations"
    );
    assert!(matches!(
        aliases["package.Values.Resource"].1,
        CoreType::Struct { ref name, .. } if name == "package.Values.Resource"
    ));

    let layouts = native_handle_layouts(&core).expect("opaque resource layouts");
    let canonicals = layouts
        .iter()
        .map(|layout| {
            decode_aggregate_layout(layout)
                .expect("decode")
                .canonical_type()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(canonicals, ["package.Values.Resource"]);
}

#[test]
fn template_html_uses_compiler_managed_string_representation() {
    let syntax =
        parse_module_as_syntax_output("module std.template.Template.\n\npub opaque type Html.\n")
            .expect("parse template facade");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    super::super::nominal_identity::qualify_local_nominal_types(&mut core);

    let aliases = native_package_aliases(std::slice::from_ref(&core));
    assert!(!aliases.contains_key("std.template.Template.Html"));
    assert!(
        native_handle_layouts(&core)
            .expect("template layouts")
            .is_empty(),
        "Template.Html must not acquire a native capability-handle layout"
    );
}

#[test]
fn http_request_uses_compiler_managed_tuple_representation() {
    let syntax =
        parse_module_as_syntax_output("module std.http.Request.\n\npub opaque type Request.\n")
            .expect("parse request facade");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    super::super::nominal_identity::qualify_local_nominal_types(&mut core);

    let aliases = native_package_aliases(std::slice::from_ref(&core));
    assert!(!aliases.contains_key("std.http.Request.Request"));
    assert!(
        native_handle_layouts(&core)
            .expect("request layouts")
            .is_empty(),
        "Request must not acquire a native capability-handle layout"
    );
}
