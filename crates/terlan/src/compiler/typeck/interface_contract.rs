//! Validation of handwritten module signatures and implementation agreement.

use super::*;
use crate::terlan_hir::{syntax_module_output_to_interface, FunctionSignature};

/// Checks an implementation against an independent public interface. Additional
/// implementation declarations remain local to its inferred interface; consumers
/// must use the handwritten interface supplied by the compilation driver.
pub(crate) fn check_module_interface_contract(
    module: &SyntaxModuleOutput,
    expected: &ModuleInterface,
    expected_resolved: &ResolvedModule,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let expected = qualify_contract_interface(expected.clone(), expected_resolved);
    let expected = &expected;
    let actual_public_types = syntax_module_output_to_interface(module).public_types;
    // Comparison needs the implementation's complete local alias environment,
    // including private representation aliases. This temporary view is never
    // published and cannot satisfy a public type obligation with a private type.
    let mut comparison_module = module.clone();
    for declaration in &mut comparison_module.declarations {
        match &mut declaration.payload {
            SyntaxDeclarationPayload::Type { is_public, .. }
            | SyntaxDeclarationPayload::Struct { is_public, .. } => *is_public = true,
            _ => {}
        }
    }
    let actual = qualify_contract_interface(
        syntax_module_output_to_interface(&comparison_module),
        resolved,
    );
    let mut diagnostics = Vec::new();
    let mut report = |message: String| {
        diagnostics.push(Diagnostic {
            span: module.span.into(),
            message: format!("interface_contract: {message}"),
            severity: DiagSeverity::Error,
        });
    };
    if actual.module != expected.module {
        report(format!(
            "expected module `{}`, found `{}`",
            expected.module, actual.module
        ));
        return diagnostics;
    }
    let aliases = imported_type_aliases(resolved);
    let mut names = expected.public_types.iter().collect::<Vec<_>>();
    names.sort();
    for name in names {
        if !actual_public_types.contains(name) {
            report(format!("missing public type `{name}`"));
            continue;
        }
        let expected_params = expected.type_params.get(name).cloned().unwrap_or_default();
        let actual_params = actual.type_params.get(name).cloned().unwrap_or_default();
        if generic_shape(&expected_params) != generic_shape(&actual_params) {
            report(format!("type parameters differ for `{name}`"));
        }
        if expected.opaque_types.contains(name) {
            if !actual.opaque_types.contains(name) {
                report(format!("type `{name}` must remain opaque"));
            }
            continue;
        }
        if let Some(union) = expected.valued_unions.get(name) {
            let matches = actual.valued_unions.get(name).is_some_and(|found| {
                type_matches(
                    &union.representation,
                    &expected_params,
                    expected,
                    &found.representation,
                    &actual_params,
                    &actual,
                    &aliases,
                ) && union.arms.len() == found.arms.len()
                    && union.arms.iter().zip(&found.arms).all(|(left, right)| {
                        left.name == right.name && left.value_text == right.value_text
                    })
            });
            if !matches {
                report(format!(
                    "valued union representation or members differ for `{name}`"
                ));
            }
            continue;
        }
        if let Some(fields) = expected.struct_fields.get(name) {
            let matches = actual.struct_fields.get(name).is_some_and(|found| {
                fields.len() == found.len()
                    && fields.iter().zip(found).all(|(left, right)| {
                        left.name == right.name
                            && left.is_private == right.is_private
                            && type_matches(
                                &left.annotation,
                                &expected_params,
                                expected,
                                &right.annotation,
                                &actual_params,
                                &actual,
                                &aliases,
                            )
                    })
            });
            if !matches {
                report(format!("struct fields differ for `{name}`"));
            }
        } else if let Some(body) = expected.type_bodies.get(name) {
            let matches = !actual.opaque_types.contains(name)
                && actual.type_bodies.get(name).is_some_and(|found| {
                    type_matches(
                        &body.join(" | "),
                        &expected_params,
                        expected,
                        &found.join(" | "),
                        &actual_params,
                        &actual,
                        &aliases,
                    )
                });
            if !matches {
                report(format!(
                    "representation differs for transparent type `{name}`"
                ));
            }
        }
    }
    let mut functions = expected.function_overloads.iter().collect::<Vec<_>>();
    functions.sort_by_key(|(key, _)| *key);
    for ((name, arity), signatures) in functions {
        let declared_count = signatures
            .iter()
            .filter(|signature| signature.public)
            .count();
        if actual
            .function_overloads
            .get(&(name.clone(), *arity))
            .is_some_and(|found| found.len() > declared_count)
        {
            report(format!(
                "additional overloads of `{name}/{arity}` are not declared in the interface"
            ));
        }
        for signature in signatures.iter().filter(|signature| signature.public) {
            let matches = actual
                .function_overloads
                .get(&(name.clone(), *arity))
                .is_some_and(|candidates| {
                    candidates.iter().any(|candidate| {
                        candidate.public
                            && function_matches(signature, expected, candidate, &actual, &aliases)
                    })
                });
            if !matches {
                report(format!("missing or incompatible public signature `{name}/{arity}` (including purity, receiver, bounds, and defaults)"));
            }
        }
    }
    // The remaining signature forms carry compile-time values or structured
    // declarations. Compare their canonical public representation, never bodies
    // of ordinary functions or documentation. Restrict to declared items so an
    // implementation may have additional helpers without exposing them.
    let mut declared = expected.clone();
    let mut provided = actual.clone();
    for interface in [&mut declared, &mut provided] {
        interface.public_types.clear();
        interface.private_types.clear();
        interface.functions.clear();
        interface.function_overloads.clear();
        for constructor in interface.constructors.values_mut().flatten() {
            constructor.body.clear();
        }
    }
    provided
        .constants
        .retain(|key, _| declared.constants.contains_key(key));
    provided
        .const_functions
        .retain(|key, _| declared.const_functions.contains_key(key));
    provided
        .expression_macros
        .retain(|key, _| declared.expression_macros.contains_key(key));
    provided
        .shapes
        .retain(|key, _| declared.shapes.contains_key(key));
    provided
        .traits
        .retain(|key, _| declared.traits.contains_key(key));
    provided
        .constructors
        .retain(|key, _| declared.constructors.contains_key(key));
    provided
        .trait_conformances
        .retain(|item| declared.trait_conformances.contains(item));
    provided
        .associated_constants
        .retain(|key, _| declared.associated_constants.contains_key(key));
    if declared.to_terlan_interface_type_text() != provided.to_terlan_interface_type_text() {
        report(
            "public constants, constructors, shapes, traits, or conformance declarations differ"
                .to_string(),
        );
    }
    diagnostics
}

/// Resolve dependency identities separately in the contract and implementation.
/// Equal short names from different providers must never satisfy a signature.
fn qualify_contract_interface(
    mut interface: ModuleInterface,
    resolved: &ResolvedModule,
) -> ModuleInterface {
    let mut refs = resolved
        .imported_types
        .iter()
        .chain(resolved.imported_traits.iter())
        .map(|(name, imported)| {
            (
                name.clone(),
                format!("{}.{}", imported.source_module, imported.source_name),
            )
        })
        .collect::<HashMap<_, _>>();
    for name in interface.traits.keys() {
        refs.insert(name.clone(), format!("{}.{}", interface.module, name));
    }
    for signatures in interface.function_overloads.values_mut() {
        for signature in signatures {
            let mut refs = refs.clone();
            for param in &signature.generic_params {
                refs.remove(&normalize_type_param_name(param));
            }
            for param in &mut signature.params {
                param.annotation =
                    crate::terlan_hir::qualify_syntax_type_text(&param.annotation, &refs);
            }
            signature.return_type =
                crate::terlan_hir::qualify_syntax_type_text(&signature.return_type, &refs);
            for bound in &mut signature.generic_bounds {
                *bound = crate::terlan_hir::qualify_syntax_type_text(bound, &refs);
            }
            for param in &mut signature.generic_params {
                *param = crate::terlan_hir::qualify_syntax_type_text(param, &refs);
            }
        }
    }
    for (name, bodies) in &mut interface.type_bodies {
        let mut refs = refs.clone();
        if let Some(params) = interface.type_params.get(name) {
            for param in params {
                refs.remove(&normalize_type_param_name(param));
            }
        }
        for body in bodies {
            *body = crate::terlan_hir::qualify_syntax_type_text(body, &refs);
        }
    }
    for (name, fields) in &mut interface.struct_fields {
        let mut refs = refs.clone();
        if let Some(params) = interface.type_params.get(name) {
            for param in params {
                refs.remove(&normalize_type_param_name(param));
            }
        }
        for field in fields {
            field.annotation =
                crate::terlan_hir::qualify_syntax_type_text(&field.annotation, &refs);
        }
    }
    interface
}

fn function_matches(
    expected: &FunctionSignature,
    expected_interface: &ModuleInterface,
    actual: &FunctionSignature,
    actual_interface: &ModuleInterface,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    if expected.receiver_method != actual.receiver_method
        || expected.receiver_mutable != actual.receiver_mutable
        || (expected.pure && !actual.pure)
        || generic_shape(&expected.generic_params) != generic_shape(&actual.generic_params)
        || expected.params.len() != actual.params.len()
        || !expected
            .params
            .iter()
            .zip(&actual.params)
            .all(|(left, right)| {
                left.is_mutable == right.is_mutable && left.default_text == right.default_text
            })
    {
        return false;
    }
    let Some(left) = parse_interface_signature(expected, expected_interface, aliases) else {
        return false;
    };
    let Some(right) = parse_interface_signature(actual, actual_interface, aliases) else {
        return false;
    };
    if left.params != right.params
        || left.ret != right.ret
        || left.bounds.len() != right.bounds.len()
    {
        return false;
    }
    let mut unmatched = right.bounds.iter().collect::<Vec<_>>();
    for bound in &left.bounds {
        let Some(index) = unmatched.iter().position(|candidate| {
            bound.trait_name == candidate.trait_name && bound.trait_args == candidate.trait_args
        }) else {
            return false;
        };
        unmatched.remove(index);
    }
    true
}

fn type_matches(
    left: &str,
    left_params: &[String],
    left_interface: &ModuleInterface,
    right: &str,
    right_params: &[String],
    right_interface: &ModuleInterface,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let signature = |text: &str, params: &[String]| FunctionSignature {
        name: "type_contract".to_string(),
        generic_params: params.to_vec(),
        params: Vec::new(),
        return_type: text.to_string(),
        generic_bounds: Vec::new(),
        receiver_method: false,
        receiver_mutable: false,
        public: true,
        pure: false,
        docs: Vec::new(),
    };
    function_matches(
        &signature(left, left_params),
        left_interface,
        &signature(right, right_params),
        right_interface,
        aliases,
    )
}

fn generic_shape(params: &[String]) -> Vec<String> {
    let names = params
        .iter()
        .enumerate()
        .map(|(index, param)| (normalize_type_param_name(param), format!("T{index}")))
        .collect::<HashMap<_, _>>();
    params
        .iter()
        .map(|param| {
            crate::terlan_hir::qualify_syntax_type_text(param, &names)
                .split_whitespace()
                .collect::<String>()
        })
        .collect()
}

/// Rejects unknown names and arities in interface and native signatures rather
/// than interpreting a misspelled declared type as an inferred variable.
pub(super) fn check_interface_annotations(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let is_interface = module.source_kind == crate::terlan_syntax::SyntaxSourceKind::Interface;
    if !is_interface
        && !module.declarations.iter().any(|declaration| {
            crate::compiler::native_bindings::declaration_operation(
                &module.module_name,
                declaration,
            )
            .is_some()
        })
    {
        return Vec::new();
    }
    let mut names = collect_syntax_type_names(module);
    names.extend(resolved.imported_types.keys().cloned());
    names.extend(primitive_type_names());
    names.extend(
        [
            "Binary", "Number", "Atom", "Term", "Dynamic", "Never", "Text", "List", "Array",
        ]
        .map(str::to_string),
    );
    let mut arities = names
        .iter()
        .map(|name| (name.clone(), 0))
        .collect::<HashMap<_, _>>();
    arities.insert("List".to_string(), 1);
    arities.insert("Array".to_string(), 2);
    let own = syntax_module_output_to_interface(module);
    for interface in resolved.interface_map.values().chain(std::iter::once(&own)) {
        for name in &interface.public_types {
            for path in [
                format!("{}.{}", interface.module, name),
                crate::terlan_hir::public_type_path(&interface.module, name),
            ] {
                arities.insert(path, interface.type_params.get(name).map_or(0, Vec::len));
            }
        }
    }
    for (name, imported) in &resolved.imported_types {
        if let Some(arity) = arities
            .get(&format!(
                "{}.{}",
                imported.source_module, imported.source_name
            ))
            .copied()
        {
            arities.insert(name.clone(), arity);
        }
    }
    for name in &names {
        if let Some(params) = own.type_params.get(name) {
            arities.insert(name.clone(), params.len());
        }
    }
    let mut diagnostics = if is_interface {
        check_interface_imports(module, resolved)
    } else {
        Vec::new()
    };
    for declaration in &module.declarations {
        if !is_interface
            && crate::compiler::native_bindings::declaration_operation(
                &module.module_name,
                declaration,
            )
            .is_none()
        {
            continue;
        }
        let mut annotations = Vec::new();
        let mut generics = Vec::new();
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                generic_params,
                params,
                return_type,
                ..
            }
            | SyntaxDeclarationPayload::Method {
                generic_params,
                params,
                return_type,
                ..
            } => {
                generics.extend(generic_params.iter().cloned());
                annotations.extend(params.iter().map(|param| &param.annotation));
                annotations.push(return_type);
                if let SyntaxDeclarationPayload::Method { receiver, .. } = &declaration.payload {
                    annotations.push(&receiver.annotation);
                    if let Some((owner, _)) = split_named_type(&receiver.annotation.text) {
                        if let Some(params) = own.type_params.get(owner) {
                            generics.extend(params.iter().cloned());
                        }
                    }
                }
            }
            SyntaxDeclarationPayload::Type {
                params, variants, ..
            } => {
                generics.extend(params.iter().cloned());
                annotations.extend(variants);
            }
            SyntaxDeclarationPayload::Struct {
                generic_params,
                fields,
                ..
            } => {
                generics.extend(generic_params.iter().cloned());
                annotations.extend(fields.iter().map(|field| &field.annotation));
            }
            SyntaxDeclarationPayload::Constant { annotation, .. } => annotations.push(annotation),
            SyntaxDeclarationPayload::ConstFunction {
                params,
                return_type,
                ..
            } => {
                annotations.extend(params.iter().map(|param| &param.annotation));
                annotations.push(return_type);
            }
            SyntaxDeclarationPayload::Constructor {
                params, clauses, ..
            } => {
                generics.extend(params.iter().cloned());
                for clause in clauses {
                    annotations.extend(clause.params.iter().map(|param| &param.annotation));
                    annotations.push(&clause.return_type);
                }
            }
            SyntaxDeclarationPayload::Trait {
                params,
                methods,
                constants,
                ..
            } => {
                generics.extend(params.iter().cloned());
                for method in methods {
                    let mut params = params.clone();
                    params.extend(method.generic_params.iter().cloned());
                    let mut types = method
                        .params
                        .iter()
                        .map(|param| &param.annotation)
                        .collect::<Vec<_>>();
                    types.push(&method.return_type);
                    validate_annotations(types, &params, &names, &arities, &mut diagnostics);
                }
                annotations.extend(constants.iter().map(|constant| &constant.annotation));
            }
            SyntaxDeclarationPayload::TraitImpl {
                generic_params,
                for_type,
                methods,
                ..
            } => {
                generics.extend(generic_params.iter().cloned());
                annotations.push(for_type);
                for method in methods {
                    annotations.extend(method.params.iter().map(|param| &param.annotation));
                    annotations.push(&method.return_type);
                }
            }
            _ => {}
        }
        validate_annotations(annotations, &generics, &names, &arities, &mut diagnostics);
    }
    diagnostics
}

/// Bodyless declarations still need valid imports. Ordinary function-call
/// checking cannot diagnose a missing callable when no body uses it yet.
fn check_interface_imports(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: crate::terlan_syntax::SyntaxImportKind::Module,
            module_name,
            items,
            is_type,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        for item in items {
            let child_module = format!("{module_name}.{}", item.name);
            let exists = resolved.interface_map.contains_key(&child_module)
                || resolved
                    .interface_map
                    .get(module_name)
                    .is_some_and(|interface| {
                        item.name == "*"
                            || interface.public_types.contains(&item.name)
                            || interface.traits.contains_key(&item.name)
                            || (!is_type
                                && (interface.constants.contains_key(&item.name)
                                    || interface
                                        .const_functions
                                        .keys()
                                        .any(|(name, _)| name == &item.name)
                                    || interface
                                        .expression_macros
                                        .keys()
                                        .any(|(name, _)| name == &item.name)
                                    || interface.shapes.contains_key(&item.name)
                                    || interface.constructors.get(&item.name).is_some_and(
                                        |signatures| {
                                            signatures.iter().any(|signature| signature.public)
                                        },
                                    )
                                    || interface.function_overloads.iter().any(
                                        |((name, _), signatures)| {
                                            name == &item.name
                                                && signatures
                                                    .iter()
                                                    .any(|signature| signature.public)
                                        },
                                    )
                                    || interface.functions.iter().any(|((name, _), signature)| {
                                        name == &item.name && signature.public
                                    })))
                    })
                || (item.name == "*"
                    && resolved
                        .interface_map
                        .keys()
                        .any(|name| name.starts_with(&format!("{module_name}."))));
            if !exists {
                diagnostics.push(Diagnostic {
                    span: item.span.into(),
                    message: format!(
                        "cannot find public import `{module_name}.{}` in interface",
                        item.name
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }
    diagnostics
}

fn validate_annotations(
    annotations: Vec<&crate::terlan_syntax::SyntaxTypeOutput>,
    generics: &[String],
    names: &HashSet<String>,
    arities: &HashMap<String, usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let declared = generics
        .iter()
        .map(|param| normalize_type_param_name(param))
        .collect::<HashSet<_>>();
    for annotation in annotations {
        let mut vars = declared
            .iter()
            .enumerate()
            .map(|(id, name)| (name.clone(), id))
            .collect::<HashMap<_, _>>();
        let mut next = vars.len();
        let parsed = parse_type_expr(&annotation.text, names, &mut vars, &mut next);
        let mut errors = vars
            .keys()
            .filter(|name| !declared.contains(*name))
            .map(|name| format!("unknown type `{name}` in interface signature"))
            .collect::<Vec<_>>();
        if let Some(ty) = parsed {
            check_named_type(&ty, names, arities, &mut errors);
        } else {
            errors.push(format!("invalid interface type `{}`", annotation.text));
        }
        errors.sort();
        errors.dedup();
        diagnostics.extend(errors.into_iter().map(|message| Diagnostic {
            span: annotation.span.into(),
            message,
            severity: DiagSeverity::Error,
        }));
    }
}

fn check_named_type(
    ty: &Type,
    names: &HashSet<String>,
    arities: &HashMap<String, usize>,
    errors: &mut Vec<String>,
) {
    match ty {
        Type::Named { module, name, args } => {
            let key = module
                .as_ref()
                .map_or_else(|| name.clone(), |module| format!("{module}.{name}"));
            if let Some(arity) = arities.get(&key) {
                if args.len() != *arity {
                    errors.push(format!(
                        "interface type `{key}` expects {arity} argument(s), found {}",
                        args.len()
                    ));
                }
            } else if !names.contains(&key) {
                errors.push(format!("unknown type `{key}` in interface signature"));
            }
            for arg in args {
                check_named_type(arg, names, arities, errors);
            }
        }
        Type::List(ty) | Type::Existential { body: ty, .. } | Type::Array { elem: ty, .. } => {
            check_named_type(ty, names, arities, errors)
        }
        Type::Tuple(types) | Type::Union(types) | Type::Apply { args: types, .. } => {
            for ty in types {
                check_named_type(ty, names, arities, errors);
            }
        }
        Type::Map(fields) => {
            for field in fields {
                check_named_type(&field.value, names, arities, errors);
            }
        }
        Type::Function { params, ret } => {
            for ty in params {
                check_named_type(ty, names, arities, errors);
            }
            check_named_type(ret, names, arities, errors);
        }
        _ => {}
    }
}
