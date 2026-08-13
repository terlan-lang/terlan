//! Backend-neutral numerical semantics for accelerator placement and validation.

use serde::{Deserialize, Serialize};

use super::AcceleratorDeterminism;

/// Integer overflow behavior required of every generated and package kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIntegerOverflow {
    /// Overflow is rejected by proof, checked execution, or typed failure.
    Checked,
}

/// Ordering policy for reductions whose floating result depends on association.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorReductionOrder {
    /// A fixed backend-independent reduction tree is required.
    FixedTree,
    /// The selected maintained implementation may choose a documented order.
    MaintainedImplementation,
    /// Execution order may vary and must be explicitly admitted by the target.
    Unspecified,
}

/// Floating comparison and exceptional-value contract.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorFloatPolicy {
    /// Maximum accepted absolute error.
    pub absolute_tolerance: f64,
    /// Maximum accepted relative error.
    pub relative_tolerance: f64,
    /// Whether two NaN values compare as equivalent for differential testing.
    pub nan_equivalent: bool,
    /// Whether positive and negative zero compare as equivalent.
    pub signed_zero_equivalent: bool,
    /// Whether infinity must preserve its sign exactly.
    pub infinity_sign_exact: bool,
}

impl AcceleratorFloatPolicy {
    /// Validates finite nonnegative tolerances.
    pub fn validate(self) -> Result<(), &'static str> {
        if !self.absolute_tolerance.is_finite()
            || !self.relative_tolerance.is_finite()
            || self.absolute_tolerance < 0.0
            || self.relative_tolerance < 0.0
        {
            return Err("accelerator floating tolerances must be finite and nonnegative");
        }
        Ok(())
    }

    /// Compares two floating results according to the complete exceptional-value policy.
    pub fn equivalent(self, expected: f64, actual: f64) -> bool {
        if expected.is_nan() || actual.is_nan() {
            return self.nan_equivalent && expected.is_nan() && actual.is_nan();
        }
        if expected.is_infinite() || actual.is_infinite() {
            return expected.is_infinite()
                && actual.is_infinite()
                && (!self.infinity_sign_exact
                    || expected.is_sign_positive() == actual.is_sign_positive());
        }
        if expected == 0.0 && actual == 0.0 {
            return self.signed_zero_equivalent
                || expected.is_sign_positive() == actual.is_sign_positive();
        }
        let difference = (expected - actual).abs();
        difference <= self.absolute_tolerance
            || difference
                <= self.relative_tolerance * expected.abs().max(actual.abs()).max(f64::MIN_POSITIVE)
    }
}

/// Complete numerical contract attached to one admitted accelerator execution.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorNumericPolicy {
    /// Exact checked integer behavior.
    pub integer_overflow: AcceleratorIntegerOverflow,
    /// Boolean storage is exactly zero or one and Boolean operations are exact.
    pub canonical_boolean_storage: bool,
    /// Floating comparison behavior.
    pub floating: AcceleratorFloatPolicy,
    /// Reduction ordering implied by target determinism.
    pub reduction_order: AcceleratorReductionOrder,
    /// Target determinism selected during admission.
    pub determinism: AcceleratorDeterminism,
}

impl AcceleratorNumericPolicy {
    /// Creates the release policy for one admitted determinism mode.
    pub fn release(determinism: AcceleratorDeterminism) -> Self {
        let reduction_order = match determinism {
            AcceleratorDeterminism::Strict => AcceleratorReductionOrder::FixedTree,
            AcceleratorDeterminism::BestEffort => {
                AcceleratorReductionOrder::MaintainedImplementation
            }
            AcceleratorDeterminism::Nondeterministic => AcceleratorReductionOrder::Unspecified,
        };
        Self {
            integer_overflow: AcceleratorIntegerOverflow::Checked,
            canonical_boolean_storage: true,
            floating: AcceleratorFloatPolicy {
                absolute_tolerance: 1.0e-6,
                relative_tolerance: 1.0e-5,
                nan_equivalent: true,
                signed_zero_equivalent: false,
                infinity_sign_exact: true,
            },
            reduction_order,
            determinism,
        }
    }

    /// Validates internal consistency before this policy enters an artifact.
    pub fn validate(self) -> Result<(), &'static str> {
        self.floating.validate()?;
        if !self.canonical_boolean_storage {
            return Err("accelerator Boolean storage must be canonical");
        }
        let compatible = matches!(
            (self.determinism, self.reduction_order),
            (
                AcceleratorDeterminism::Strict,
                AcceleratorReductionOrder::FixedTree
            ) | (
                AcceleratorDeterminism::BestEffort,
                AcceleratorReductionOrder::MaintainedImplementation
            ) | (
                AcceleratorDeterminism::Nondeterministic,
                AcceleratorReductionOrder::Unspecified
            )
        );
        if !compatible {
            return Err("accelerator reduction order conflicts with determinism mode");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "semantics_test.rs"]
mod tests;
