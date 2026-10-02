use super::recommended_seconds;

#[test]
fn timing_recommendations_have_headroom_rounding_and_finite_bounds() {
    assert_eq!(recommended_seconds(0), 60);
    assert_eq!(recommended_seconds(31_000), 120);
    assert_eq!(recommended_seconds(1_800_373), 3_660);
    assert_eq!(recommended_seconds(u128::MAX), 7_200);
}
