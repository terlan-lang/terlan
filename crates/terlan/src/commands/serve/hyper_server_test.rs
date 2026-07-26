use super::*;

#[test]
fn protocol_errors_are_hyper_responses() {
    let response = error_response(400, "bad request".to_string());
    assert_eq!(response.status(), http::StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(http::header::CONTENT_TYPE),
        Some(&http::HeaderValue::from_static("text/plain; charset=utf-8"))
    );
}

#[test]
fn web_root_is_copied_once_per_permanent_protocol_owner() {
    let first = Arc::new(PathBuf::from("/tmp/terlan-owner-a"));
    let first_local = owner_local_web_root(&first);
    let first_reused = owner_local_web_root(&first);
    assert!(Rc::ptr_eq(&first_local, &first_reused));

    let second = Arc::new(PathBuf::from("/tmp/terlan-owner-b"));
    let second_local = owner_local_web_root(&second);
    assert!(!Rc::ptr_eq(&first_local, &second_local));
    assert_eq!(second_local.as_path(), second.as_path());
}
