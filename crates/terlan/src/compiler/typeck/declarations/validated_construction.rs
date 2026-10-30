use super::super::*;

/// Enforces opt-in validated construction at public constructor boundaries.
///
/// Inputs:
/// - The checked module plus its visible local and imported type identities.
///
/// Output:
/// - Diagnostics for public constructors that expose an `@validated` struct
///   without a standard fallible result or an explicit trust boundary.
///
/// Transformation:
/// - Collects only local structs carrying the marker, parses constructor return
///   types without expanding `Option` or `Result`, and accepts standard-library
///   `Option[T]` and `Result[T, E]`. Existing unannotated types are unaffected.
pub(super) fn check_validated_construction_contracts(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
) -> Vec<Diagnostic> {
    let validated_types = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Struct { name, .. } = &declaration.payload else {
                return None;
            };
            declaration_has_marker_annotation(declaration, &["validated"]).then(|| name.clone())
        })
        .collect::<HashSet<_>>();
    if validated_types.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            params,
            is_public: true,
            clauses,
        } = &declaration.payload
        else {
            continue;
        };
        if declaration_has_marker_annotation(declaration, &["trusted"]) {
            continue;
        }

        let mut vars = HashMap::new();
        let mut next_var = 0;
        for param in params {
            vars.insert(normalize_type_param_name(param), next_var);
            next_var += 1;
        }

        for clause in clauses {
            let Some(parsed) = parse_type_expr(
                &clause.return_type.text,
                alias_names,
                &mut vars,
                &mut next_var,
            ) else {
                continue;
            };
            let return_type = qualify_type_names(&parsed, imported_type_names);
            let Some(validated_name) =
                first_local_validated_type(&return_type, &validated_types, &module.module_name)
            else {
                continue;
            };
            if is_checked_constructor_result(&return_type, &validated_types, &module.module_name) {
                continue;
            }

            diagnostics.push(Diagnostic {
                span: clause.return_type.span.into(),
                message: format!(
                    "public constructor `{name}` exposes validated type `{validated_name}` without a checked result; return Option[{validated_name}] or Result[{validated_name}, E], or mark the constructor @trusted"
                ),
                severity: DiagSeverity::Error,
            });
        }
    }
    diagnostics
}

/// Reports the first local validated type nested in a return type.
fn first_local_validated_type<'a>(
    ty: &'a Type,
    validated_types: &'a HashSet<String>,
    current_module: &str,
) -> Option<&'a str> {
    match ty {
        Type::Named { module, name, args } => {
            if validated_types.contains(name)
                && module
                    .as_deref()
                    .is_none_or(|module| module == current_module)
            {
                return Some(name);
            }
            args.iter()
                .find_map(|arg| first_local_validated_type(arg, validated_types, current_module))
        }
        Type::List(item) => first_local_validated_type(item, validated_types, current_module),
        Type::Tuple(items) | Type::Union(items) => items
            .iter()
            .find_map(|item| first_local_validated_type(item, validated_types, current_module)),
        Type::Map(fields) => fields.iter().find_map(|field| {
            first_local_validated_type(&field.value, validated_types, current_module)
        }),
        Type::Array { elem, .. } => {
            first_local_validated_type(elem, validated_types, current_module)
        }
        Type::Apply { args, .. } => args
            .iter()
            .find_map(|arg| first_local_validated_type(arg, validated_types, current_module)),
        Type::Existential { body, .. } => {
            first_local_validated_type(body, validated_types, current_module)
        }
        Type::Function { params, ret } => params
            .iter()
            .chain(std::iter::once(ret.as_ref()))
            .find_map(|item| first_local_validated_type(item, validated_types, current_module)),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::LiteralBool(_)
        | Type::Var(_)
        | Type::Placeholder => None,
    }
}

/// Accepts only standard `Option` and `Result` as checked public boundaries.
fn is_checked_constructor_result(
    ty: &Type,
    validated_types: &HashSet<String>,
    current_module: &str,
) -> bool {
    let Type::Named { module, name, args } = ty else {
        return false;
    };
    let standard_container = match (name.as_str(), module.as_deref()) {
        ("Option", Some("std.core" | "std.core.Option")) if args.len() == 1 => true,
        ("Result", Some("std.core" | "std.core.Result")) if args.len() == 2 => true,
        _ => false,
    };
    if !standard_container {
        return false;
    }

    first_local_validated_type(&args[0], validated_types, current_module).is_some()
        && args
            .iter()
            .skip(1)
            .all(|arg| first_local_validated_type(arg, validated_types, current_module).is_none())
}
