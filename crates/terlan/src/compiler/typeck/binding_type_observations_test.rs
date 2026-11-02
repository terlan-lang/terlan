use super::*;

fn pattern() -> SyntaxPatternOutput {
    SyntaxPatternOutput {
        kind: SyntaxPatternKind::Var,
        arity: 0,
        text: Some("local".to_string()),
        children: Vec::new(),
        fields: Vec::new(),
        expanded_from: Vec::new(),
    }
}

fn identity() -> CoreBindingIdentity {
    CoreBindingIdentity {
        id: CoreBindingId(u64::MAX),
        region: CoreBindingRegionId(u64::MAX),
        name: "local".to_string(),
        kind: CoreBindingKind::Pattern,
        path: "function:value/0:binding:0".to_string(),
        span_start: 1,
        span_end: 2,
    }
}

fn prepare(pattern: &SyntaxPatternOutput) {
    ACTIVE.with(|cell| {
        let mut active = cell.borrow_mut();
        let index = &mut active.as_mut().unwrap().index;
        index.module = "sample".to_string();
        index.collecting = true;
    });
    index_pattern(pattern, &[identity()]);
}

#[test]
fn abandoned_clause_and_nested_capture_do_not_leak_observations() {
    let pattern = pattern();
    let (_, observations) = capture(|| {
        prepare(&pattern);
        let _abandoned = clause();
        record(
            std::slice::from_ref(&pattern),
            &HashMap::from([("local".to_string(), Type::Int)]),
            &HashMap::new(),
        );
        let (_, nested) = capture(|| {});
        assert!(nested.bindings.is_empty());
    });
    assert!(observations.bindings.is_empty());
    assert!(ACTIVE.with(|cell| cell.borrow().is_none()));
}

#[test]
fn index_bound_reports_incomplete_capture_without_adding_pointer_keys() {
    let pattern = pattern();
    let (_, observations) = capture(|| {
        ACTIVE.with(|cell| {
            let mut active = cell.borrow_mut();
            let index = &mut active.as_mut().unwrap().index;
            index.collecting = true;
            index.binding_count = 65536;
        });
        index_pattern(&pattern, &[identity()]);
        assert!(ACTIVE.with(|cell| cell.borrow().as_ref().unwrap().index.patterns.is_empty()));
    });
    assert!(observations.truncated);
    assert!(observations.bindings.is_empty());
}

#[test]
fn final_clause_substitution_precedes_observation_variable_canonicalization() {
    let pattern = pattern();
    let (_, observations) = capture(|| {
        prepare(&pattern);
        let completed = clause();
        record(
            std::slice::from_ref(&pattern),
            &HashMap::from([("local".to_string(), Type::Var(47))]),
            &HashMap::new(),
        );
        completed.finish(&HashMap::from([(47, Type::Int)]));
    });
    assert!(!observations.truncated);
    assert_eq!(observations.bindings.len(), 1);
    assert_eq!(observations.bindings[0]["compiler_type"], "Int");
    assert_eq!(observations.bindings[0]["binding"]["id"], u64::MAX);
}

#[test]
fn canonical_variables_preserve_array_and_existential_relationships() {
    fn ty(free: usize, bound: usize) -> Type {
        Type::Function {
            params: vec![Type::Var(free)],
            ret: Box::new(Type::Existential {
                params: vec![bound],
                body: Box::new(Type::Array {
                    size: super::super::types::ArraySize::Param(bound),
                    elem: Box::new(Type::Tuple(vec![Type::Var(free), Type::Var(bound)])),
                }),
            }),
        }
    }
    assert_eq!(
        super::super::type_system::canonical_observation_type(&ty(42, 99)),
        ty(0, 1)
    );
    assert_eq!(
        super::super::type_system::canonical_observation_type(&ty(700, 3)),
        ty(0, 1)
    );
}
