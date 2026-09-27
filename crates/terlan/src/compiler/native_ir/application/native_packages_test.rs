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
    assert_eq!(canonicals, ["Named(package.Values.Resource)"]);
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
fn http_values_keep_managed_storage_without_exempting_package_namesakes() {
    for (module, name, managed) in [
        ("std.http.Request", "Request", true),
        ("std.http.Response", "Response", true),
        ("std.http.Cookies", "Jar", true),
        ("std.http.Session", "Session", true),
        ("app.Cookies", "Jar", false),
        ("app.Session", "Session", false),
    ] {
        let syntax = parse_module_as_syntax_output(&format!(
            "module {module}.\n\npub opaque type {name}.\n"
        ))
        .expect("parse HTTP facade or package namesake");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
        super::super::nominal_identity::qualify_local_nominal_types(&mut core);

        let aliases = native_package_aliases(std::slice::from_ref(&core));
        assert_eq!(aliases.contains_key(&format!("{module}.{name}")), !managed);
        assert_eq!(
            native_handle_layouts(&core)
                .expect("HTTP layouts")
                .is_empty(),
            managed,
            "wrong capability-handle layout for {module}.{name}"
        );
    }
}

#[test]
fn collections_keep_managed_storage_without_exempting_package_namesakes() {
    for (name, parameters) in [
        ("List", "T"),
        ("Map", "K, V"),
        ("Set", "T"),
        ("Iterator", "T"),
    ] {
        for owner in ["std.collections", "package"] {
            let module = format!("{owner}.{name}");
            let syntax = parse_module_as_syntax_output(&format!(
                "module {module}. pub opaque type {name}[{parameters}]."
            ))
            .expect("parse opaque collection declaration");
            let resolved = resolve_syntax_module_output(&syntax).module;
            let core = lower_syntax_module_output_to_core(&syntax, &resolved);
            let aliases = native_package_aliases(std::slice::from_ref(&core));
            let layouts = native_handle_layouts(&core).expect("collection layouts");
            let expected_handles = usize::from(owner == "package");
            assert_eq!(aliases.len(), expected_handles, "{module}");
            assert_eq!(layouts.len(), expected_handles, "{module}");
        }
    }
}

#[test]
fn vm_buffers_keep_managed_storage_without_exempting_package_namesakes() {
    for name in ["Bytes", "BitString"] {
        for owner in ["std.vm", "package"] {
            let module = format!("{owner}.{name}");
            let syntax =
                parse_module_as_syntax_output(&format!("module {module}. pub opaque type {name}."))
                    .expect("parse opaque buffer declaration");
            let resolved = resolve_syntax_module_output(&syntax).module;
            let core = lower_syntax_module_output_to_core(&syntax, &resolved);
            let aliases = native_package_aliases(std::slice::from_ref(&core));
            let layouts = native_handle_layouts(&core).expect("buffer layouts");
            let expected_handles = usize::from(owner == "package");
            assert_eq!(aliases.len(), expected_handles, "{module}");
            assert_eq!(layouts.len(), expected_handles, "{module}");
        }
    }
}

#[test]
fn vm_tokens_keep_intrinsic_storage_without_exempting_package_namesakes() {
    let declarations = "\
        pub opaque type Process[T].\n\
        pub opaque type Entry[T].\n\
        pub opaque type Timer.\n\
        pub opaque type Monitor[T].\n\
        pub opaque type ResourceKind[T].\n\
        pub opaque type Resource[T].\n\
        pub opaque type ExitReason.\n\
        pub opaque type SchedulingClass.\n";
    for module in ["std.vm.Process", "package.Process"] {
        let syntax = parse_module_as_syntax_output(&format!("module {module}.\n{declarations}"))
            .expect("parse opaque token declarations");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
        let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
        super::super::nominal_identity::qualify_local_nominal_types(&mut core);
        let aliases = native_package_aliases(std::slice::from_ref(&core));
        let layouts = native_handle_layouts(&core).expect("opaque token layouts");
        if module == "std.vm.Process" {
            assert!(
                aliases.is_empty(),
                "VM tokens must retain their intrinsic ABI"
            );
            assert!(
                layouts.is_empty(),
                "VM tokens are not worker resource handles"
            );
        } else {
            assert_eq!(aliases.len(), 8);
            assert_eq!(layouts.len(), 8);
            for name in ["Timer", "ExitReason", "SchedulingClass"] {
                assert!(matches!(
                    &aliases[&format!("{module}.{name}")].1,
                    CoreType::Struct { name: canonical, .. }
                        if canonical == &format!("{module}.{name}")
                ));
            }
        }
    }
}

#[test]
fn generic_native_handles_preserve_type_arguments_until_specialization() {
    use super::native_packages::canonicalize_native_package_types;
    let syntax = parse_module_as_syntax_output(
        "module package.Vector.\npub opaque type Vector[T].\npub keep(value: Vector[Int]): Vector[Int] -> value.\n",
    ).expect("parse generic resource");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let serialized = serde_json::to_string(&core).expect("serialize opaque resource");
    let mut cores = vec![serde_json::from_str(&serialized).expect("restore resource CoreIR")];
    super::super::nominal_identity::qualify_application_nominal_types(&mut cores);
    let aliases = native_package_aliases(&cores);
    let original = cores[0].functions[0].core_return_type.clone();
    assert!(matches!(&original, Some(CoreType::Apply { args, .. }) if args == &[CoreType::Int]));
    canonicalize_native_package_types(&mut cores, &aliases, false).expect("early identities");
    assert_eq!(cores[0].functions[0].core_return_type, original);
    canonicalize_native_package_types(&mut cores, &aliases, true).expect("late handle storage");
    let result = cores[0].functions[0].core_return_type.as_ref().unwrap();
    assert!(matches!(result, CoreType::Struct { name, fields }
        if name == "package.Vector.Vector" && fields.len() == 4));
    assert_eq!(
        cores[0].functions[0].params[0].core_ty.as_ref(),
        Some(result)
    );
    let layouts = super::super::aggregate_types::managed_aggregate_layouts([result])
        .expect("admit opaque handle");
    let descriptor = decode_aggregate_layout(&layouts[0]).expect("decode handle layout");
    assert_eq!(
        super::super::native_type(Some(result), &result.contract_text()),
        Some(super::super::NativeType::ManagedRef(
            descriptor.managed().semantic_id()
        )),
    );
}

#[test]
fn mailbox_message_uses_payload_storage_without_exempting_package_namesakes() {
    for owner in ["std.vm", "package"] {
        let module = format!("{owner}.Message");
        let syntax = parse_module_as_syntax_output(&format!(
            "module {module}. pub opaque type Message[T]. pub keep(value: Message[Int]): Message[Int] -> value."
        )).expect("parse opaque mailbox declaration");
        let resolved = resolve_syntax_module_output(&syntax).module;
        let core = lower_syntax_module_output_to_core(&syntax, &resolved);
        let mut cores = vec![core];
        super::super::nominal_identity::qualify_application_nominal_types(&mut cores);
        let aliases = native_package_aliases(&cores);
        super::native_packages::canonicalize_native_package_types(&mut cores, &aliases, true)
            .expect("late native package admission");
        let result = cores[0].functions[0].core_return_type.as_ref().unwrap();
        if owner == "std.vm" {
            assert_eq!(
                super::super::native_type(Some(result), &result.contract_text()),
                Some(super::super::NativeType::Int)
            );
            assert!(native_handle_layouts(&cores[0]).unwrap().is_empty());
        } else {
            assert!(matches!(result, CoreType::Struct { .. }));
            assert_eq!(native_handle_layouts(&cores[0]).unwrap().len(), 1);
        }
    }
}
