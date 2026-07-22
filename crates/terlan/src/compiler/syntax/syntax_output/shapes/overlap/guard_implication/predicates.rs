use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PredicateFact {
    call: SyntaxExprOutput,
    positive: bool,
}

impl PredicateFact {
    pub(super) fn new_with_polarity(expression: &SyntaxExprOutput, positive: bool) -> Option<Self> {
        (expression.kind == SyntaxExprKind::Call).then(|| Self {
            call: expression.clone(),
            positive,
        })
    }

    fn contradicts(&self, other: &Self) -> bool {
        self.call == other.call && self.positive != other.positive
    }
}

pub(super) fn predicate_constraints_imply(
    later: &[PredicateFact],
    earlier: &[PredicateFact],
) -> bool {
    earlier.iter().all(|predicate| later.contains(predicate))
}

pub(super) fn merge_predicate_constraints(
    target: &mut Vec<PredicateFact>,
    source: Vec<PredicateFact>,
) -> bool {
    let mut impossible = false;
    for predicate in source {
        impossible |= target
            .iter()
            .any(|existing| existing.contradicts(&predicate));
        if !target.contains(&predicate) {
            target.push(predicate);
        }
    }
    impossible
}
