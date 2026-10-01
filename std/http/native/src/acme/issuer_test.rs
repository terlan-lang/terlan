use super::*;
use crate::acme::acme_runtime_plan;
#[cfg(feature = "acme-issuer")]
use crate::acme::load_acme_runtime_tls_cache;
use crate::tls_config::{Mode, Settings};
use std::future::{pending, ready};
use std::task::{Context, Poll, Waker};

#[path = "issuer_test/transport.rs"]
mod transport;
use transport::{Scenario, TestCa};

fn plan(root: &std::path::Path) -> AcmeRuntimePlan {
    let config = Settings {
        mode: Some(Mode::Auto),
        domains: Some(vec!["example.test".into()]),
        ..Settings::default()
    }
    .validate()
    .unwrap();
    let mut plan = acme_runtime_plan(root, &config);
    plan.directory_url = "https://ca.test/directory".into();
    plan
}

fn completed<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("in-memory ACME transport unexpectedly suspended"),
    }
}

#[test]
fn invalid_plans_fail_before_contacting_the_ca() {
    let dir = tempfile::tempdir().unwrap();
    for invalid in 0..3 {
        let mut plan = plan(dir.path());
        match invalid {
            0 => plan.domains.clear(),
            1 => plan.domains = vec!["   ".into()],
            _ => plan.certificate_path = dir.path().join("../escape.pem"),
        }
        let ca = TestCa::new(Scenario::Ready);
        assert!(completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |_| Ok(()),
            |_| ready(())
        ))
        .is_err());
        assert!(ca.requests().is_empty());
        assert!(!plan.cache_dir.exists());
    }
}

#[test]
fn authorization_failure_and_transport_failure_do_not_publish_certificates() {
    for (scenario, message) in [
        (Scenario::InvalidAuthorization, "not usable"),
        (Scenario::NoHttp01, "did not offer HTTP-01"),
        (
            Scenario::MalformedDirectory,
            "failed to create ACME account",
        ),
        (Scenario::RejectedOrder, "failed to create ACME order"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(dir.path());
        let ca = TestCa::new(scenario);
        let error = completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |_| Ok(()),
            |_| ready(()),
        ))
        .unwrap_err();
        assert!(error.contains(message), "{error}");
        assert!(!plan.certificate_path.exists());
        assert!(!ca.requests().iter().any(|path| path == "/finalize"));
    }
}

#[test]
fn order_and_certificate_polling_have_bounded_nonblocking_backoff() {
    for (scenario, expected, message) in [
        (
            Scenario::PendingOrder,
            vec![250, 500, 1000, 2000, 4000],
            "did not become ready after 5 polls",
        ),
        (
            Scenario::PendingCertificate,
            vec![1000; 9],
            "certificate was not available after 10 polls",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(dir.path());
        let ca = TestCa::new(scenario);
        let mut delays = Vec::new();
        let error = completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |_| Ok(()),
            |duration| {
                delays.push(duration.as_millis());
                ready(())
            },
        ))
        .unwrap_err();
        assert!(error.contains(message), "{error}");
        assert_eq!(delays, expected);
        assert_eq!(
            ca.requests()
                .iter()
                .filter(|path| *path == "/order")
                .count(),
            if scenario == Scenario::PendingOrder {
                5
            } else {
                10
            }
        );
        assert!(!plan.certificate_path.exists());
    }
}

#[test]
fn client_errors_preserve_the_failed_stage_without_publishing() {
    for (path, scenario, context) in [
        ("/directory", Scenario::Ready, "create ACME account"),
        ("/auth", Scenario::Ready, "fetch ACME authorizations"),
        (
            "/challenge",
            Scenario::Ready,
            "mark ACME HTTP-01 challenge ready",
        ),
        ("/order", Scenario::PendingOrder, "refresh ACME order"),
        ("/finalize", Scenario::Ready, "finalize ACME order"),
        ("/certificate", Scenario::Ready, "fetch ACME certificate"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan(dir.path());
        let ca = TestCa::new(scenario).rejecting(path);
        let error = completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |_| Ok(()),
            |_| ready(()),
        ))
        .unwrap_err();
        assert!(error.contains(context), "{error}");
        assert_eq!(ca.requests().last().unwrap(), path);
        assert!(!plan.certificate_path.exists());
    }
}

#[test]
fn refreshed_terminal_order_states_are_honored_even_at_the_poll_limit() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path());
    let ca = TestCa::new(Scenario::InvalidOrderOnLastPoll);
    let error = completed(issue_certificate_cache(
        &plan,
        Some(Box::new(ca.clone())),
        |_| Ok(()),
        |_| ready(()),
    ))
    .unwrap_err();
    assert!(error.contains("order became invalid"), "{error}");
    assert_eq!(
        ca.requests()
            .iter()
            .filter(|path| *path == "/order")
            .count(),
        5
    );
    assert!(!ca.requests().iter().any(|path| path == "/finalize"));

    let ca = TestCa::new(Scenario::ReadyAfterPoll).rejecting("/finalize");
    let error = completed(issue_certificate_cache(
        &plan,
        Some(Box::new(ca.clone())),
        |_| Ok(()),
        |_| ready(()),
    ))
    .unwrap_err();
    assert!(error.contains("finalize ACME order"), "{error}");
    assert_eq!(
        ca.requests()
            .iter()
            .filter(|path| *path == "/order")
            .count(),
        1
    );
}

#[test]
fn cancelling_a_pending_delay_stops_protocol_progress() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path());
    let ca = TestCa::new(Scenario::PendingOrder);
    let mut future = Box::pin(issue_certificate_cache(
        &plan,
        Some(Box::new(ca.clone())),
        |_| Ok(()),
        |_| pending::<()>(),
    ));
    assert!(future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending());
    let requests = ca.requests();
    assert!(requests.iter().any(|path| path == "/challenge"));
    assert!(!requests.iter().any(|path| path == "/order"));
    drop(future);
    assert_eq!(ca.requests(), requests);
    assert!(!plan.certificate_path.exists());
}

#[test]
fn observer_failure_prevents_challenge_submission_and_finalization() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path());
    let ca = TestCa::new(Scenario::Ready);
    assert_eq!(
        completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |_| Err("owner exited".into()),
            |_| ready(()),
        ))
        .unwrap_err(),
        "owner exited"
    );
    assert!(!ca
        .requests()
        .iter()
        .any(|path| path == "/challenge" || path == "/finalize"));
    assert!(!plan.certificate_path.exists());
}

#[test]
fn maintained_authorization_types_cover_valid_and_terminal_states() {
    for state in ["valid", "invalid", "revoked", "expired"] {
        let authorization: Authorization = serde_json::from_value(serde_json::json!({
            "identifier": {"type": "dns", "value": "example.test"},
            "status": state, "challenges": []
        }))
        .unwrap();
        let authorizations = [authorization];
        let result = pending_http01_challenges(&authorizations);
        if state == "valid" {
            assert!(result.unwrap().is_empty());
        } else {
            assert!(result.err().unwrap().contains("not usable"));
        }
    }
    assert!(serde_json::from_value::<Authorization>(serde_json::json!({
        "identifier": {"type": "dns", "value": "example.test"},
        "status": "unknown", "challenges": []
    }))
    .is_err());
    assert_eq!(
        acme_contact_strings(Some(" admin@example.test ")),
        ["mailto:admin@example.test"]
    );
    assert!(acme_contact_strings(None).is_empty());
    assert!(acme_contact_strings(Some(" ")).is_empty());
    assert_eq!(
        acme_domain_identifiers(&[" example.test ".into()]).unwrap(),
        [Identifier::Dns("example.test".into())]
    );
}

// The feature gate opts into an OpenSSL test CA, never public network issuance.
#[cfg(feature = "acme-issuer")]
#[test]
fn real_client_issues_matching_material_and_reuses_cached_account() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path());
    for iteration in 0..2 {
        let ca = TestCa::new(Scenario::SignedCertificate);
        let mut events = Vec::new();
        completed(issue_certificate_cache(
            &plan,
            Some(Box::new(ca.clone())),
            |event| {
                if let IssuanceEvent::ChallengePrepared {
                    token,
                    key_authorization,
                } = &event
                {
                    assert_eq!(
                        std::fs::read_to_string(plan.http01_challenge_dir.join(token)).unwrap(),
                        *key_authorization
                    );
                    assert!(key_authorization.starts_with("test_token."));
                }
                events.push(format!("{event:?}"));
                Ok(())
            },
            |_| ready(()),
        ))
        .unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(&events[1..], ["Issuing", "WritingCache", "Complete"]);
        assert!(load_acme_runtime_tls_cache(&plan).unwrap().is_some());
        assert_eq!(
            ca.requests()
                .iter()
                .filter(|path| *path == "/account")
                .count(),
            usize::from(iteration == 0)
        );
    }
}

#[cfg(feature = "acme-issuer")]
#[test]
fn observer_can_reject_publication_after_certificate_arrives() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan(dir.path());
    let ca = TestCa::new(Scenario::SignedCertificate);
    let result = completed(issue_certificate_cache(
        &plan,
        Some(Box::new(ca.clone())),
        |event| {
            if event == IssuanceEvent::WritingCache {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        },
        |_| ready(()),
    ));
    assert_eq!(result.unwrap_err(), "cancelled");
    assert!(ca.requests().iter().any(|path| path == "/certificate"));
    assert!(!plan.certificate_path.exists());
    assert!(!plan.private_key_path.exists());
}
