use super::{call, supported_handle_type, supports, typed_result_error_name};
use crate::runtime::native_boundary::resource::{ResourceStore, ResourceValue};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::ReplValue;

const OWNER_PROCESS_ID: u64 = 7;

#[test]
fn compiled_json_projections_cross_the_package_boundary_as_source_records() {
    super::super::source_test_support::assert_source_checks(
        "json_projection_source",
        r#"
module json_projection_source.
import std.data.Json.
import std.core.Result.{Ok, Err}.
import std.core.Option.{Some, None}.

pub optional_rows(): Bool ->
    case Json.parse("[{\"name\":\"\"},false]") {
        Ok(json) -> case json.string_field_rows(["name", "missing", "name"]) {
            Ok([first, second]) -> first.object and not second.object
                and first.values == [Some(""), None, Some("")]
                and second.values == [None, None, None];
            _ -> false
        };
        _ -> false
    }.

pub nested_rows(): Bool ->
    case Json.parse("[{\"name\":\"parent\",\"children\":[{\"name\":\"child\"},null]}]") {
        Ok(json) -> case json.nested_string_field_rows(["name"], "children", ["name"]) {
            Ok([parent]) -> case parent.children {
                [first, second] -> parent.object and parent.child_array
                    and parent.values == [Some("parent")]
                    and first.object and first.values == [Some("child")]
                    and not second.object and second.values == [None];
                _ -> false
            };
            _ -> false
        };
        _ -> false
    }.

pub required_columns(): Bool ->
    case Json.parse("{\"name\":\"Ada\",\"count\":-9,\"items\":[1,2]}") {
        Ok(json) -> case json.required_fields(["name", "name"], ["count"], ["items"]) {
            Ok(row) -> row.strings == ["Ada", "Ada"] and row.ints == [-9] and row.array_lengths == [2];
            _ -> false
        };
        _ -> false
    }.

pub required_rows(): Bool ->
    case Json.parse("[{\"name\":\"Ada\",\"count\":9,\"active\":false}]") {
        Ok(json) -> case json.required_field_rows(["name"], ["count"], ["active"]) {
            Ok([row]) -> row.strings == ["Ada"] and row.ints == [9] and row.bools == [false];
            _ -> false
        };
        _ -> false
    }.
"#,
        &[
            "optional_rows",
            "nested_rows",
            "required_columns",
            "required_rows",
        ],
    );
}

#[test]
fn json_package_contract_matches_parsed_source_signatures() {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreType};
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(include_str!(
        "../../../../../../std/data/Json.terl"
    ))
    .unwrap();
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut seen = std::collections::BTreeSet::new();
    for function in &core.functions {
        let Some(operation) = &function.native_operation else {
            continue;
        };
        assert!(seen.insert(operation.as_str()));
        let contract = crate::std_native_packages::resource_operation(operation).unwrap();
        assert_eq!(contract.arity, function.arity, "{operation}");
        let output = match function.core_return_type.as_ref().unwrap() {
            CoreType::Apply { constructor, args } if constructor == "Result" => &args[0],
            output => output,
        };
        assert_eq!(contract.resource_type, "std.data.Json.Json");
        assert_eq!(
            contract.returns_resource,
            matches!(output, CoreType::Named(name) if name == "Json" || name == "std.data.Json.Json"),
            "{operation}: {output:?}"
        );
        assert_eq!(
            contract.mutates_receiver, function.receiver_mutable,
            "{operation}"
        );
        let result_error = match function.core_return_type.as_ref().unwrap() {
            CoreType::Apply { constructor, args } if constructor == "Result" => {
                assert_eq!(args.len(), 2);
                let CoreType::Named(error) = &args[1] else {
                    panic!("named source error required");
                };
                Some(error.as_str())
            }
            _ => None,
        };
        assert_eq!(contract.result_error, result_error, "{operation}");
    }
    let registered: std::collections::BTreeSet<_> = crate::std_native_packages::RESOURCE_OPERATIONS
        .iter()
        .flat_map(|group| group.iter())
        .map(|contract| contract.operation)
        .collect();
    assert_eq!(seen, registered);
}

#[test]
fn package_resource_contracts_control_arity_and_result_wrapping() {
    for contract in crate::std_native_packages::RESOURCE_OPERATIONS
        .iter()
        .flat_map(|group| group.iter())
    {
        assert!(supports(contract.operation));
        assert_eq!(
            typed_result_error_name(contract.operation),
            contract.result_error
        );
        assert_eq!(
            crate::terlan_native_boundary::dispatch::operation_arity(contract.operation),
            Some(contract.arity)
        );
        for actual in [contract.arity + 1, usize::MAX] {
            let error = crate::terlan_native_boundary::dispatch::validate_operation_arity(
                contract.operation,
                actual,
                |_| panic!("registered operation must not be unknown"),
            )
            .unwrap_err();
            assert_eq!(error.code(), "dispatch.arity");
        }
    }
    for unknown in [
        "std.data.json.unknown",
        "std.data.json.parse.extra",
        "app.data.json.parse",
        "std.data.json.",
    ] {
        assert!(!supports(unknown));
        assert_eq!(typed_result_error_name(unknown), None);
        assert_eq!(
            crate::terlan_native_boundary::dispatch::operation_arity(unknown),
            None
        );
    }
}

#[test]
fn native_handle_types_admit_only_direct_std_resources() {
    assert!(supported_handle_type("std.regex.Regex.Regex"));
    assert!(!supported_handle_type("std.http.Request.Request"));
    assert!(!supported_handle_type("std.regex.Regex.Other"));
    assert!(!supported_handle_type("third.party.Handle"));
}

#[test]
fn direct_random_preserves_generator_tuples_and_typed_errors() {
    let mut store = ResourceStore::new();
    let invoke = |store: &mut ResourceStore, method: &str, args| {
        let operation = format!("std.random.random.{method}");
        assert!(supports(&operation));
        call(
            store,
            OWNER_PROCESS_ID,
            &PureNativeCapabilityRequest {
                capability: "package-native".into(),
                operation,
                arguments: vec![],
                package_arguments: Some(args),
                result_type: TvmBoundaryType::Unit,
            },
        )
        .unwrap()
    };
    let ReplValue::Record { name, fields } = invoke(&mut store, "seed", vec![ReplValue::Int(42)])
    else {
        panic!("typed result")
    };
    assert_eq!(name, "Ok");
    let generator = fields[0].1.clone();
    let first = invoke(&mut store, "int", vec![generator.clone()]);
    let repeated = invoke(&mut store, "int", vec![generator.clone()]);
    let (ReplValue::Tuple(first), ReplValue::Tuple(repeated)) = (first, repeated) else {
        panic!("tuple draws")
    };
    assert_eq!(first[1], repeated[1]);
    assert!(matches!(&first[0], ReplValue::Record { name, .. } if name == "Generator"));
    let ReplValue::Record { name, fields } = invoke(
        &mut store,
        "bounded_int",
        vec![generator, ReplValue::Int(1), ReplValue::Int(1)],
    ) else {
        panic!("typed error")
    };
    assert_eq!(name, "Err");
    let ReplValue::Record { name, fields } = &fields[0].1 else {
        panic!("random error")
    };
    assert_eq!(name, "RandomError");
    assert!(fields.contains(&(
        "code".into(),
        ReplValue::Atom("random.invalid_bounds".into())
    )));
    assert!(!supports("std.random.random.unknown"));
}

/// Pure Rust MD5 runs through the same owner-local dispatcher as other safe codecs.
#[test]
fn call_supports_direct_md5_without_a_std_package_helper() {
    let operation = "std.encoding.md5.digest";
    assert!(!supports(operation));
    assert!(!supports("std.encoding.md5.unknown"));
    assert_eq!(typed_result_error_name(operation), None);
    let value = super::super::value_package::call(
        super::super::value_package::binding(operation).unwrap(),
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: operation.to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![ReplValue::String("abc".to_string())]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("MD5 digest succeeded");
    assert_eq!(
        value,
        ReplValue::String("900150983cd24fb0d6963f7d28e17f72".to_string())
    );
}

#[test]
fn supports_http_and_uri_operations() {
    assert!(!supports("std.encoding.base64.encode"));
    assert!(!supports("std.encoding.base64.decode"));
    assert!(!supports("std.http.request.body_json"));
    assert!(!supports("std.http.request.body_text"));
    assert!(!supports("std.http.request.param"));
    assert!(!supports("std.http.request.query"));
    assert!(!supports("std.http.response.json"));
    assert!(supports("std.data.json.to_string"));
    assert!(!supports("std.http.response.redirect"));
    assert!(!supports("std.http.cookies.set_header"));
    assert!(!supports("std.http.cookies.set_header_with_options"));
    assert!(!supports("std.http.cookies.delete_header"));
    assert!(!supports("std.net.uri.parse"));
    assert!(!supports("std.net.uri.to_string"));
    assert!(supports("std.package.registry.parse_publish_request"));
    assert!(supports("std.package.registry.parse_yank_request"));

    assert!(!supports("std.http.cookies.set"));
    assert!(!supports("std.http.request.unknown"));
    assert!(!supports("std.http.response.stream"));
    assert!(!supports("std.http.response.status"));
}

#[test]
fn typed_result_error_names_cover_new_http_and_uri_paths() {
    assert_eq!(typed_result_error_name("std.encoding.base64.decode"), None);
    assert_eq!(typed_result_error_name("std.encoding.base64.encode"), None);
    assert_eq!(typed_result_error_name("std.http.request.body_json"), None);
    assert_eq!(typed_result_error_name("std.http.cookies.set_header"), None);
    assert_eq!(
        typed_result_error_name("std.http.cookies.set_header_with_options"),
        None
    );
    assert_eq!(
        typed_result_error_name("std.http.cookies.delete_header"),
        None
    );
    assert_eq!(typed_result_error_name("std.net.uri.parse"), None);
    assert_eq!(
        typed_result_error_name("std.data.json.parse"),
        Some("JsonError")
    );
    assert_eq!(
        typed_result_error_name("std.package.registry.parse_publish_request"),
        Some("RegistryProtocolError")
    );
    assert_eq!(
        typed_result_error_name("std.package.registry.parse_yank_request"),
        Some("RegistryProtocolError")
    );
    assert!(typed_result_error_name("std.io.path.join").is_some());
    assert_eq!(typed_result_error_name("std.http.response.text"), None);
}

#[test]
fn call_supports_direct_base64_without_a_std_package_helper() {
    let encoded = super::super::value_package::call(
        super::super::value_package::binding("std.encoding.base64.encode").unwrap(),
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.encoding.base64.encode".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![ReplValue::String("Terlan".to_string())]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("base64 encode succeeded");
    assert_eq!(encoded, ReplValue::String("VGVybGFu".to_string()));

    let invalid = super::super::value_package::call(
        super::super::value_package::binding("std.encoding.base64.decode_text").unwrap(),
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.encoding.base64.decode_text".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![ReplValue::String("%%%".to_string())]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("base64 typed error returned");
    assert!(matches!(
        invalid,
        ReplValue::Record { name, fields } if name == "Err"
            && matches!(
                fields.as_slice(),
                [(field, ReplValue::Tuple(values))]
                    if field == "reason" && matches!(values.as_slice(), [ReplValue::Bool(false), ReplValue::String(message)] if !message.is_empty())
            )
    ));
}

#[test]
fn retired_response_handles_are_not_admitted_even_with_live_resource_ids() {
    let mut resources = ResourceStore::new();
    let handle = resources
        .insert_for_owner(
            OWNER_PROCESS_ID,
            ResourceValue::Json(crate::terlan_native::json::null()),
        )
        .unwrap();
    let before = resources.clone();
    let value =
        super::native_handle_value(OWNER_PROCESS_ID, handle, "std.http.Response.Response").unwrap();
    assert!(!supported_handle_type("std.http.Response.Response"));
    for argument in [
        value.clone(),
        ReplValue::List(vec![value.clone()]),
        ReplValue::Tuple(vec![value]),
    ] {
        let error = call(
            &mut resources,
            OWNER_PROCESS_ID,
            &PureNativeCapabilityRequest {
                capability: "package-native".into(),
                operation: "std.data.json.render".into(),
                arguments: vec![],
                package_arguments: Some(vec![argument]),
                result_type: TvmBoundaryType::String,
            },
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported handle type `std.http.Response.Response`"));
        assert_eq!(resources, before);
    }
}

#[test]
fn request_source_methods_cannot_be_dispatched_as_native_operations() {
    for method in [
        "body_file_path",
        "body_text",
        "body_json",
        "method",
        "path",
        "param",
        "query",
        "query_string",
        "header",
        "cookie",
        "cookies",
    ] {
        let operation = format!("std.http.request.{method}");
        assert!(!supports(&operation), "{operation}");
        let error = call(
            &mut ResourceStore::new(),
            OWNER_PROCESS_ID,
            &PureNativeCapabilityRequest {
                capability: "package-native".into(),
                operation,
                arguments: vec![],
                package_arguments: Some(vec![]),
                result_type: TvmBoundaryType::Unit,
            },
        )
        .expect_err("removed Request entry point must not execute");
        assert!(
            error.to_string().contains("dispatch.unknown_operation"),
            "{error}"
        );
    }
}

#[test]
fn source_cookie_jar_has_no_native_operations_or_handle_type() {
    assert!(!supported_handle_type("std.http.Cookies.Jar"));
    for operation in [
        "std.http.cookies.get",
        "std.http.cookies.set",
        "std.http.cookies.delete",
        "std.http.cookies.headers",
        "std.http.response.with_cookies",
    ] {
        assert!(!supports(operation), "{operation}");
        let error = call(
            &mut ResourceStore::new(),
            OWNER_PROCESS_ID,
            &PureNativeCapabilityRequest {
                capability: "package-native".into(),
                operation: operation.into(),
                arguments: vec![],
                package_arguments: Some(vec![]),
                result_type: TvmBoundaryType::Unit,
            },
        )
        .expect_err("source operation must not dispatch natively");
        assert!(
            error.to_string().contains("dispatch.unknown_operation"),
            "{error}"
        );
    }
}
