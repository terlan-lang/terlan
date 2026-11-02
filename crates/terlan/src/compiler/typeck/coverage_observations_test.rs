use super::*;

fn verdict(covered: bool) -> Analysis {
    Analysis {
        exhaustive: Some(covered),
        remaining: Vec::new(),
        guarded_clauses: Vec::new(),
        reason: None,
        error: None,
    }
}

#[test]
fn conflicting_inference_contexts_never_export_an_exhaustive_verdict() {
    let (_, result) = capture(|| {
        let span = Span { start: 1, end: 2 };
        let _owner = owner("sample", "value", 1, span);
        record("case", span, None, None, &Type::Bool, &verdict(true));
        record("case", span, None, None, &Type::Bool, &verdict(false));
    });
    assert_eq!(result.observations.len(), 2);
    assert!(result.observations.iter().all(|row| !row.consistent
        && row.exhaustive.is_none()
        && row.reason == Some("ambiguous_inference_context")));
}

#[test]
fn nested_capture_restores_outer_owner_even_after_unwinding() {
    let (_, result) = capture(|| {
        let span = Span { start: 1, end: 2 };
        let _owner = owner("sample", "outer", 1, span);
        let aborted = std::panic::catch_unwind(|| {
            capture(|| {
                let _inner = owner("sample", "inner", 1, span);
                panic!("interrupted semantic export");
            })
        });
        assert!(aborted.is_err());
        record("case", span, None, None, &Type::Bool, &verdict(true));
    });
    assert_eq!(result.observations.len(), 1);
    assert_eq!(result.observations[0].owner.name, "outer");
}

#[test]
fn capture_limit_is_reported_instead_of_claiming_a_complete_set() {
    let (_, result) = capture(|| {
        let _owner = owner(
            "sample",
            "value",
            1,
            Span {
                start: 0,
                end: 10_000,
            },
        );
        for index in 0..4097 {
            record(
                "case",
                Span {
                    start: index,
                    end: index + 1,
                },
                None,
                None,
                &Type::Bool,
                &verdict(true),
            );
        }
    });
    assert!(result.truncated);
    assert_eq!(result.observations.len(), 4096);
}
