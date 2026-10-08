use crate::runtime::vm::ReplValue;
use terlan_http_native::{Request, RequestFieldProjection};

use super::{replace_vm_request_descriptor, vm_request_descriptor_owned};

fn request() -> Request {
    Request::from_parts_with_raw_query_metadata(
        "POST",
        "/items/7",
        "payload",
        terlan_http_native::RequestMetadata {
            params: vec![("id".to_string(), "7".to_string())],
            query_string: ("page=2").into(),
            query: vec![("page".to_string(), "2".to_string())],
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            cookies: vec![("session".to_string(), "abc".to_string())],
        },
    )
    .with_body_file_path("/tmp/terlan-upload")
}

#[test]
fn package_metadata_projection_and_late_source_projection_agree() {
    let params = vec![("id".into(), "7".into())];
    let mut headers = http::HeaderMap::new();
    headers.insert("X-Mode", http::HeaderValue::from_static("first"));
    headers.insert(
        "cookie",
        http::HeaderValue::from_static("user=Ada; user=Grace"),
    );
    let query = "name=Ada+Lovelace&name=Grace";
    let make_request = |projection| {
        Request::from_parts_with_raw_query_metadata(
            "GET",
            "/",
            "",
            terlan_http_native::RequestMetadata::from_http(projection, &params, query, &headers),
        )
    };
    let complete = make_request(RequestFieldProjection::Complete);
    for mask in 0..(1 << 11) {
        let projection = RequestFieldProjection::Fields(mask);
        assert_eq!(
            vm_request_descriptor_owned(make_request(projection).into_parts(), projection),
            vm_request_descriptor_owned(complete.clone().into_parts(), projection),
            "projection mask {mask}",
        );
    }
}

#[test]
fn borrowed_router_request_uses_the_same_source_record_as_owned_ingress() {
    let request = request();
    for params in [vec![], vec![("route".into(), "changed".into())]] {
        let mut parts = request.clone().into_parts();
        parts.params = params.clone();
        let expected = vm_request_descriptor_owned(parts, RequestFieldProjection::Complete);
        assert_eq!(
            super::super::vm_request_descriptor(&request, &params),
            expected
        );
    }
    assert_eq!(request.into_parts().params, [("id".into(), "7".into())]);
}

#[test]
fn body_only_projection_keeps_layout_but_omits_unobservable_payloads() {
    let value = vm_request_descriptor_owned(
        request().into_parts(),
        RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY),
    );
    let ReplValue::Record { name, fields } = value else {
        panic!("request record");
    };
    assert_eq!(name, "Request");

    assert_eq!(fields.len(), 9);
    assert_eq!(fields[0].1, ReplValue::String(String::new()));
    assert_eq!(fields[1].1, ReplValue::String(String::new()));
    assert_eq!(fields[2].1, ReplValue::List(Vec::new()));
    assert_eq!(fields[3].1, ReplValue::String("payload".to_string()));
    assert_eq!(fields[4].1, ReplValue::String(String::new()));
    assert_eq!(fields[5].1, ReplValue::List(Vec::new()));
    assert_eq!(fields[6].1, ReplValue::List(Vec::new()));
    assert_eq!(fields[7].1, ReplValue::List(Vec::new()));
    assert_eq!(fields[8].1, ReplValue::String(String::new()));
}

#[test]
fn complete_projection_preserves_every_request_field() {
    let value =
        vm_request_descriptor_owned(request().into_parts(), RequestFieldProjection::Complete);
    let ReplValue::Record { name, fields } = value else {
        panic!("request record");
    };
    assert_eq!(name, "Request");

    assert_eq!(fields[0].1, ReplValue::String("POST".to_string()));
    assert_eq!(fields[1].1, ReplValue::String("/items/7".to_string()));
    assert_eq!(fields[3].1, ReplValue::String("payload".to_string()));
    assert_eq!(fields[4].1, ReplValue::String("page=2".to_string()));
    assert!(matches!(&fields[2].1, ReplValue::List(entries) if entries.len() == 1));
    assert_eq!(
        fields[5].1,
        ReplValue::List(vec![ReplValue::Tuple(vec![
            ReplValue::String("page".to_string()),
            ReplValue::String("2".to_string()),
        ])])
    );
    assert_eq!(
        fields[6].1,
        ReplValue::List(vec![ReplValue::Tuple(vec![
            ReplValue::String("content-type".to_string()),
            ReplValue::String("text/plain".to_string()),
        ])])
    );
    assert_eq!(
        fields[7].1,
        ReplValue::List(vec![ReplValue::Tuple(vec![
            ReplValue::String("session".to_string()),
            ReplValue::String("abc".to_string()),
        ])])
    );
    assert_eq!(
        fields[8].1,
        ReplValue::String("/tmp/terlan-upload".to_string())
    );
}

#[test]
fn file_body_projection_omits_text_and_preserves_only_runtime_path() {
    let value = vm_request_descriptor_owned(
        request().into_parts(),
        RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY_FILE_PATH),
    );
    let ReplValue::Record { name, fields } = value else {
        panic!("request record");
    };
    assert_eq!(name, "Request");
    assert_eq!(fields[3].1, ReplValue::String(String::new()));
    assert_eq!(
        fields[8].1,
        ReplValue::String("/tmp/terlan-upload".to_string())
    );
}

#[test]
fn repeated_projection_reuses_fixed_request_vector_without_native_jar() {
    let projection = RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY);
    let mut value = vm_request_descriptor_owned(request().into_parts(), projection);
    let ReplValue::Record { name, fields } = &value else {
        panic!("request record");
    };
    assert_eq!(name, "Request");
    let request_storage = fields.as_ptr();

    let replacement = Request::from_parts("POST", "/items/8", "replacement");
    replace_vm_request_descriptor(&mut value, replacement.into_parts(), projection);

    let ReplValue::Record { name, fields } = value else {
        panic!("request record");
    };
    assert_eq!(name, "Request");
    assert_eq!(fields.as_ptr(), request_storage);
    assert_eq!(fields[3].1, ReplValue::String("replacement".to_string()));
    assert_eq!(fields.len(), 9);
    assert!(fields.iter().all(|(name, _)| name != "cookie_jar"));
}

#[test]
fn replacing_request_clears_incoming_cookies_and_repairs_invalid_pair_shapes() {
    let mut value =
        vm_request_descriptor_owned(request().into_parts(), RequestFieldProjection::Complete);
    for replacement in [
        ReplValue::Map(vec![(
            ReplValue::String("stale".into()),
            ReplValue::String("1".into()),
        )]),
        ReplValue::String("invalid".into()),
        ReplValue::Record {
            name: "Other".into(),
            fields: Vec::new(),
        },
        ReplValue::Tuple(Vec::new()),
    ] {
        let ReplValue::Record { fields, .. } = &mut value else {
            panic!("request")
        };
        fields[7].1 = replacement;
        replace_vm_request_descriptor(
            &mut value,
            Request::from_parts("GET", "/", "").into_parts(),
            RequestFieldProjection::Complete,
        );
        let ReplValue::Record { fields, .. } = &value else {
            panic!("request")
        };
        assert_eq!(fields[7].1, ReplValue::List(Vec::new()));
    }
}
