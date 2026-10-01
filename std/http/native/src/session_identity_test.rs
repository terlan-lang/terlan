use super::*;
use std::collections::BTreeSet;

#[test]
fn identities_use_all_entropy_bytes_and_cookie_safe_encoding() {
    for value in [0, 1, 127, 255] {
        let identity = issue_with("", &mut |_| false, |bytes| {
            bytes.fill(value);
            Ok(())
        })
        .unwrap();
        assert_eq!(identity.len(), 43);
        assert!(identity
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)));
        assert_eq!(
            URL_SAFE_NO_PAD.decode(identity).unwrap(),
            [value; IDENTITY_BYTES]
        );
    }
}

#[test]
fn collisions_retry_without_reusing_an_occupied_identity() {
    let mut attempt = 0;
    let taken = URL_SAFE_NO_PAD.encode([0; IDENTITY_BYTES]);
    let identity = issue_with("", &mut |candidate| candidate == taken, |bytes| {
        bytes.fill(attempt);
        attempt += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(attempt, 2);
    assert_eq!(identity, URL_SAFE_NO_PAD.encode([1; IDENTITY_BYTES]));
}

#[test]
fn exhausted_collisions_and_entropy_failure_fail_closed() {
    let mut attempts = 0;
    let error = issue_with("", &mut |_| true, |_| {
        attempts += 1;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(attempts, MAX_ATTEMPTS);
    assert_eq!(error.code(), "http.session.identity_collision");
    let error = issue_with(
        "",
        &mut |_| panic!("failed entropy cannot reach admission"),
        |_| {
            Err(NativeAdapterError::new(
                "http.session.entropy",
                "unavailable",
                0,
            ))
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), "http.session.entropy");
}

#[test]
fn operating_system_issuance_produces_distinct_full_length_tokens() {
    let mut issued = BTreeSet::new();
    for _ in 0..256 {
        let identity = issue("", |candidate| issued.contains(candidate)).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD.decode(&identity).unwrap().len(),
            IDENTITY_BYTES
        );
        assert!(issued.insert(identity));
    }
}

#[test]
fn unrecognized_input_is_excluded_even_when_the_registry_is_empty() {
    let supplied = URL_SAFE_NO_PAD.encode([0; IDENTITY_BYTES]);
    let mut attempts = 0;
    let identity = issue_with(&supplied, &mut |_| false, |bytes| {
        bytes.fill(attempts);
        attempts += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(attempts, 2);
    assert_ne!(identity, supplied);
    let error = issue_with(&supplied, &mut |_| false, |bytes| {
        bytes.fill(0);
        Ok(())
    })
    .unwrap_err();
    assert_eq!(error.code(), "http.session.identity_collision");
}
