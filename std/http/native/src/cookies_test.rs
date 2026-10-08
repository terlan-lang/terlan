use crate::*;

/// Verifies request cookie headers parse through the NativeBoundary boundary.
///
/// Inputs:
/// - A raw `Cookie` request header with optional whitespace and one malformed
///   segment.
///
/// Output:
/// - Parsed cookie name/value pairs in header order.
///
/// Transformation:
/// - Pins the NativeBoundary-owned parser boundary used by the VM bridge while
///   the actual cookie-pair parsing is delegated to a maintained crate.
#[test]
fn request_cookie_header_parser_splits_request_cookie_pairs() {
    let cookies = parse_request_cookie_header("session=abc; theme = dark; empty; user=Ada");

    assert_eq!(
        cookies,
        vec![
            ("session".to_string(), "abc".to_string()),
            ("theme".to_string(), "dark".to_string()),
            ("user".to_string(), "Ada".to_string()),
        ]
    );
}

/// Verifies request cookie parsing follows the maintained split-parser
/// semantics for empty segments and malformed pairs.
///
/// Inputs:
/// - A raw `Cookie` header containing leading/trailing separators, whitespace,
///   empty values, malformed pairs, and values containing extra `=`.
///
/// Output:
/// - Parsed valid pairs in source order.
///
/// Transformation:
/// - Pins Terlan to the `cookie` crate's request-header parser behavior
///   instead of a Terlan-owned tokenization path.
#[test]
fn request_cookie_header_parser_ignores_malformed_segments() {
    let cookies = parse_request_cookie_header(" ; session=abc ; ; =bad ; empty= ; token=a== ");

    assert_eq!(
        cookies,
        vec![
            ("session".to_string(), "abc".to_string()),
            ("empty".to_string(), "".to_string()),
            ("token".to_string(), "a==".to_string()),
        ]
    );
}

/// Verifies request cookie parsing preserves source-visible parser semantics
/// for quoted values and duplicate cookie names.
///
/// Inputs:
/// - A raw `Cookie` header containing duplicate cookie names, a quoted value,
///   and a normal value after the duplicate.
///
/// Output:
/// - Parsed pairs in maintained parser order without collapsing duplicates or
///   unquoting values at the boundary.
///
/// Transformation:
/// - Keeps duplicate-cookie resolution outside the low-level parser so request
///   lookup and jar construction can choose their own policy.
#[test]
fn request_cookie_header_parser_preserves_duplicates_and_quoted_values() {
    let cookies = parse_request_cookie_header("theme=light; quoted=\"dark mode\"; theme=dark");

    assert_eq!(
        cookies,
        vec![
            ("theme".to_string(), "light".to_string()),
            ("quoted".to_string(), "\"dark mode\"".to_string()),
            ("theme".to_string(), "dark".to_string()),
        ]
    );
}

/// Verifies cookie header construction emits the stable first supported shape.
///
/// Inputs:
/// - Cookie name, value, path, and boolean flags.
///
/// Output:
/// - Test passes when the serialized header includes the requested attributes.
///
/// Transformation:
/// - Exercises the Rust-owned cookie serialization boundary without routing a
///   full HTTP request.
#[test]
fn cookie_set_header_serializes_supported_attributes() {
    let header = set_header_with_options(
        "session",
        "abc123",
        &CookieOptions {
            path: "/account".into(),
            http_only: true,
            secure: true,
            ..default_options()
        },
    )
    .expect("valid cookie");

    assert_eq!(header, "session=abc123; HttpOnly; Secure; Path=/account");
}

/// Verifies cookie option serialization covers the typed option surface.
///
/// Inputs:
/// - Cookie name, value, and every currently supported typed option.
///
/// Output:
/// - Test passes when the serialized header includes attributes in stable
///   order.
///
/// Transformation:
/// - Exercises the richer native cookie option boundary before the Terlan
///   source helper grows a mutable cookie jar.
#[test]
fn cookie_set_header_with_options_serializes_full_option_surface() {
    let options = CookieOptions {
        path: "/account".to_string(),
        domain: Some("example.com".to_string()),
        max_age: Some(3600),
        expires: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        http_only: true,
        secure: true,
        same_site: Some(CookieSameSite::Strict),
    };

    let header =
        set_header_with_options("session", "abc123", &options).expect("valid cookie options");

    assert_eq!(
        header,
        "session=abc123; HttpOnly; SameSite=Strict; Secure; Path=/account; Domain=example.com; Max-Age=3600; Expires=Wed, 21 Oct 2026 07:28:00 GMT"
    );
}

/// Verifies every supported SameSite policy serializes predictably.
///
/// Inputs:
/// - Cookie options using `Lax`, `Strict`, and `None` SameSite policies.
///
/// Output:
/// - Test passes when each policy appears with the expected header spelling.
///
/// Transformation:
/// - Locks the native adapter vocabulary used by source-visible cookie option
///   wrappers before richer record-to-native lowering is introduced.
#[test]
fn cookie_set_header_with_options_serializes_same_site_variants() {
    for (policy, expected) in [
        (CookieSameSite::Lax, "SameSite=Lax"),
        (CookieSameSite::Strict, "SameSite=Strict"),
        (CookieSameSite::None, "SameSite=None"),
    ] {
        let mut options = default_options();
        options.same_site = Some(policy);
        let header =
            set_header_with_options("session", "abc123", &options).expect("valid cookie options");

        assert!(
            header.contains(expected),
            "expected `{expected}` in `{header}`"
        );
    }
}

/// Verifies cookie deletion header construction emits an expiring cookie.
///
/// Inputs:
/// - Cookie name and path.
///
/// Output:
/// - Test passes when deletion metadata is included.
///
/// Transformation:
/// - Exercises the deletion helper that handlers can pass to
///   `Response.set_cookie_header`.
#[test]
fn cookie_delete_header_serializes_expiring_cookie() {
    let header = set_header_with_options(
        "session",
        "",
        &CookieOptions {
            max_age: Some(0),
            expires: Some("Thu, 01 Jan 1970 00:00:00 GMT".into()),
            ..default_options()
        },
    )
    .expect("valid deletion cookie");

    assert_eq!(
        header,
        "session=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    );
}

/// Verifies invalid cookie names are rejected.
///
/// Inputs:
/// - Cookie names that are empty, reserved, or contain separators.
///
/// Output:
/// - Test passes when all invalid names produce the stable invalid-name code.
///
/// Transformation:
/// - Pins the conservative adapter validation boundary before a richer cookie
///   crate-backed jar is introduced.
#[test]
fn cookie_set_header_rejects_invalid_names() {
    for name in ["", "$Version", "a$b", "trailing$", "bad name", "bad;name"] {
        let error = set_header_with_options(name, "abc", &default_options())
            .expect_err("invalid cookie name");

        assert_eq!(error.code(), "http.cookie.invalid_name");
        assert_eq!(error.status(), 400);
    }
}

#[test]
fn cookie_name_token_parser_preserves_the_existing_ascii_domain_and_case() {
    // Fixed domain, independent of the maintained parser under test.
    let accepted = b"!#%&'*+-.0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ^_`abcdefghijklmnopqrstuvwxyz|~";
    for byte in 0..=255 {
        let token = char::from(byte);
        for name in [token.to_string(), format!("prefix{token}Suffix")] {
            let result = set_header_with_options(&name, "value", &default_options());
            if accepted.contains(&byte) {
                assert_eq!(result.unwrap(), format!("{name}=value; Path=/"));
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.code(), "http.cookie.invalid_name", "byte {byte}");
                assert_eq!(error.status(), 400);
            }
        }
    }
    for name in [
        "\u{80}",
        "\u{e9}",
        "\u{212a}",
        "name\u{2028}suffix",
        "\u{1f600}",
    ] {
        assert_eq!(
            set_header_with_options(name, "value", &default_options())
                .unwrap_err()
                .code(),
            "http.cookie.invalid_name",
        );
    }
}

#[test]
fn cookie_name_length_is_bounded_by_the_maintained_http_parser() {
    let longest = "A".repeat(65_535);
    assert_eq!(
        set_header_with_options(&longest, "v", &default_options()).unwrap(),
        format!("{longest}=v; Path=/"),
    );
    let error = set_header_with_options(&(longest + "A"), "v", &default_options()).unwrap_err();
    assert_eq!(error.code(), "http.cookie.invalid_name");
    assert_eq!(error.status(), 400);
}

/// Verifies invalid cookie values and paths are rejected.
///
/// Inputs:
/// - Values and paths that could inject attributes or invalid metadata.
///
/// Output:
/// - Test passes when stable error codes identify the failed field.
///
/// Transformation:
/// - Keeps the first cookie builder intentionally strict at the native adapter
///   boundary.
#[test]
fn cookie_set_header_rejects_invalid_values_and_paths() {
    let value_error = set_header_with_options("session", "abc;HttpOnly", &default_options())
        .expect_err("bad value");
    let path_error = set_header_with_options(
        "session",
        "abc",
        &CookieOptions {
            path: "relative".into(),
            ..default_options()
        },
    )
    .expect_err("bad path");

    assert_eq!(value_error.code(), "http.cookie.invalid_value");
    assert_eq!(path_error.code(), "http.cookie.invalid_path");
}

/// Verifies invalid optional cookie attributes are rejected.
///
/// Inputs:
/// - Cookie options with unsafe domain and expires attribute values.
///
/// Output:
/// - Test passes when both invalid attributes produce the stable option error.
///
/// Transformation:
/// - Pins the validation boundary for future typed cookie option lowering.
#[test]
fn cookie_set_header_with_options_rejects_invalid_optional_attributes() {
    let mut options = default_options();
    options.domain = Some("bad;domain".to_string());
    let domain_error = set_header_with_options("session", "abc", &options).expect_err("bad domain");

    let mut options = default_options();
    options.expires = Some("bad\nexpires".to_string());
    let expires_error =
        set_header_with_options("session", "abc", &options).expect_err("bad expires");

    assert_eq!(domain_error.code(), "http.cookie.invalid_attribute");
    assert_eq!(expires_error.code(), "http.cookie.invalid_attribute");
}

fn default_options() -> CookieOptions {
    CookieOptions {
        path: "/".into(),
        domain: None,
        max_age: None,
        expires: None,
        http_only: false,
        secure: false,
        same_site: None,
    }
}
