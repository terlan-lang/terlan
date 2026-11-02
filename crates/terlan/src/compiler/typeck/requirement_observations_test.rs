use super::*;

fn scheme() -> FunctionScheme {
    FunctionScheme {
        params: vec![Type::Int],
        ret: Type::Int,
        generic_params: Vec::new(),
        bounds: Vec::new(),
    }
}

#[test]
fn nested_interrupted_capture_does_not_contaminate_the_outer_export() {
    let (_, result) = capture(|| {
        let aborted = std::panic::catch_unwind(|| {
            capture(|| {
                record("inner", "value", 1, Span { start: 0, end: 1 }, &scheme());
                panic!("interrupted export");
            })
        });
        assert!(aborted.is_err());
        record("outer", "value", 1, Span { start: 0, end: 1 }, &scheme());
    });
    assert_eq!(result.requirements.len(), 1);
    assert_eq!(result.requirements[0]["module"], "outer");
    let (_, next) = capture(|| ());
    assert!(next.requirements.is_empty());
}

#[test]
fn bounded_requirement_capture_cannot_silently_drop_distinct_contracts() {
    let (_, result) = capture(|| {
        for index in 0..4097 {
            record(
                "sample",
                "value",
                1,
                Span {
                    start: index,
                    end: index + 1,
                },
                &scheme(),
            );
        }
    });
    assert_eq!(result.requirements.len(), 4096);
    assert!(result.truncated);
}
