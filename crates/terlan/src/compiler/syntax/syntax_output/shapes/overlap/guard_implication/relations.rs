use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RelationFact {
    left: String,
    operator: &'static str,
    right: String,
}

impl RelationFact {
    pub(super) fn new(left: String, operator: &str, right: String) -> Option<Self> {
        let (left, operator, right) = match operator {
            "<" => (left, "<", right),
            "<=" => (left, "<=", right),
            ">" => (right, "<", left),
            ">=" => (right, "<=", left),
            "==" if left <= right => (left, "==", right),
            "==" => (right, "==", left),
            "!=" if left <= right => (left, "!=", right),
            "!=" => (right, "!=", left),
            _ => return None,
        };
        Some(Self {
            left,
            operator,
            right,
        })
    }
}

pub(super) fn relation_constraints_imply(later: &[RelationFact], earlier: &[RelationFact]) -> bool {
    let later_closure = RelationClosure::new(later);
    if later_closure.is_impossible(later) {
        return true;
    }
    if RelationClosure::new(earlier).is_impossible(earlier) {
        return false;
    }
    earlier.iter().all(|fact| later_closure.implies(fact))
}

struct RelationClosure {
    names: Vec<String>,
    paths: Vec<Vec<Option<bool>>>,
    inequalities: BTreeSet<(String, String)>,
}

impl RelationClosure {
    fn new(facts: &[RelationFact]) -> Self {
        let names = facts
            .iter()
            .flat_map(|fact| [&fact.left, &fact.right])
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut paths = vec![vec![None; names.len()]; names.len()];
        for (index, row) in paths.iter_mut().enumerate() {
            row[index] = Some(false);
        }
        let inequalities = facts
            .iter()
            .filter(|fact| fact.operator == "!=")
            .map(|fact| (fact.left.clone(), fact.right.clone()))
            .collect();
        let mut closure = Self {
            names,
            paths,
            inequalities,
        };
        for fact in facts {
            match fact.operator {
                "<" => closure.add_path(&fact.left, &fact.right, true),
                "<=" => closure.add_path(&fact.left, &fact.right, false),
                "==" => {
                    closure.add_path(&fact.left, &fact.right, false);
                    closure.add_path(&fact.right, &fact.left, false);
                }
                "!=" => {}
                _ => unreachable!("relation facts are normalized when constructed"),
            }
        }
        closure.complete();
        closure
    }

    fn add_path(&mut self, left: &str, right: &str, strict: bool) {
        let (Some(left), Some(right)) = (self.index(left), self.index(right)) else {
            return;
        };
        self.paths[left][right] = Some(self.paths[left][right].unwrap_or(false) || strict);
    }

    fn complete(&mut self) {
        for middle in 0..self.names.len() {
            for left in 0..self.names.len() {
                let Some(left_strict) = self.paths[left][middle] else {
                    continue;
                };
                for right in 0..self.names.len() {
                    let Some(right_strict) = self.paths[middle][right] else {
                        continue;
                    };
                    let strict = left_strict || right_strict;
                    self.paths[left][right] =
                        Some(self.paths[left][right].unwrap_or(false) || strict);
                }
            }
        }
    }

    fn implies(&self, fact: &RelationFact) -> bool {
        match fact.operator {
            "<" => self.path(&fact.left, &fact.right) == Some(true),
            "<=" => self.path(&fact.left, &fact.right).is_some(),
            "==" => self.equivalent(&fact.left, &fact.right),
            "!=" => {
                self.path(&fact.left, &fact.right) == Some(true)
                    || self.path(&fact.right, &fact.left) == Some(true)
                    || self.explicit_inequality(fact)
            }
            _ => false,
        }
    }

    fn is_impossible(&self, facts: &[RelationFact]) -> bool {
        self.paths
            .iter()
            .enumerate()
            .any(|(index, row)| row[index] == Some(true))
            || facts
                .iter()
                .any(|fact| fact.operator == "!=" && self.equivalent(&fact.left, &fact.right))
    }

    fn equivalent(&self, left: &str, right: &str) -> bool {
        self.path(left, right) == Some(false) && self.path(right, left) == Some(false)
    }

    fn explicit_inequality(&self, fact: &RelationFact) -> bool {
        self.inequalities
            .iter()
            .any(|(left, right)| left == &fact.left && right == &fact.right)
    }

    fn path(&self, left: &str, right: &str) -> Option<bool> {
        self.paths[self.index(left)?][self.index(right)?]
    }

    fn index(&self, name: &str) -> Option<usize> {
        self.names
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .ok()
    }
}
