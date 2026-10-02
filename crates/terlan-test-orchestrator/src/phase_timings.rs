//! Observed deadlines and conservative recommendations; never test-pass evidence.

/// Recommends twice the observed duration, rounded to minutes and bounded at two hours.
pub(super) fn recommended_seconds(elapsed_ms: u128) -> u64 {
    (elapsed_ms
        .saturating_mul(2)
        .div_ceil(60_000)
        .saturating_mul(60))
    .clamp(60, 7_200) as u64
}

#[cfg(test)]
#[path = "phase_timings_test.rs"]
mod tests;
