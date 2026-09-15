//! Preserve renamed singleton type values before import aliases leave the typed boundary.

use super::*;
use crate::terlan_syntax::{SyntaxPatternKind, SyntaxPatternOutput};

#[cfg(test)]
#[path = "imported_atoms_test.rs"]
mod tests;

pub(super) fn canonicalize(module: &mut SyntaxModuleOutput, resolved: &ResolvedModule) {
    let aliases = imported_type_aliases(resolved);
    let atoms = resolved
        .imported_types
        .iter()
        .filter_map(|(local, imported)| {
            if local == &imported.source_name {
                return None;
            }
            let alias = aliases.get(local)?;
            if alias.is_opaque || !alias.params.is_empty() {
                return None;
            }
            let Type::LiteralAtom(mut atom) = expand_type_aliases(&alias.body, &aliases) else {
                return None;
            };
            if imported.source_name == "Unit" && atom == "unit" {
                atom = "Unit".to_string();
            }
            Some((local.clone(), atom))
        })
        .collect::<HashMap<_, _>>();
    if atoms.is_empty() {
        return;
    }
    for declaration in &mut module.declarations {
        match &mut declaration.payload {
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            }
            | SyntaxDeclarationPayload::Method {
                params, clauses, ..
            } => {
                let mut defaults = HashSet::new();
                for param in params {
                    if let Some(default) = &mut param.default {
                        rewrite(default, &atoms, &defaults);
                    }
                    defaults.insert(param.name.clone());
                }
                for clause in clauses {
                    let mut bound = HashSet::new();
                    for pattern in &mut clause.patterns {
                        rewrite_pattern(pattern, &atoms);
                        collect_bindings(pattern, &mut bound);
                    }
                    if let Some(guard) = &mut clause.guard {
                        rewrite(guard, &atoms, &bound);
                    }
                    rewrite(&mut clause.body, &atoms, &bound);
                }
            }
            _ => {}
        }
    }
}

fn rewrite_pattern(pattern: &mut SyntaxPatternOutput, atoms: &HashMap<String, String>) {
    if pattern.kind == SyntaxPatternKind::Constructor && pattern.children.is_empty() {
        if let Some(atom) = pattern.text.as_ref().and_then(|name| atoms.get(name)) {
            pattern.kind = SyntaxPatternKind::Atom;
            pattern.text = Some(atom.clone());
        }
    }
    for child in &mut pattern.children {
        rewrite_pattern(child, atoms);
    }
    for field in &mut pattern.fields {
        rewrite_pattern(&mut field.value, atoms);
    }
}

fn rewrite(expr: &mut SyntaxExprOutput, atoms: &HashMap<String, String>, bound: &HashSet<String>) {
    // Syntax children can share their enclosing declaration's span. Lexical
    // scope, not span equality, decides whether a name denotes an imported atom.
    if expr.kind == SyntaxExprKind::Var {
        if let Some(atom) = expr.text.as_ref().and_then(|name| atoms.get(name)) {
            if !expr.text.as_ref().is_some_and(|name| bound.contains(name)) {
                expr.kind = SyntaxExprKind::Atom;
                expr.text = Some(atom.clone());
            }
        }
    }
    if matches!(
        expr.kind,
        SyntaxExprKind::Let | SyntaxExprKind::ListComprehension
    ) {
        let comprehension = expr.kind == SyntaxExprKind::ListComprehension;
        let mut success = bound.clone();
        for (index, pattern) in expr.patterns.iter_mut().enumerate() {
            if let Some(value) = expr.children.get_mut(index + usize::from(comprehension)) {
                rewrite(value, atoms, &success);
            }
            rewrite_pattern(pattern, atoms);
            collect_bindings(pattern, &mut success);
            if let Some(guard) = expr
                .let_guards
                .get_mut(index)
                .and_then(Option::as_deref_mut)
            {
                rewrite(guard, atoms, &success);
            }
        }
        let body_index = if comprehension {
            0
        } else {
            expr.patterns.len()
        };
        if let Some(body) = expr.children.get_mut(body_index) {
            rewrite(body, atoms, &success);
        }
        if comprehension {
            for guard in expr.children.iter_mut().skip(expr.patterns.len() + 1) {
                rewrite(guard, atoms, &success);
            }
        }
    } else {
        for child in &mut expr.children {
            rewrite(child, atoms, bound);
        }
    }
    for field in &mut expr.fields {
        rewrite(&mut field.value, atoms, bound);
    }
    for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
        let mut local = bound.clone();
        for pattern in &mut clause.patterns {
            rewrite_pattern(pattern, atoms);
            collect_bindings(pattern, &mut local);
        }
        if let Some(guard) = &mut clause.guard {
            rewrite(guard, atoms, &local);
        }
        rewrite(&mut clause.body, atoms, &local);
    }
    if let Some(after) = &mut expr.try_after {
        rewrite(&mut after.trigger, atoms, bound);
        rewrite(&mut after.body, atoms, bound);
    }
}

fn collect_bindings(pattern: &SyntaxPatternOutput, bound: &mut HashSet<String>) {
    if let Some(text) = &pattern.text {
        match pattern.kind {
            SyntaxPatternKind::Var | SyntaxPatternKind::Alias => {
                bound.insert(text.clone());
            }
            SyntaxPatternKind::StringCapture => {
                bound.insert(text.split(':').next().unwrap_or(text).trim().to_string());
            }
            _ => {}
        }
    }
    for child in &pattern.children {
        collect_bindings(child, bound);
    }
    for field in &pattern.fields {
        if pattern.kind == SyntaxPatternKind::BinaryLayout {
            bound.insert(field.key.clone());
        } else {
            collect_bindings(&field.value, bound);
        }
    }
}
