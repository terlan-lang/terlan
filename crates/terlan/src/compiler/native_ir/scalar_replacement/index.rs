//! Checked constant-index facts bound to one immutable fixed aggregate shape.

use crate::terlan_typeck::CoreExpr;

/// Proof that evaluating an index is total and selects a slot of this exact width.
pub(super) struct FixedIndex {
    position: usize,
    length: usize,
}

impl FixedIndex {
    /// Accepts only closed integer expressions whose checked evaluation succeeds.
    pub(super) fn prove(expression: &CoreExpr, length: usize) -> Option<Self> {
        let position = usize::try_from(evaluate(expression, 64)?).ok()?;
        (position < length).then_some(Self { position, length })
    }

    pub(super) fn position(&self) -> usize {
        self.position
    }

    /// A proof for another shape cannot select an alias from this one.
    pub(super) fn select<'a, T>(&self, slots: &'a [T]) -> Option<&'a T> {
        (slots.len() == self.length).then(|| &slots[self.position])
    }
}

fn evaluate(expression: &CoreExpr, remaining: usize) -> Option<i64> {
    let remaining = remaining.checked_sub(1)?;
    match expression {
        CoreExpr::Int(value) => Some(*value),
        CoreExpr::UnaryOp { operator, operand } if operator == "-" => {
            evaluate(operand, remaining)?.checked_neg()
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left = evaluate(left, remaining)?;
            let right = evaluate(right, remaining)?;
            match operator.as_str() {
                "+" => left.checked_add(right),
                "-" => left.checked_sub(right),
                "*" => left.checked_mul(right),
                "div" => left.checked_div(right),
                "rem" => left.checked_rem(right),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "index_test.rs"]
mod tests;
