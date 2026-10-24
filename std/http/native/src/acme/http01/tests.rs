use super::*;
use crate::acme::{acme_runtime_plan, cache::store_acme_http01_challenge};
use crate::tls_config::{Mode, Settings};
use http::header;

fn config() -> ProjectServerTls {
    Settings {
        mode: Some(Mode::Auto),
        domains: Some(vec!["example.test".into()]),
        ..Settings::default()
    }
    .validate()
    .unwrap()
}

fn path(token: &str) -> String {
    format!("{ACME_HTTP01_PATH_PREFIX}{token}")
}

fn assert_response(response: &Response<Bytes>, status: u16, body: &str, head: bool) {
    assert_eq!(response.status().as_u16(), status);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/plain; charset=utf-8"
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.headers()[header::CONTENT_LENGTH],
        body.len().to_string()
    );
    assert!(!response.headers().contains_key(header::CONNECTION));
    assert!(!response.headers().contains_key(header::TRANSFER_ENCODING));
    assert_eq!(
        response.body().as_ref(),
        if head { b"" } else { body.as_bytes() }
    );
}

#[test]
fn admission_precedes_discovery_across_the_response_matrix() {
    let root = tempfile::tempdir().unwrap();
    let tls = config();
    let plan = acme_runtime_plan(root.path(), &tls);
    store_acme_http01_challenge(&plan, "token", "token.thumbprint").unwrap();
    let mut manual = tls.clone();
    manual.mode = Mode::Manual;
    let mut internal = tls.clone();
    internal.mode = Mode::Internal;
    let project = |tls| Ok(Some((root.path().to_path_buf(), tls)));
    let missing = "not found";
    let invalid = "error[serve_tls]: ACME HTTP-01 token `bad.token` is invalid";
    let cases = [
        (
            path("token"),
            project(tls.clone()),
            Some((200, "token.thumbprint")),
        ),
        (path("missing"), project(tls.clone()), Some((404, missing))),
        (
            path("bad.token"),
            project(tls.clone()),
            Some((400, invalid)),
        ),
        (
            path("token"),
            Err("discovery failed"),
            Some((500, "discovery failed")),
        ),
        (path("token"), Ok(None), None),
        (path("token"), project(manual), None),
        (path("token"), project(internal), None),
        ("/index.html".into(), project(tls), None),
        ("/index.html".into(), Err("must not discover"), None),
    ];
    for method in ["GET", "HEAD", "POST"] {
        for (path, discovery, expected) in &cases {
            let mut calls = 0;
            let result = response(method, path, || {
                calls += 1;
                discovery.clone().map_err(ServiceError::from)
            })
            .unwrap();
            let admitted = method != "POST" && path.starts_with(ACME_HTTP01_PATH_PREFIX);
            assert_eq!(calls, usize::from(admitted));
            match expected.filter(|_| admitted) {
                Some((status, body)) => {
                    assert_response(&result.unwrap(), status, body, method == "HEAD")
                }
                None => assert!(result.is_none()),
            }
        }
    }
}

#[test]
fn unrelated_paths_and_methods_never_discover_projects() {
    for method in ["POST", "PUT", "DELETE", "OPTIONS", "TRACE", "get", ""] {
        assert!(
            response(method, &path("token"), || panic!("unexpected discovery"))
                .unwrap()
                .is_none()
        );
    }
    for method in ["GET", "HEAD"] {
        for path in [
            "/",
            "/index.html",
            "/.well-known/acme-challenge",
            "/x/.well-known/acme-challenge/token",
        ] {
            assert!(response(method, path, || panic!("unexpected discovery"))
                .unwrap()
                .is_none());
        }
    }
}

#[test]
fn absent_or_nonautomatic_tls_does_not_claim_the_route() {
    let root = tempfile::tempdir().unwrap();
    for method in ["GET", "HEAD"] {
        assert!(response(method, &path("token"), || Ok(None))
            .unwrap()
            .is_none());
        for mode in [Mode::Manual, Mode::Internal] {
            let mut tls = config();
            tls.mode = mode;
            assert!(response(method, &path("token"), || Ok(Some((
                root.path().into(),
                tls
            ))))
            .unwrap()
            .is_none());
        }
    }
}

#[test]
fn challenges_and_missing_tokens_preserve_head_metadata() {
    let root = tempfile::tempdir().unwrap();
    let tls = config();
    let plan = acme_runtime_plan(root.path(), &tls);
    store_acme_http01_challenge(&plan, "safe_123-ABC", "safe.thumbprint").unwrap();
    for method in ["GET", "HEAD"] {
        for (token, status, body) in [
            ("safe_123-ABC", 200, "safe.thumbprint"),
            ("missing", 404, "not found"),
        ] {
            let mut calls = 0;
            let response = response(method, &path(token), || {
                calls += 1;
                Ok(Some((root.path().into(), tls.clone())))
            })
            .unwrap()
            .unwrap();
            assert_eq!(calls, 1);
            assert_response(&response, status, body, method == "HEAD");
        }
    }
}

#[test]
fn invalid_tokens_are_plain_text_errors_not_paths_or_headers() {
    let root = tempfile::tempdir().unwrap();
    for method in ["GET", "HEAD"] {
        for token in [
            "",
            ".",
            "..",
            "../secret",
            "a/b",
            "%2fsecret",
            "a\\b",
            "a?b",
            "a\0b",
            "a\r\nInjected: yes",
            "\u{e9}",
        ] {
            let result = response(method, &path(token), || {
                Ok(Some((root.path().into(), config())))
            })
            .unwrap()
            .unwrap();
            let body = format!("error[serve_tls]: ACME HTTP-01 token `{token}` is invalid");
            assert_response(&result, 400, &body, method == "HEAD");
            assert!(!result.headers().contains_key("injected"));
        }
    }
    assert!(!root.path().join(".terlan").exists());
}

#[test]
fn discovery_and_cache_read_failures_are_server_errors() {
    let root = tempfile::tempdir().unwrap();
    let tls = config();
    let plan = acme_runtime_plan(root.path(), &tls);
    std::fs::create_dir_all(plan.http01_challenge_dir.join("directory")).unwrap();
    for method in ["GET", "HEAD"] {
        let discovery = response(method, &path("token"), || Err("project unavailable".into()))
            .unwrap()
            .unwrap();
        assert_response(&discovery, 500, "project unavailable", method == "HEAD");
        let result = response(method, &path("directory"), || {
            Ok(Some((root.path().into(), tls.clone())))
        })
        .unwrap()
        .unwrap();
        assert_eq!(result.status(), StatusCode::INTERNAL_SERVER_ERROR);
        if method == "GET" {
            assert!(std::str::from_utf8(result.body())
                .unwrap()
                .contains("failed to read ACME HTTP-01"));
        } else {
            assert!(result.body().is_empty());
            assert_ne!(result.headers()[header::CONTENT_LENGTH], "0");
        }
    }
}

#[cfg(unix)]
#[test]
fn symlink_challenges_cannot_disclose_files_outside_the_cache() {
    let root = tempfile::tempdir().unwrap();
    let tls = config();
    let plan = acme_runtime_plan(root.path(), &tls);
    std::fs::create_dir_all(&plan.http01_challenge_dir).unwrap();
    let secret = root.path().join("secret");
    std::fs::write(&secret, "must not escape").unwrap();
    std::os::unix::fs::symlink(secret, plan.http01_challenge_dir.join("token")).unwrap();
    for method in ["GET", "HEAD"] {
        let result = response(method, &path("token"), || {
            Ok(Some((root.path().into(), tls.clone())))
        })
        .unwrap()
        .unwrap();
        assert_eq!(result.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!std::str::from_utf8(result.body())
            .unwrap()
            .contains("must not escape"));
    }
}
