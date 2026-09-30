use super::*;

#[test]
fn only_complete_unguarded_list_coverage_narrows_a_fallback() {
    let union = CoreType::Union(vec![CoreType::Int, CoreType::List(Box::new(CoreType::Int))]);
    let empty = CorePattern::List(vec![]);
    let nonempty = CorePattern::ListCons {
        head: Box::new(CorePattern::Wildcard),
        tail: Box::new(CorePattern::Var("tail".into())),
    };
    let restricted = CorePattern::ListCons {
        head: Box::new(CorePattern::Int(1)),
        tail: Box::new(CorePattern::Wildcard),
    };
    for pattern in [&CorePattern::Wildcard, &CorePattern::Var("rest".into())] {
        let mut coverage = ListCoverage::default();
        assert_eq!(coverage.residual(pattern, Some(&union)), None);
        coverage.record(&nonempty, true);
        coverage.record(&empty, false);
        assert_eq!(coverage.residual(pattern, Some(&union)), None);
        coverage.record(&restricted, false);
        assert_eq!(coverage.residual(pattern, Some(&union)), None);
        coverage.record(&nonempty, false);
        assert_eq!(
            coverage.residual(pattern, Some(&union)),
            Some(CoreType::Int)
        );
        assert_eq!(coverage.residual(&empty, Some(&union)), None);
        assert_eq!(coverage.residual(pattern, None), None);
        assert_eq!(coverage.residual(pattern, Some(&CoreType::Int)), None);
    }
}

#[test]
fn residual_preserves_all_non_list_variants() {
    let mut coverage = ListCoverage {
        empty: true,
        nonempty: false,
    };
    let nonempty = CorePattern::ListCons {
        head: Box::new(CorePattern::Var("head".into())),
        tail: Box::new(CorePattern::Wildcard),
    };
    coverage.record(&nonempty, false);
    let list = CoreType::Apply {
        constructor: "std.core.List".into(),
        args: vec![CoreType::Int],
    };
    let union = CoreType::Union(vec![CoreType::Int, list.clone(), CoreType::Bool]);
    assert_eq!(
        coverage.residual(&CorePattern::Wildcard, Some(&union)),
        Some(CoreType::Union(vec![CoreType::Int, CoreType::Bool]))
    );
    assert_eq!(
        coverage.residual(&CorePattern::Wildcard, Some(&CoreType::Union(vec![list]))),
        None
    );
}
