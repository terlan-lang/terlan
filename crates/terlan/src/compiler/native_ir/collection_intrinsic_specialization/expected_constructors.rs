//! Expected-result annotations for structural Option, Result, and union constructors.

use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

pub(super) fn annotate_expected_structural_constructors(expr: &mut CoreExpr, expected: &CoreType) {
    if let CoreExpr::ConstructorCall { constructor, .. } = expr {
        if structural_constructor_matches(constructor, expected) {
            let constructor = std::mem::replace(expr, CoreExpr::Atom("Unit".to_string()));
            *expr = CoreExpr::Cast {
                expr: Box::new(constructor),
                target_type: expected.clone(),
            };
            return;
        }
    }
    match expr {
        CoreExpr::Let { body, .. } => annotate_expected_structural_constructors(body, expected),
        CoreExpr::If { clauses } => {
            for clause in clauses {
                annotate_expected_structural_constructors(&mut clause.body, expected);
            }
        }
        CoreExpr::Case { clauses, .. } => {
            for clause in clauses {
                annotate_expected_structural_constructors(&mut clause.body, expected);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            ..
        } => {
            annotate_expected_structural_constructors(body, expected);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                annotate_expected_structural_constructors(&mut clause.body, expected);
            }
        }
        _ => {}
    }
}

fn structural_constructor_matches(constructor: &str, expected: &CoreType) -> bool {
    let name = constructor.rsplit('.').next().unwrap_or(constructor);
    match expected {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            matches!(name, "None" | "Some")
        }
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            matches!(name, "Ok" | "Err")
        }
        CoreType::Union(variants) => variants.iter().any(|variant| {
            let CoreType::Tuple(elements) = variant else {
                return false;
            };
            let Some(first) = elements.first() else {
                return false;
            };
            let atom = match first {
                CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
                | CoreTupleTypeElem::Field {
                    ty: CoreType::AtomLiteral(atom),
                    ..
                } => atom,
                _ => return false,
            };
            (name == "Err" && atom == "error") || name.eq_ignore_ascii_case(atom)
        }),
        _ => false,
    }
}
