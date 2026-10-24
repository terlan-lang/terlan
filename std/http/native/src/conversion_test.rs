use super::*;

#[test]
fn cookie_storage_preserves_all_pairs_for_source_precedence() {
    let cookies = "sid=first; empty=; sid=second; SID=upper; empty=later".to_owned();
    let request = Request::from_parts_with_raw_query_metadata(
        "GET",
        "/",
        "",
        RequestMetadata {
            cookies: vec![cookies.clone()],
            ..Default::default()
        },
    );
    assert_eq!(request.cookie_headers(), std::slice::from_ref(&cookies));
    let first_value = request.cookie_headers()[0].as_ptr();
    let parts = request.into_parts();
    assert_eq!(parts.cookies, [cookies]);
    assert_eq!(parts.cookies[0].as_ptr(), first_value);
    assert!(Request::from_parts("GET", "/", "")
        .into_parts()
        .cookies
        .is_empty());
}

#[test]
fn request_parts_transfer_all_metadata_without_cloning() {
    let metadata = RequestMetadata {
        params: vec![("id".into(), "42".into())],
        query_string: "tag=a&tag=&tag=b".into(),
        headers: vec![("x-tag".into(), "a".into()), ("x-tag".into(), "b".into())],
        cookies: vec!["session=abc".into()],
    };
    let request = Request::from_parts_with_raw_query_metadata(
        "POST",
        "/items/42",
        "payload",
        metadata.clone(),
    )
    .with_body_file_path("/tmp/upload");
    assert_eq!(request.query_string(), metadata.query_string);
    assert_eq!(request.header_pairs(), metadata.headers);
    let body_ptr = request.body().as_ptr();
    let query_ptr = request.query_string().as_ptr();
    let headers_ptr = request.header_pairs().as_ptr();
    let parts = request.into_parts();
    assert_eq!(parts.method, "POST");
    assert_eq!(parts.path, "/items/42");
    assert_eq!(parts.body, "payload");
    assert_eq!(parts.body_file_path, "/tmp/upload");
    assert_eq!(parts.params, metadata.params);
    assert_eq!(parts.query_string, metadata.query_string);
    assert_eq!(parts.headers, metadata.headers);
    assert_eq!(parts.cookies, metadata.cookies);
    assert_eq!(parts.body.as_ptr(), body_ptr);
    assert_eq!(parts.query_string.as_ptr(), query_ptr);
    assert_eq!(parts.headers.as_ptr(), headers_ptr);
}
