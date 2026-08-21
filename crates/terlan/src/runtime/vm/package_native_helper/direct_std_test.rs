use super::{
    call, native_handle_from_store, native_handle_value, supports, typed_result_error_name,
};
use crate::runtime::native_boundary::resource::{ResourceStore, ResourceValue};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::ReplValue;

const OWNER_PROCESS_ID: u64 = 7;

#[test]
fn supports_http_and_uri_operations() {
    assert!(supports("std.encoding.base64.encode"));
    assert!(supports("std.encoding.base64.decode"));
    assert!(supports("std.http.request.body_json"));
    assert!(supports("std.http.request.body_text"));
    assert!(supports("std.http.request.param"));
    assert!(supports("std.http.request.query"));
    assert!(supports("std.http.response.json"));
    assert!(supports("std.http.response.redirect"));
    assert!(supports("std.http.cookies.set_header"));
    assert!(supports("std.http.cookies.set_header_with_options"));
    assert!(supports("std.http.cookies.delete_header"));
    assert!(supports("std.net.uri.parse"));
    assert!(supports("std.net.uri.to_string"));
    assert!(supports("std.package.registry.parse_publish_request"));
    assert!(supports("std.package.registry.parse_yank_request"));

    assert!(!supports("std.http.cookies.set"));
    assert!(!supports("std.http.request.unknown"));
    assert!(!supports("std.http.response.stream"));
    assert!(!supports("std.http.response.status"));
}

#[test]
fn typed_result_error_names_cover_new_http_and_uri_paths() {
    assert_eq!(
        typed_result_error_name("std.encoding.base64.decode"),
        Some("Base64Error")
    );
    assert_eq!(typed_result_error_name("std.encoding.base64.encode"), None);
    assert_eq!(
        typed_result_error_name("std.http.request.body_json"),
        Some("HttpError")
    );
    assert_eq!(
        typed_result_error_name("std.http.cookies.set_header"),
        Some("HttpError")
    );
    assert_eq!(
        typed_result_error_name("std.http.cookies.set_header_with_options"),
        Some("HttpError")
    );
    assert_eq!(
        typed_result_error_name("std.http.cookies.delete_header"),
        Some("HttpError")
    );
    assert_eq!(
        typed_result_error_name("std.net.uri.parse"),
        Some("UriError")
    );
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
    let mut resources = ResourceStore::new();
    let encoded = call(
        &mut resources,
        OWNER_PROCESS_ID,
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

    let invalid = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.encoding.base64.decode".to_string(),
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
                [(field, ReplValue::Record { name, .. })]
                    if field == "reason" && name == "Base64Error"
            )
    ));
}

#[test]
fn native_handle_from_store_includes_http_uri_handles() {
    let mut resources = ResourceStore::new();
    let request_handle = resources
        .insert_for_owner(
            OWNER_PROCESS_ID,
            ResourceValue::HttpRequest(crate::runtime::native::http::Request::from_parts(
                "GET", "/", "",
            )),
        )
        .expect("request inserted into test store");
    let response_handle = resources
        .insert_for_owner(
            OWNER_PROCESS_ID,
            ResourceValue::HttpResponse(crate::runtime::native::http::Response::from_parts(
                200,
                "text/plain",
                "ok",
            )),
        )
        .expect("response inserted into test store");
    let uri_handle = resources
        .insert_for_owner(
            OWNER_PROCESS_ID,
            ResourceValue::Uri(
                crate::runtime::native::uri::parse("https://example.com/path?query=one")
                    .expect("uri parsed in test setup"),
            ),
        )
        .expect("uri inserted into test store");
    let jar_handle = resources
        .insert_for_owner(
            OWNER_PROCESS_ID,
            ResourceValue::HttpCookieJar(crate::runtime::native::http::CookieJar::from_pairs(
                vec![("theme".to_string(), "dark".to_string())],
            )),
        )
        .expect("cookie jar inserted into test store");

    assert_eq!(
        native_handle_from_store(&resources, OWNER_PROCESS_ID, request_handle)
            .expect("request handle projected")
            .type_name(),
        "std.http.Request.Request"
    );
    assert_eq!(
        native_handle_from_store(&resources, OWNER_PROCESS_ID, response_handle)
            .expect("response handle projected")
            .type_name(),
        "std.http.Response.Response"
    );
    assert_eq!(
        native_handle_from_store(&resources, OWNER_PROCESS_ID, uri_handle)
            .expect("uri handle projected")
            .type_name(),
        "std.net.Uri.Uri"
    );
    assert_eq!(
        native_handle_from_store(&resources, OWNER_PROCESS_ID, jar_handle)
            .expect("cookie jar handle projected")
            .type_name(),
        "std.http.Cookies.Jar"
    );
}

#[test]
fn call_supports_http_request_and_uri_paths() {
    let mut resources = ResourceStore::new();
    let request = ResourceValue::HttpRequest(
        crate::runtime::native::http::Request::from_parts_with_metadata(
            "POST",
            "/users/42",
            "{\"id\": 7}",
            vec![("id".to_string(), "42".to_string())],
            vec![("filter".to_string(), "active".to_string())],
            vec![("theme".to_string(), "dark".to_string())],
        ),
    );
    let handle = resources
        .insert_for_owner(OWNER_PROCESS_ID, request)
        .expect("http request inserted");
    let request_record = native_handle_value(OWNER_PROCESS_ID, handle, "std.http.Request.Request")
        .expect("test request record built");

    let path = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.http.request.path".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![request_record.clone()]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("path call succeeded");
    assert_eq!(path, ReplValue::String("/users/42".to_string()));

    let param = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.http.request.param".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![
                request_record.clone(),
                ReplValue::String("id".to_string()),
            ]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("param call succeeded");
    assert_eq!(
        param,
        ReplValue::Record {
            name: "Some".to_string(),
            fields: vec![("value".to_string(), ReplValue::String("42".to_string()))],
        }
    );

    let body_json = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.http.request.body_json".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![request_record]),
            result_type: TvmBoundaryType::Json,
        },
    )
    .expect("body_json call succeeded");
    assert!(matches!(body_json, ReplValue::Record { name, .. } if name == "Ok"));

    let uri = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.net.uri.parse".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![ReplValue::String(
                "https://example.com/path?filter=active#section".to_string(),
            )]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("uri parse call succeeded");

    let ReplValue::Record { name, fields } = uri else {
        panic!("URI parse result should be a record");
    };
    assert_eq!(name, "Ok");
    let Some((field, ReplValue::Record { name, fields })) = fields.first() else {
        panic!("URI parse success should contain a URI record");
    };
    assert_eq!(field, "value");
    assert_eq!(name, "Uri");
    assert!(fields.iter().any(|(field, value)| {
        field == "$native_owner" && matches!(value, ReplValue::String(value) if value == "7")
    }));
    assert!(fields.iter().any(|(field, value)| {
        field == "$native_type"
            && matches!(value, ReplValue::String(value) if value == "std.net.Uri.Uri")
    }));
    let typed_error = call(
        &mut resources,
        OWNER_PROCESS_ID,
        &PureNativeCapabilityRequest {
            capability: "package-native".to_string(),
            operation: "std.net.uri.parse".to_string(),
            arguments: Vec::new(),
            package_arguments: Some(vec![ReplValue::String("%%".to_string())]),
            result_type: TvmBoundaryType::String,
        },
    )
    .expect("uri parse typed error is wrapped");
    assert!(matches!(
        typed_error,
        ReplValue::Record { name, fields } if name == "Err"
            && matches!(
                fields.as_slice(),
                [(field, ReplValue::Record { name, .. })]
                    if field == "reason" && name == "UriError"
            )
    ));
}

impl ReplValue {
    fn type_name(&self) -> &str {
        match self {
            ReplValue::Record { name: _, fields } => fields
                .iter()
                .find_map(|(field, value)| {
                    if field == "$native_type" {
                        if let ReplValue::String(value) = value {
                            return Some(value.as_str());
                        }
                    }
                    None
                })
                .unwrap_or(""),
            _ => "",
        }
    }
}
