//! Visible constructor kinds preserve canonical and legacy type paths.

use super::*;

/// Collects visible type-constructor arities from the module and aliases.
///
/// Inputs:
/// - `module`: syntax-output module containing local type declarations.
/// - `aliases`: visible type aliases, including imported aliases.
///
/// Output:
/// - Map from source-visible type name to constructor arity.
///
/// Transformation:
/// - Starts with built-in generic type constructors, adds visible aliases, and
///   overlays local type declarations using their declared parameter count.
pub(super) fn type_constructor_arities(
    module: &SyntaxModuleOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, usize> {
    let mut arities = HashMap::from([
        ("List".to_string(), 1usize),
        ("Map".to_string(), 2usize),
        ("Array".to_string(), 2usize),
    ]);

    for (name, alias) in aliases {
        arities.insert(name.clone(), alias.params.len());
        if let Some((module, declared)) = name.rsplit_once('.') {
            arities.insert(
                crate::terlan_hir::public_type_path(module, declared),
                alias.params.len(),
            );
        }
        if let Some(last) = name.rsplit('.').next() {
            arities
                .entry(last.to_string())
                .or_insert(alias.params.len());
        }
    }

    for (name, params) in declared_parameters(module) {
        for path in [
            name.to_string(),
            format!("{}.{name}", module.module_name),
            crate::terlan_hir::public_type_path(&module.module_name, name),
        ] {
            arities.insert(path, params.len());
        }
    }

    arities
}

/// Collects visible type-constructor parameter variances.
///
/// Inputs:
/// - `module`: syntax-output module containing local type declarations.
/// - `aliases`: visible type aliases, including imported aliases.
///
/// Output:
/// - Map from source-visible type constructor name to parameter variances.
///
/// Transformation:
/// - Starts with built-in collection defaults, imports alias variance metadata,
///   and overlays local type declarations using their source parameter markers.
pub(super) fn type_constructor_variances(
    module: &SyntaxModuleOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, Vec<Variance>> {
    let mut variances = HashMap::from([
        ("List".to_string(), vec![Variance::Covariant]),
        (
            "Map".to_string(),
            vec![Variance::Covariant, Variance::Covariant],
        ),
        ("Array".to_string(), vec![Variance::Covariant]),
    ]);

    for (name, alias) in aliases {
        variances.insert(name.clone(), alias.param_variance.clone());
        if let Some((module, declared)) = name.rsplit_once('.') {
            variances.insert(
                crate::terlan_hir::public_type_path(module, declared),
                alias.param_variance.clone(),
            );
        }
        if let Some(last) = name.rsplit('.').next() {
            variances
                .entry(last.to_string())
                .or_insert_with(|| alias.param_variance.clone());
        }
    }

    for (name, params) in declared_parameters(module) {
        for path in [
            name.to_string(),
            format!("{}.{name}", module.module_name),
            crate::terlan_hir::public_type_path(&module.module_name, name),
        ] {
            variances.insert(path, type_param_variances(params));
        }
    }

    variances
}

/// Keeps generic metadata aligned across aliases, explicit constructors, and structs.
fn declared_parameters(module: &SyntaxModuleOutput) -> impl Iterator<Item = (&str, &[String])> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, params, .. }
            | SyntaxDeclarationPayload::Constructor { name, params, .. } => {
                Some((name.as_str(), params.as_slice()))
            }
            SyntaxDeclarationPayload::Struct {
                name,
                generic_params,
                ..
            } => Some((name.as_str(), generic_params.as_slice())),
            _ => None,
        })
}
