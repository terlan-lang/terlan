use super::*;
use crate::{ErrorDomain, NativeBinding};

#[test]
fn invalid_arity_never_enters_callback_and_matches_stateless_diagnostics() {
    let binding = NativeContextBinding::new("example.call", 1, |calls: &mut usize, args| {
        *calls += 1;
        Ok(args[0].clone())
    });
    let stateless = NativeBinding {
        operation: "example.call",
        arity: 1,
        invoke: |_| unreachable!(),
    };
    let mut calls = 0;
    for args in [vec![], vec![NativeValue::Unit; 2]] {
        assert_eq!(
            binding.call(&mut calls, &args),
            stateless
                .validate_arity(args.len())
                .map(|()| NativeValue::Unit)
        );
        assert_eq!(calls, 0);
    }
    assert_eq!(
        binding.call(&mut calls, &[NativeValue::Int(42)]),
        Ok(NativeValue::Int(42))
    );
    assert_eq!(calls, 1);
}

#[test]
fn accepts_borrowed_unsized_contexts_without_erasing_errors() {
    let binding = NativeContextBinding::new("example.slice", 0, |values: &mut [u8], _| {
        values.fill(7);
        Err(BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "slice",
            "error[example]: failed",
        ))
    });
    let mut left = [0; 2];
    let right = [3; 2];
    let error = binding.call(&mut left, &[]).unwrap_err();
    assert_eq!(left, [7, 7]);
    assert_eq!(right, [3, 3]);
    assert_eq!(error.code(), "example");
    assert_eq!(error.operation(), "slice");
    assert_eq!(error.domain(), ErrorDomain::NativeBoundary);
}
