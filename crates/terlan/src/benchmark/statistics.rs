//! Shared benchmark order statistics. Historical reports are not recomputed.

/// Returns the nearest-rank percentile of sorted, nonempty samples.
/// Percentiles are integers in 1..=100; invalid internal inputs fail explicitly.
pub(crate) fn percentile<T: Copy>(sorted: &[T], percent: usize) -> T {
    assert!(!sorted.is_empty(), "percentile requires nonempty samples");
    assert!(
        (1..=100).contains(&percent),
        "percentile must be in 1..=100"
    );
    // Split the product to avoid overflow even for a maximal slice length.
    let rank = (sorted.len() / 100) * percent + ((sorted.len() % 100) * percent).div_ceil(100);
    sorted[rank - 1]
}

#[cfg(test)]
#[path = "statistics_test.rs"]
mod tests;
