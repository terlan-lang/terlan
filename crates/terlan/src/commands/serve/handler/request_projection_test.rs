use crate::runtime::native::http::{Request, RequestFieldProjection};
use crate::runtime::vm::ReplValue;

use super::{replace_vm_request_descriptor, vm_request_descriptor_owned};

fn request() -> Request {
    Request::from_parts_with_raw_query_metadata(
        "POST",
        "/items/7",
        "payload",
        crate::terlan_native::http::RequestMetadata {
            params: vec![("id".to_string(), "7".to_string())],
            query_string: ("page=2").into(),
            query: vec![("page".to_string(), "2".to_string())],
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            cookies: vec![("session".to_string(), "abc".to_string())],
        },
    )
}

#[test]
fn body_only_projection_keeps_layout_but_omits_unobservable_payloads() {
    let value = vm_request_descriptor_owned(
        request().into_parts(),
        RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY),
    );
    let ReplValue::Tuple(fields) = value else {
        panic!("request tuple");
    };

    assert_eq!(fields.len(), 10);
    assert_eq!(fields[1], ReplValue::String(String::new()));
    assert_eq!(fields[2], ReplValue::String(String::new()));
    assert_eq!(fields[3], ReplValue::Map(Vec::new()));
    assert_eq!(fields[4], ReplValue::String("payload".to_string()));
    assert_eq!(fields[5], ReplValue::String(String::new()));
    assert_eq!(fields[6], ReplValue::Map(Vec::new()));
    assert_eq!(fields[7], ReplValue::Map(Vec::new()));
    assert_eq!(fields[8], ReplValue::Map(Vec::new()));
    assert_eq!(
        fields[9],
        ReplValue::Tuple(vec![
            ReplValue::Map(Vec::new()),
            ReplValue::List(Vec::new())
        ])
    );
}

#[test]
fn complete_projection_preserves_every_request_field() {
    let value =
        vm_request_descriptor_owned(request().into_parts(), RequestFieldProjection::Complete);
    let ReplValue::Tuple(fields) = value else {
        panic!("request tuple");
    };

    assert_eq!(fields[1], ReplValue::String("POST".to_string()));
    assert_eq!(fields[2], ReplValue::String("/items/7".to_string()));
    assert_eq!(fields[4], ReplValue::String("payload".to_string()));
    assert_eq!(fields[5], ReplValue::String("page=2".to_string()));
    assert!(matches!(&fields[3], ReplValue::Map(entries) if entries.len() == 1));
    assert!(matches!(&fields[6], ReplValue::Map(entries) if entries.len() == 1));
    assert!(matches!(&fields[7], ReplValue::Map(entries) if entries.len() == 1));
    assert!(matches!(&fields[8], ReplValue::Map(entries) if entries.len() == 1));
}

#[test]
fn repeated_projection_reuses_fixed_request_and_cookie_jar_vectors() {
    let projection = RequestFieldProjection::Fields(1 << RequestFieldProjection::BODY);
    let mut value = vm_request_descriptor_owned(request().into_parts(), projection);
    let ReplValue::Tuple(fields) = &value else {
        panic!("request tuple");
    };
    let request_storage = fields.as_ptr();
    let ReplValue::Tuple(jar) = &fields[9] else {
        panic!("cookie jar tuple");
    };
    let jar_storage = jar.as_ptr();

    let replacement = Request::from_parts("POST", "/items/8", "replacement");
    replace_vm_request_descriptor(&mut value, replacement.into_parts(), projection);

    let ReplValue::Tuple(fields) = value else {
        panic!("request tuple");
    };
    assert_eq!(fields.as_ptr(), request_storage);
    assert_eq!(fields[4], ReplValue::String("replacement".to_string()));
    let ReplValue::Tuple(jar) = &fields[9] else {
        panic!("cookie jar tuple");
    };
    assert_eq!(jar.as_ptr(), jar_storage);
}
