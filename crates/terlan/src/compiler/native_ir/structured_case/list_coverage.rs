//! Residual union types after exhaustive, unguarded list patterns.

use crate::terlan_typeck::{CorePattern, CoreType};

#[derive(Default)]
pub(super) struct ListCoverage {
    empty: bool,
    nonempty: bool,
}

#[cfg(test)]
#[path = "list_coverage_test.rs"]
mod tests;

impl ListCoverage {
    pub(super) fn record(&mut self, pattern: &CorePattern, guarded: bool) {
        if guarded {
            return;
        }
        self.empty |= matches!(pattern, CorePattern::List(items) if items.is_empty());
        self.nonempty |= matches!(pattern, CorePattern::ListCons { head, tail }
            if matches!(head.as_ref(), CorePattern::Var(_) | CorePattern::Wildcard)
                && matches!(tail.as_ref(), CorePattern::Var(_) | CorePattern::Wildcard));
    }

    pub(super) fn residual(
        &self,
        pattern: &CorePattern,
        core_type: Option<&CoreType>,
    ) -> Option<CoreType> {
        if !self.empty
            || !self.nonempty
            || !matches!(pattern, CorePattern::Var(_) | CorePattern::Wildcard)
        {
            return None;
        }
        let CoreType::Union(variants) = core_type? else {
            return None;
        };
        let remaining = variants
            .iter()
            .filter(|variant| {
                !matches!(variant, CoreType::List(_))
                    && !matches!(variant, CoreType::Apply { constructor, args }
                    if constructor.rsplit('.').next() == Some("List") && args.len() == 1)
            })
            .cloned()
            .collect::<Vec<_>>();
        match remaining.as_slice() {
            [only] => Some(only.clone()),
            [] => None,
            _ => Some(CoreType::Union(remaining)),
        }
    }
}
