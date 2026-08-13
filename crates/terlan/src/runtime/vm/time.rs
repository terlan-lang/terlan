#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmTimeResolution(u64);

#[cfg(test)]
impl VmTimeResolution {
    pub(crate) const SECOND: Self = Self(1);
    pub(crate) const MILLISECOND: Self = Self(1_000);
    pub(crate) const MICROSECOND: Self = Self(1_000_000);
    pub(crate) const NANOSECOND: Self = Self(1_000_000_000);

    /// Creates a custom positive time resolution.
    pub(crate) fn new(units_per_second: u64) -> Result<Self, String> {
        if units_per_second == 0 {
            return Err("VM time resolution must be non-zero".to_string());
        }
        Ok(Self(units_per_second))
    }

    /// Returns the number of ticks representing one second.
    pub(crate) const fn units_per_second(self) -> u64 {
        self.0
    }
}

/// Converts a signed time value between VM resolutions, rounding toward
/// negative infinity so every target tick names the containing time interval.
#[cfg(test)]
pub(crate) fn convert_time_unit(
    value: i128,
    from: VmTimeResolution,
    to: VmTimeResolution,
) -> Result<i128, String> {
    let scaled = value
        .checked_mul(i128::from(to.units_per_second()))
        .ok_or_else(|| {
            format!(
                "VM time conversion overflow for value {value} from resolution {} to {}",
                from.units_per_second(),
                to.units_per_second()
            )
        })?;
    Ok(scaled.div_euclid(i128::from(from.units_per_second())))
}

#[cfg(test)]
#[path = "time_beam_suite_parity_test.rs"]
#[cfg(test)]
mod time_beam_suite_parity_test;
