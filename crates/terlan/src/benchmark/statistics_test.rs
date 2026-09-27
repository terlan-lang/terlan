use super::percentile;

#[test]
fn nearest_rank_handles_even_odd_singleton_and_tail_boundaries() {
    assert_eq!(percentile(&[10, 20, 30, 40], 50), 20);
    assert_eq!(percentile(&[10, 20, 30], 50), 20);
    for percent in 1..=100 {
        assert_eq!(percentile(&[7u128], percent), 7);
    }
    let values: Vec<_> = (1..=100).collect();
    for percent in 1..=100 {
        assert_eq!(percentile(&values, percent), percent);
    }
    assert_eq!(percentile(&[0, 0, u64::MAX, u64::MAX], 50), 0);
    assert_eq!(percentile(&[0, u128::MAX], 100), u128::MAX);
}

#[test]
fn nearest_rank_matches_reference_for_all_small_sample_sizes() {
    for length in 1..=201 {
        let values: Vec<_> = (0..length).collect();
        for percent in 1..=100 {
            assert_eq!(
                percentile(&values, percent),
                (length * percent).div_ceil(100) - 1
            );
        }
    }
}

#[test]
#[should_panic(expected = "nonempty samples")]
fn empty_samples_are_not_reported_as_zero_latency() {
    percentile::<u64>(&[], 50);
}

#[test]
#[should_panic(expected = "1..=100")]
fn zero_percentile_is_rejected() {
    percentile(&[1], 0);
}

#[test]
#[should_panic(expected = "1..=100")]
fn out_of_range_percentile_is_rejected() {
    percentile(&[1], usize::MAX);
}
