use super::super::*;

/// Validates constructor clause ambiguity rules.
///
/// Inputs:
/// - `clauses`: constructor clauses parsed from one constructor declaration.
///
/// Output:
/// - `Ok(())` when clause shapes are unambiguous.
/// - `Err(ParseError)` for duplicate vararg clauses or overlapping fixed
///   arity/default ranges.
///
/// Transformation:
/// - Compares every pair of clauses so later phases can select constructors by
///   arity without resolving ambiguous source shapes.
pub(super) fn validate_constructor_clause_shapes(clauses: &[ConstructorClause]) -> ParseResult<()> {
    for (idx, left) in clauses.iter().enumerate() {
        for right in clauses.iter().skip(idx + 1) {
            let left_varargs = left.params.iter().any(|param| param.is_varargs);
            let right_varargs = right.params.iter().any(|param| param.is_varargs);

            if left_varargs && right_varargs {
                return Err(ParseError {
                    message: "constructor has ambiguous varargs clauses".to_string(),
                    span: right.span,
                });
            }

            if !left_varargs && !right_varargs {
                let left_range = constructor_clause_arity_range(left);
                let right_range = constructor_clause_arity_range(right);
                if ranges_overlap(left_range, right_range) {
                    return Err(ParseError {
                        message: "constructor has ambiguous arity clauses".to_string(),
                        span: right.span,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Returns the accepted arity range for one constructor clause.
///
/// Inputs:
/// - `clause`: constructor clause with fixed, defaulted, and possible vararg
///   parameters already parsed.
///
/// Output:
/// - `(minimum, maximum)` accepted arity for non-vararg ambiguity checks.
///
/// Transformation:
/// - Treats defaulted parameters as optional and required parameters as part of
///   the minimum arity while preserving the declared maximum parameter count.
fn constructor_clause_arity_range(clause: &ConstructorClause) -> (usize, usize) {
    let max = clause.params.len();
    let min = clause
        .params
        .iter()
        .filter(|param| param.default.is_none())
        .count();
    (min, max)
}

/// Returns whether two inclusive arity ranges overlap.
///
/// Inputs:
/// - `left` and `right`: `(minimum, maximum)` arity ranges.
///
/// Output:
/// - `true` when a call arity could match both ranges.
///
/// Transformation:
/// - Applies inclusive range overlap logic for constructor clause ambiguity
///   detection.
fn ranges_overlap(left: (usize, usize), right: (usize, usize)) -> bool {
    left.0 <= right.1 && right.0 <= left.1
}
