use std::collections::{HashMap, HashSet};

use terlan_hir::ResolvedModule;
use terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxParamOutput, SyntaxTypeOutput,
};

use super::{
    collect_local_syntax_struct_fields, expand_imported_aliases_except_named,
    normalize_trait_type_text, parse_generic_bounds, parse_interface_signature, parse_type_expr,
    qualify_type_names, FunctionBound, FunctionScheme, QualifiedTypeName, Type, TypeAlias,
    TypeVarId,
};

/// Receiver-method candidate used by expression dispatch.
///
/// Inputs:
/// - Receiver type, callable function scheme, and receiver mutability from
///   local or imported method declarations.
///
/// Output:
/// - Dispatch candidate stored under method name and non-receiver arity.
///
/// Transformation:
/// - Rewrites receiver-first declarations into the call shape needed for
///   `value.method(args...)` type inference.
#[derive(Debug, Clone)]
pub(super) struct ReceiverMethodDispatchSignature {
    pub(super) receiver_type: Type,
    pub(super) scheme: FunctionScheme,
    pub(super) receiver_mutable: bool,
}

/// Declaration-site receiver-method signature.
///
/// Inputs:
/// - Syntax-output receiver method declaration.
///
/// Output:
/// - Normalized parameter, return, mutability, and source-span metadata.
///
/// Transformation:
/// - Preserves declaration shape for trait conformance and inherited receiver
///   method checks before dispatch candidates are generated.
#[derive(Debug, Clone)]
pub(super) struct ReceiverMethodSignature {
    pub(super) params: Vec<String>,
    pub(super) param_mutability: Vec<bool>,
    pub(super) return_type: String,
    pub(super) receiver_mutable: bool,
    pub(super) span: Span,
}

/// Collects receiver methods by receiver type and method name.
///
/// Inputs:
/// - `module`: syntax-output module to scan.
///
/// Output:
/// - Map keyed by `(receiver type text, method name)`.
///
/// Transformation:
/// - Converts receiver-method declarations into normalized signature summaries
///   used by declaration-site conformance validation.
pub(super) fn collect_syntax_receiver_method_signatures(
    module: &SyntaxModuleOutput,
) -> HashMap<(String, String), ReceiverMethodSignature> {
    let mut methods = HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        methods.insert(
            (
                normalize_trait_type_text(&receiver.annotation.text),
                name.clone(),
            ),
            ReceiverMethodSignature {
                params: params
                    .iter()
                    .map(|param| normalize_trait_type_text(&param.annotation.text))
                    .collect(),
                param_mutability: params.iter().map(|param| param.is_mutable).collect(),
                return_type: normalize_trait_type_text(&return_type.text),
                receiver_mutable: receiver.is_mutable,
                span: declaration.span.into(),
            },
        );
    }

    extend_receiver_method_signatures_with_local_struct_derives(module, &mut methods);
    methods
}

/// Collects local and imported-inherited receiver-method dispatch signatures.
///
/// Inputs:
/// - `module`: syntax-output module containing receiver methods and struct
///   derivation clauses.
/// - `resolved`: resolved module context with imported type/interface metadata.
/// - `alias_names`, `imported_type_names`, `imported_type_aliases`, and
///   `local_aliases`: visible type-resolution context.
///
/// Output:
/// - Receiver-method dispatch table including local receiver methods, local
///   inherited receiver methods, and imported parent receiver methods inherited
///   through `struct Child derives ImportedParent`.
///
/// Transformation:
/// - Starts with the local dispatch collector, then reads provider interfaces
///   for explicitly marked receiver methods whose receiver type is an imported
///   derived parent. Inherited imported candidates are rewritten to the local
///   child receiver type.
pub(super) fn collect_syntax_receiver_method_dispatch_signatures_with_imports(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> {
    let mut methods = collect_syntax_receiver_method_dispatch_signatures(
        module,
        alias_names,
        imported_type_names,
        imported_type_aliases,
        local_aliases,
    );
    extend_receiver_method_dispatch_with_imported_receiver_methods(resolved, &mut methods);
    extend_receiver_method_dispatch_with_imported_struct_derives(module, resolved, &mut methods);
    methods
}

/// Adds direct imported receiver-method dispatch signatures.
///
/// Inputs:
/// - `resolved`: resolved module context containing imported provider
///   interfaces.
/// - `methods`: receiver-method dispatch table to extend.
///
/// Output:
/// - None; `methods` is updated in place.
///
/// Transformation:
/// - Scans loaded provider interfaces for public receiver-method signatures,
///   parses each receiver-first interface signature, removes the receiver from
///   callable parameters, and stores a receiver-specialized dispatch candidate.
///   This makes generated wrapper methods such as `value.set_text(...)`
///   callable from modules that import the wrapper type.
fn extend_receiver_method_dispatch_with_imported_receiver_methods(
    resolved: &ResolvedModule,
    methods: &mut HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) {
    let imported = resolved
        .interface_map
        .values()
        .flat_map(|interface| {
            interface
                .function_overloads
                .values()
                .flat_map(move |signatures| {
                    signatures
                        .iter()
                        .filter(|signature| signature.public && signature.receiver_method)
                        .filter_map(move |signature| {
                            let scheme =
                                parse_interface_signature(signature, interface, &HashMap::new())?;
                            let mut params = scheme.params;
                            if params.is_empty() {
                                return None;
                            }
                            let receiver_type = params.remove(0);
                            Some((
                                (signature.name.clone(), params.len()),
                                ReceiverMethodDispatchSignature {
                                    receiver_type,
                                    scheme: FunctionScheme {
                                        params,
                                        ret: scheme.ret,
                                        bounds: scheme.bounds,
                                    },
                                    receiver_mutable: signature.receiver_mutable,
                                },
                            ))
                        })
                })
        })
        .collect::<Vec<_>>();

    for (key, signature) in imported {
        let candidates = methods.entry(key).or_default();
        if candidates.iter().any(|candidate| {
            candidate.receiver_type == signature.receiver_type
                && candidate.receiver_mutable == signature.receiver_mutable
                && candidate.scheme.params == signature.scheme.params
        }) {
            continue;
        }
        candidates.push(signature);
    }
}

/// Adds inherited local receiver-method signatures for derived structs.
///
/// Inputs:
/// - `module`: syntax-output module containing struct and receiver-method
///   declarations.
/// - `methods`: receiver-method signature map keyed by receiver type and
///   method name.
///
/// Output:
/// - None; `methods` is updated in place.
///
/// Transformation:
/// - For each local `struct Child derives Parent`, copies parent receiver
///   method signatures to the child receiver type unless the child already has
///   an explicit method with the same name. This affects type-level
///   conformance validation only; method bodies remain represented by the
///   original parent receiver declaration.
fn extend_receiver_method_signatures_with_local_struct_derives(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, String), ReceiverMethodSignature>,
) {
    let derive_edges = collect_local_syntax_struct_derive_edges(module);
    let inherited = derive_edges
        .iter()
        .flat_map(|(child, parent)| {
            methods
                .iter()
                .filter(move |((receiver, _), _)| receiver == parent)
                .map(move |((_, method_name), signature)| {
                    ((child.clone(), method_name.clone()), signature.clone())
                })
        })
        .collect::<Vec<_>>();

    for (key, signature) in inherited {
        methods.entry(key).or_insert(signature);
    }
}

/// Collects dispatchable receiver methods by method name and arity.
///
/// Inputs:
/// - `module`: syntax-output module containing receiver-method declarations.
/// - `alias_names`: visible type names used while parsing annotations.
/// - `imported_type_names`: imported type aliases that need qualification.
/// - `imported_type_aliases`: imported alias bodies visible to signatures.
/// - `local_aliases`: local alias bodies visible to signatures.
///
/// Output:
/// - Map keyed by `(method name, non-receiver arity)` with one or more
///   receiver-specialized callable schemes.
///
/// Transformation:
/// - Parses the receiver annotation and method parameter/return annotations in
///   one shared type-variable scope, preserving generic receiver relationships.
///   The resulting callable scheme excludes the receiver parameter because
///   receiver call inference checks the receiver separately, then checks the
///   ordinary call arguments through the existing function scheme path.
pub(super) fn collect_syntax_receiver_method_dispatch_signatures(
    module: &SyntaxModuleOutput,
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> {
    let mut methods: HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> =
        HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Method {
            receiver,
            name,
            params,
            return_type,
            generic_bounds,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let Some(signature) = receiver_method_dispatch_signature(
            receiver,
            params,
            return_type,
            generic_bounds,
            alias_names,
            imported_type_names,
            imported_type_aliases,
            local_aliases,
        ) else {
            continue;
        };

        methods
            .entry((name.clone(), params.len()))
            .or_default()
            .push(signature);
    }

    extend_receiver_method_dispatch_with_local_struct_derives(module, &mut methods);
    methods
}

/// Adds imported parent receiver methods for local derived structs.
///
/// Inputs:
/// - `module`: syntax-output module containing struct derivation clauses.
/// - `resolved`: resolved imports and provider interfaces.
/// - `methods`: receiver-method dispatch table to extend.
///
/// Output:
/// - None; `methods` is updated in place.
///
/// Transformation:
/// - For each `struct Child derives Parent` where `Parent` is imported, scans
///   the provider interface for public receiver methods whose first parameter
///   is the provider's parent type. Each match is rewritten as a dispatch
///   candidate for `Child` with the receiver argument removed from the callable
///   scheme, matching local receiver-method dispatch.
fn extend_receiver_method_dispatch_with_imported_struct_derives(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    methods: &mut HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) {
    let inherited = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, derives, .. } => Some((name, derives)),
            _ => None,
        })
        .flat_map(|(child, derives)| {
            derives.iter().filter_map(move |parent| {
                let imported = resolved.imported_types.get(parent)?;
                let interface = resolved.interface_map.get(&imported.source_module)?;
                Some((child.clone(), imported.source_name.clone(), interface))
            })
        })
        .flat_map(|(child, parent_source_name, interface)| {
            interface
                .functions
                .values()
                .filter(move |signature| {
                    signature.public
                        && signature.receiver_method
                        && signature
                            .params
                            .first()
                            .is_some_and(|param| param.annotation == parent_source_name)
                })
                .filter_map(move |signature| {
                    let scheme = parse_interface_signature(signature, interface, &HashMap::new())?;
                    let mut params = scheme.params;
                    if params.is_empty() {
                        return None;
                    }
                    params.remove(0);
                    Some((
                        (signature.name.clone(), params.len()),
                        ReceiverMethodDispatchSignature {
                            receiver_type: Type::Named {
                                module: None,
                                name: child.clone(),
                                args: Vec::new(),
                            },
                            scheme: FunctionScheme {
                                params,
                                ret: scheme.ret,
                                bounds: scheme.bounds,
                            },
                            receiver_mutable: signature.receiver_mutable,
                        },
                    ))
                })
        })
        .collect::<Vec<_>>();

    for (key, signature) in inherited {
        let candidates = methods.entry(key).or_default();
        if candidates
            .iter()
            .any(|candidate| candidate.receiver_type == signature.receiver_type)
        {
            continue;
        }
        candidates.push(signature);
    }
}

/// Adds inherited local receiver-method dispatch signatures for derived structs.
///
/// Inputs:
/// - `module`: syntax-output module containing struct derivation edges.
/// - `methods`: dispatch table keyed by method name and non-receiver arity.
///
/// Output:
/// - None; `methods` is updated in place.
///
/// Transformation:
/// - Copies dispatch candidates whose receiver type is a local parent struct to
///   each derived child struct. Explicit child receiver methods win over
///   inherited candidates for the same method/arity/receiver type.
fn extend_receiver_method_dispatch_with_local_struct_derives(
    module: &SyntaxModuleOutput,
    methods: &mut HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) {
    let derive_edges = collect_local_syntax_struct_derive_edges(module);
    let inherited = derive_edges
        .iter()
        .flat_map(|(child, parent)| {
            methods.iter().flat_map(move |(key, candidates)| {
                candidates
                    .iter()
                    .filter(move |candidate| {
                        receiver_type_matches_local_struct(parent, &candidate.receiver_type)
                    })
                    .map(move |candidate| {
                        let mut inherited = candidate.clone();
                        inherited.receiver_type = Type::Named {
                            module: None,
                            name: child.clone(),
                            args: Vec::new(),
                        };
                        (key.clone(), inherited)
                    })
            })
        })
        .collect::<Vec<_>>();

    for (key, signature) in inherited {
        let candidates = methods.entry(key).or_default();
        if candidates
            .iter()
            .any(|candidate| candidate.receiver_type == signature.receiver_type)
        {
            continue;
        }
        candidates.push(signature);
    }
}

/// Returns local child-parent struct derive edges.
///
/// Inputs:
/// - `module`: syntax-output module containing struct declarations.
///
/// Output:
/// - Ordered `(child, parent)` pairs for derives whose parent is also a local
///   struct.
///
/// Transformation:
/// - Filters `derives` clauses to local struct parents so receiver-method
///   inheritance does not guess at imported receiver-method metadata.
fn collect_local_syntax_struct_derive_edges(module: &SyntaxModuleOutput) -> Vec<(String, String)> {
    let local_structs = collect_local_syntax_struct_fields(module);
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, derives, .. } => Some((name, derives)),
            _ => None,
        })
        .flat_map(|(child, derives)| {
            let local_structs = &local_structs;
            derives
                .iter()
                .filter(move |parent| local_structs.contains_key(*parent))
                .map(move |parent| (child.clone(), parent.clone()))
        })
        .collect()
}

/// Checks whether a parsed receiver type is a local struct with no type args.
///
/// Inputs:
/// - `name`: expected local struct name.
/// - `ty`: parsed receiver type.
///
/// Output:
/// - `true` when `ty` is exactly the unqualified local struct name.
///
/// Transformation:
/// - Rejects generic and qualified receiver types for the first derivation
///   slice, keeping inherited receiver-method dispatch conservative.
fn receiver_type_matches_local_struct(name: &str, ty: &Type) -> bool {
    matches!(
        ty,
        Type::Named {
            module: None,
            name: receiver_name,
            args
        } if receiver_name == name && args.is_empty()
    )
}

/// Builds one dispatch signature for a receiver-method declaration.
///
/// Inputs:
/// - `receiver`: declared receiver parameter.
/// - `params`: non-receiver method parameters.
/// - `return_type`: declared method return type.
/// - `generic_bounds`: callable generic bounds from the method declaration.
/// - `alias_names`, `imported_type_names`, `imported_type_aliases`, and
///   `local_aliases`: visible type-resolution context.
///
/// Output:
/// - Dispatch signature with parsed receiver type and non-receiver function
///   scheme, or `None` when the receiver annotation cannot be parsed.
///
/// Transformation:
/// - Parses all type annotations in one variable scope, expands imported aliases
///   without erasing named imported identities, qualifies imported type names,
///   converts generic bounds into the same internal form used for normal
///   functions, and preserves the receiver mutability marker for later
///   compiler-owned rebinding analysis.
fn receiver_method_dispatch_signature(
    receiver: &SyntaxParamOutput,
    params: &[SyntaxParamOutput],
    return_type: &SyntaxTypeOutput,
    generic_bounds: &[String],
    alias_names: &HashSet<String>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    imported_type_aliases: &HashMap<String, TypeAlias>,
    local_aliases: &HashMap<String, TypeAlias>,
) -> Option<ReceiverMethodDispatchSignature> {
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;

    let receiver_type = parse_type_expr(
        &receiver.annotation.text,
        alias_names,
        &mut vars,
        &mut next_var,
    )?;
    let receiver_type = expand_imported_aliases_except_named(
        &receiver_type,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let receiver_type = qualify_type_names(&receiver_type, imported_type_names);

    let params = params
        .iter()
        .map(|param| {
            let parsed = parse_type_expr(
                &param.annotation.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);
            let parsed = expand_imported_aliases_except_named(
                &parsed,
                imported_type_aliases,
                imported_type_names,
                local_aliases,
            );
            qualify_type_names(&parsed, imported_type_names)
        })
        .collect::<Vec<_>>();

    let ret = parse_type_expr(&return_type.text, alias_names, &mut vars, &mut next_var)
        .unwrap_or(Type::Dynamic);
    let ret = expand_imported_aliases_except_named(
        &ret,
        imported_type_aliases,
        imported_type_names,
        local_aliases,
    );
    let ret = qualify_type_names(&ret, imported_type_names);

    let bounds = parse_generic_bounds(generic_bounds, &vars, alias_names)
        .into_iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name,
            trait_args: bound
                .trait_args
                .into_iter()
                .map(|arg| {
                    let arg = expand_imported_aliases_except_named(
                        &arg,
                        imported_type_aliases,
                        imported_type_names,
                        local_aliases,
                    );
                    qualify_type_names(&arg, imported_type_names)
                })
                .collect(),
        })
        .collect();

    Some(ReceiverMethodDispatchSignature {
        receiver_type,
        scheme: FunctionScheme {
            params,
            ret,
            bounds,
        },
        receiver_mutable: receiver.is_mutable,
    })
}
