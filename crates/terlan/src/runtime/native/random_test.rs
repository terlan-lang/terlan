use super::*;

/// Verifies seeded generators replay the same deterministic sequence.
#[test]
fn seeded_generators_replay_same_sequence() {
    let left = seed(42).expect("valid seed");
    let right = seed(42).expect("valid seed");

    let (_left_next, left_value) = int(&left);
    let (_right_next, right_value) = int(&right);

    assert_eq!(left_value, right_value);
}

/// Verifies long seeded bounded sequences replay and stay inside their range.
#[test]
fn seeded_bounded_sequences_replay_and_respect_exclusive_upper_bound() {
    let mut left = seed(77).expect("valid seed");
    let mut right = seed(77).expect("valid seed");

    for _ in 0..100 {
        let (left_next, left_value) = bounded_int(&left, 1, 8).expect("valid left bounded draw");
        let (right_next, right_value) =
            bounded_int(&right, 1, 8).expect("valid right bounded draw");

        assert!((1..8).contains(&left_value));
        assert_eq!(left_value, right_value);
        left = left_next;
        right = right_next;
    }
}

/// Verifies invalid random operation inputs return typed errors.
#[test]
fn invalid_inputs_return_stable_errors() {
    assert_eq!(
        seed(-1).expect_err("negative seed should fail").code(),
        "random.invalid_seed"
    );
    let generator = seed(1).expect("valid seed");
    assert_eq!(
        bounded_int(&generator, 10, 10)
            .expect_err("empty range should fail")
            .code(),
        "random.invalid_bounds"
    );
    assert_eq!(
        choice(&generator, 0)
            .expect_err("empty choice should fail")
            .code(),
        "random.empty_choice"
    );
    assert_eq!(
        sample(&generator, 2, 3)
            .expect_err("oversized sample should fail")
            .code(),
        "random.sample_too_large"
    );
}

/// Verifies helper operations produce values with expected shapes.
#[test]
fn helpers_cover_draws_and_index_generation() {
    let generator = seed(7).expect("valid seed");
    let _entropy = entropy();
    let (generator, bounded) = bounded_int(&generator, 3, 9).expect("valid bounds");
    assert!((3..9).contains(&bounded));

    let (generator, float_value) = float(&generator);
    assert!((0.0..1.0).contains(&float_value));

    let (generator, _bool_value) = bool(&generator);
    let (generator, index) = choice(&generator, 4).expect("non-empty choice");
    assert!(index < 4);

    let (generator, shuffled) = shuffle(&generator, 4);
    assert_eq!(shuffled.len(), 4);

    let (_generator, sample) = sample(&generator, 4, 2).expect("valid sample");
    assert_eq!(sample.len(), 2);
}
