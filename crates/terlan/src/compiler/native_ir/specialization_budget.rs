//! Deterministic application-wide bounds for NativeIR specialization.

/// Maximum total expansion admitted across one native application.
pub(super) const MAX_APPLICATION_SPECIALIZATIONS: usize = 512;

/// NativeIR pass that consumes the shared expansion budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SpecializationKind {
    /// Concrete instantiation of a private generic helper.
    Generic,
    /// Inlining of a private higher-order helper call.
    HigherOrder,
    /// Erasure and expansion of a statically known callable.
    StaticCallable,
    /// Inlining of a private projection-only helper.
    Projection,
}

impl SpecializationKind {
    /// Returns the stable diagnostic and accounting identity for this pass.
    fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::HigherOrder => "higher-order",
            Self::StaticCallable => "static-callable",
            Self::Projection => "projection",
        }
    }
}

/// Shared monotonic expansion budget for one canonically ordered application.
#[derive(Debug, Default)]
pub(super) struct SpecializationBudget {
    /// Number of expansions admitted by all specialization passes so far.
    consumed: usize,
}

impl SpecializationBudget {
    /// Reserves deterministic application-wide capacity for one expansion set.
    ///
    /// Inputs:
    /// - `kind`: specialization pass consuming the capacity.
    /// - `module`: canonical module currently being normalized.
    /// - `amount`: number of concrete expansions about to be committed.
    ///
    /// Output:
    /// - Success with the budget advanced, or a stable pre-codegen diagnostic.
    ///
    /// Transformation:
    /// - Performs checked addition before mutation and includes the canonical
    ///   module and pass in failures so reversed input order cannot alter which
    ///   expansion crosses the application ceiling.
    pub(super) fn reserve(
        &mut self,
        kind: SpecializationKind,
        module: &str,
        amount: usize,
    ) -> Result<(), String> {
        let next = self
            .consumed
            .checked_add(amount)
            .ok_or_else(|| specialization_budget_error(kind, module, usize::MAX))?;
        if next > MAX_APPLICATION_SPECIALIZATIONS {
            return Err(specialization_budget_error(kind, module, next));
        }
        self.consumed = next;
        Ok(())
    }

    /// Returns the number of expansions admitted for test and diagnostics use.
    #[cfg(test)]
    pub(super) fn consumed(&self) -> usize {
        self.consumed
    }
}

/// Formats one stable application specialization budget diagnostic.
fn specialization_budget_error(kind: SpecializationKind, module: &str, requested: usize) -> String {
    format!(
        "error[native_ir.application_specialization_budget]: application specialization requested {requested} expansions at `{module}` during {}; maximum is {MAX_APPLICATION_SPECIALIZATIONS}",
        kind.as_str()
    )
}
