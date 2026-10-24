//! A public singleton atom has one declaration; other modules import its name.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::terlan_hir::ModuleInterface;
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};
use crate::terlan_typeck::{DiagSeverity, Diagnostic, Type};

/// Checks local declarations against known providers and imported providers
/// against each other. Cached copies of the current module do not count twice.
pub(crate) fn check(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> Vec<Diagnostic> {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for interface in interfaces
        .values()
        .filter(|i| i.module != module.module_name)
    {
        for (name, body) in &interface.type_bodies {
            if interface.public_types.contains(name) && !interface.opaque_types.contains(name) {
                if let [body] = body.as_slice() {
                    if let Some(atom) = singleton_atom(body) {
                        owners
                            .entry(atom)
                            .or_default()
                            .insert(type_path(&interface.module, name));
                    }
                }
            }
        }
    }
    let mut diagnostics = Vec::new();
    let mut imported: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut pending = BTreeSet::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Import {
            module_name, items, ..
        } = &declaration.payload
        {
            pending.insert(module_name.clone());
            pending.extend(
                items
                    .iter()
                    .map(|item| format!("{module_name}.{}", item.name)),
            );
        }
    }
    let mut visited = BTreeSet::new();
    while let Some(mut module_name) = pending.pop_first() {
        // Selected/default imports can name a declaration after its module.
        while !interfaces.contains_key(&module_name) {
            let Some((parent, _)) = module_name.rsplit_once('.') else {
                break;
            };
            module_name = parent.to_string();
        }
        if module_name == module.module_name || !visited.insert(module_name.clone()) {
            continue;
        }
        let Some(interface) = interfaces.get(&module_name) else {
            continue;
        };
        pending.extend(referenced_interfaces(interface));
        for (name, body) in &interface.type_bodies {
            if !interface.public_types.contains(name) || interface.opaque_types.contains(name) {
                continue;
            }
            let [body] = body.as_slice() else { continue };
            let Some(atom) = singleton_atom(body) else {
                continue;
            };
            imported
                .entry(atom)
                .or_default()
                .insert(type_path(&module_name, name));
        }
    }
    // Sort identities so the diagnostic is independent of map and import order.
    for (atom, declarations) in imported {
        if declarations.len() > 1 {
            let mut declarations = declarations.into_iter();
            let original = declarations.next().expect("multiple declarations");
            for duplicate in declarations {
                diagnostics.push(diagnostic(module.span.into(), &atom, &duplicate, &original));
            }
        }
    }
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Type {
            name,
            is_public: true,
            is_opaque: false,
            variants,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let [body] = variants.as_slice() else {
            continue;
        };
        let Some(atom) = singleton_atom(&body.text) else {
            continue;
        };
        let identity = type_path(&module.module_name, name);
        let existing = owners.entry(atom.clone()).or_default();
        if let Some(original) = existing.first() {
            diagnostics.push(diagnostic(
                declaration.span.into(),
                &atom,
                &identity,
                original,
            ));
        }
        existing.insert(identity);
    }
    diagnostics
}

/// Follow public type references through facade signatures and aliases. The
/// language lexer keeps quoted atom payloads and documentation out of this graph.
fn referenced_interfaces(interface: &ModuleInterface) -> BTreeSet<String> {
    use crate::terlan_syntax::TokenKind;
    let mut referenced = BTreeSet::new();
    let Ok(tokens) = crate::terlan_syntax::lex(&interface.to_terlan_interface_type_text()) else {
        return referenced;
    };
    let identifier =
        |kind: &TokenKind| matches!(kind, TokenKind::Ident | TokenKind::Var | TokenKind::Atom);
    let mut index = 0;
    while index < tokens.len() {
        if !identifier(&tokens[index].kind) {
            index += 1;
            continue;
        }
        let mut name = tokens[index].text.clone();
        index += 1;
        while index + 1 < tokens.len()
            && tokens[index].kind == TokenKind::Dot
            && identifier(&tokens[index + 1].kind)
        {
            name.push('.');
            name.push_str(&tokens[index + 1].text);
            index += 2;
        }
        if name.contains('.') {
            referenced.insert(name);
        }
    }
    referenced
}

/// Uses the same parser as inference, including whitespace, escapes, and parentheses.
fn singleton_atom(text: &str) -> Option<String> {
    let text = text.trim();
    if !text.starts_with("Atom") && !text.starts_with('(') {
        return None;
    }
    match crate::terlan_typeck::type_system::parse_type_expr(
        text,
        &Default::default(),
        &mut Default::default(),
        &mut 0,
    ) {
        Some(Type::LiteralAtom(atom)) => Some(atom),
        _ => None,
    }
}

fn type_path(module: &str, name: &str) -> String {
    crate::terlan_hir::public_type_path(module, name)
}

fn diagnostic(
    span: crate::terlan_syntax::span::Span,
    atom: &str,
    duplicate: &str,
    original: &str,
) -> Diagnostic {
    Diagnostic {
        span,
        message: format!(
            "duplicate_atom_alias: `{duplicate}` redeclares Atom[{atom:?}], already declared by `{original}`; import the shared alias instead"
        ),
        severity: DiagSeverity::Error,
    }
}

#[cfg(test)]
#[path = "atom_aliases_test.rs"]
mod tests;
