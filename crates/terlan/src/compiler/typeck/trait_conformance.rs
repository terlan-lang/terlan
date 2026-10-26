#[path = "trait_conformance/receiver_methods.rs"]
mod receiver_methods;
use receiver_methods::{
    check_declared_implements_method, receiver_owner_type_name_from_text, DeclaredMethodConformance,
};

use super::*;

mod impls;
mod negative_impls;
mod syntax;

use impls::{check_parsed_trait_impl_signature, TraitImplSignatureContext};
pub(super) use negative_impls::{
    check_syntax_negative_trait_impls, collect_visible_negative_trait_impl_type_args,
    syntax_trait_impl_to_parsed,
};
pub(super) use syntax::{
    check_syntax_macro_decl_signatures, check_syntax_public_constructor_return_visibility,
    collect_syntax_kind_diagnostics,
};

/// Validates trait declaration identity and inheritance references.
///
/// Inputs:
/// - `module`: syntax-output module containing trait declarations.
/// - `trait_map`: known local/imported trait signatures keyed by local trait
///   name.
///
/// Output:
/// - Diagnostics for duplicate trait names, duplicate method names, unknown
///   super traits, malformed super trait references, and super-trait arity
///   mismatches.
///
/// Transformation:
/// - Checks declaration-local uniqueness, then resolves each declared
///   super-trait reference against the trait signature map without generating
///   any conformance facts.
pub(super) fn check_syntax_trait_decls(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut trait_names = HashSet::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Trait { name, methods, .. } = &declaration.payload else {
            continue;
        };

        if !trait_names.insert(name.clone()) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!("duplicate trait declaration `{}`", name),
                severity: DiagSeverity::Error,
            });
        }

        let mut method_names = HashSet::new();
        for method in methods {
            if !method_names.insert(method.name.clone()) {
                diagnostics.push(Diagnostic {
                    span: method.span.into(),
                    message: format!("duplicate method `{}` in trait {}", method.name, name),
                    severity: DiagSeverity::Error,
                });
            }
        }

        let Some(trait_signature) = trait_map.get(name) else {
            continue;
        };

        for super_trait_text in &trait_signature.super_traits {
            let Some(super_trait) = parse_trait_instance_from_text(super_trait_text) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unable to parse super trait `{}` in declaration of `{}`",
                        super_trait_text, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            let Some(super_signature) = trait_map.get(&super_trait.name) else {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unknown super trait `{}` in declaration of `{}`",
                        super_trait.name, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            if super_trait.type_args.len() != super_signature.type_params.len() {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "super trait `{}` expects {} type parameter(s), found {}",
                        super_trait.name,
                        super_signature.type_params.len(),
                        super_trait.type_args.len()
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }
    diagnostics
}

/// Collects local struct fields by struct name.
///
/// Inputs:
/// - `module`: syntax-output module containing source declarations.
///
/// Output:
/// - Map from local struct name to its declared field list.
///
/// Transformation:
/// - Scans only local `struct` declarations and clones their fields so derive
///   expansion can borrow the module mutably without aliasing source fields.
pub(super) fn collect_local_syntax_struct_fields(
    module: &SyntaxModuleOutput,
) -> HashMap<String, Vec<SyntaxStructFieldOutput>> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } => {
                Some((name.clone(), fields.clone()))
            }
            _ => None,
        })
        .collect()
}

/// Returns whether an included parent name is visible as a local or imported type.
///
/// Inputs:
/// - `parent_name`: source text inside a struct `includes` clause.
/// - `local_structs`: local struct declarations keyed by name.
/// - `imported_structs`: imported struct declarations keyed by local import
///   name.
/// - `resolved`: resolved module context containing imported type names.
///
/// Output:
/// - `true` when the parent is a local struct or imported public struct name.
///
/// Transformation:
/// - Accepts imported parent names only when the provider exposes structured
///   public field metadata.
fn is_visible_struct_include_parent(
    parent_name: &str,
    local_structs: &HashMap<String, Vec<SyntaxStructFieldOutput>>,
    imported_structs: &HashMap<String, Vec<SyntaxStructFieldOutput>>,
    resolved: &ResolvedModule,
) -> bool {
    local_structs.contains_key(parent_name)
        || (resolved.imported_types.contains_key(parent_name)
            && imported_structs.contains_key(parent_name))
}

/// Validates source-level struct inclusion clauses.
///
/// Inputs:
/// - `module`: syntax-output module containing struct declarations.
/// - `resolved`: resolved module context containing imported type names.
///
/// Output:
/// - Diagnostics for duplicate, self, or unknown included parent structs.
///
/// Transformation:
/// - Treats `includes` as struct-to-struct shape inclusion only. It does not
///   parse trait instances and does not produce trait conformance facts.
pub(super) fn check_syntax_struct_includes(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let local_structs = collect_local_syntax_struct_fields(module);
    let imported_structs = super::struct_includes::imported_struct_fields(resolved);

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { name, includes, .. } = &declaration.payload else {
            continue;
        };

        let mut seen = HashSet::new();
        for parent_name in includes {
            if parent_name.contains('[') {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "included struct `{}` in declaration of struct `{}` must be a struct name, not a trait or generic instance",
                        parent_name, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            if parent_name == name {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!("struct `{}` cannot include itself", name),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            if !seen.insert(parent_name.clone()) {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "duplicate included struct `{}` in declaration of struct `{}`",
                        parent_name, name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            if !is_visible_struct_include_parent(
                parent_name,
                &local_structs,
                &imported_structs,
                resolved,
            ) {
                diagnostics.push(Diagnostic {
                    span: declaration.span.into(),
                    message: format!(
                        "unknown included struct `{}` in declaration of struct `{}`",
                        parent_name, name
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }

    diagnostics
}

/// Validates declaration-site `implements` conformance obligations.
///
/// Inputs:
/// - `module`: compiler-facing syntax output containing type, struct, trait,
///   and receiver-method declarations.
/// - `trait_map`: known local/imported trait signatures keyed by local trait
///   name.
///
/// Output:
/// - Diagnostics for malformed, unknown, duplicate, arity-mismatched, or
///   unsatisfied `implements` entries.
///
/// Transformation:
/// - Treats each `implements TraitRef` entry as a conformance obligation for
///   the declaring type, substitutes trait type parameters with the provided
///   type arguments, and checks required trait methods against receiver methods
///   declared on that type. Trait methods with default bodies are considered
///   satisfied when no receiver method is present.
pub(super) fn check_syntax_declared_implements(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let receiver_methods = collect_syntax_receiver_method_signatures(module);
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    for declaration in &module.declarations {
        let Some((type_name, implements)) = syntax_declared_implements(declaration) else {
            continue;
        };

        let mut seen = HashSet::new();
        for trait_ref in implements {
            let Some(implemented_trait) = parse_trait_instance_from_text(&trait_ref.text) else {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "unable to parse implemented trait `{}` in declaration of `{}`",
                        trait_ref.text, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            let implement_key =
                trait_instance_key(&implemented_trait).unwrap_or_else(|| trait_ref.text.clone());
            if !seen.insert(implement_key.clone()) {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "duplicate implemented trait `{}` in declaration of `{}`",
                        implement_key, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            let Some(signature) = trait_map.get(&implemented_trait.name) else {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "unknown implemented trait `{}` in declaration of `{}`",
                        implemented_trait.name, type_name
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            };

            if implemented_trait.type_args.len() != signature.type_params.len() {
                diagnostics.push(Diagnostic {
                    span: trait_ref.span.into(),
                    message: format!(
                        "implemented trait `{}` expects {} type parameter(s), found {}",
                        implemented_trait.name,
                        signature.type_params.len(),
                        implemented_trait.type_args.len()
                    ),
                    severity: DiagSeverity::Error,
                });
                continue;
            }

            let methods = collect_trait_methods_with_inheritance(
                trait_map,
                &implemented_trait.name,
                &mut inheritance_cache,
                &mut HashSet::new(),
            )
            .unwrap_or_default();

            for (method_name, expected_method) in methods {
                check_declared_implements_method(
                    DeclaredMethodConformance {
                        type_name,
                        implemented_trait: &implemented_trait,
                        trait_signature: signature,
                        method_name: &method_name,
                        expected_method: &expected_method,
                        receiver_method: receiver_methods
                            .get(&(type_name.to_string(), method_name.clone())),
                        fallback_span: trait_ref.span.into(),
                    },
                    &mut diagnostics,
                );
            }
        }
    }

    diagnostics
}

/// Validates coherence for structured source-level trait conformance.
///
/// Inputs:
/// - `module`: syntax-output module containing declaration-site `implements`
///   and explicit `impl Trait for Type` declarations.
///
/// Output:
/// - Diagnostics for duplicate conformance keys across declaration-site and
///   explicit adapter forms.
///
/// Transformation:
/// - Converts both conformance syntaxes into stable `TraitRef for Type` keys
///   and reports repeated keys. This enforces the greenfield rule that a type
///   must not declare `implements Trait[...]` and also provide an explicit
///   adapter impl for the same pair.
pub(super) fn check_syntax_trait_impl_coherence(
    module: &SyntaxModuleOutput,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Span> = HashMap::new();
    let imported_type_refs = imported_type_text_refs(imported_type_names);

    for (key, span) in collect_syntax_positive_trait_impl_keys(module, &imported_type_refs) {
        if let Some(previous) = seen.get(&key) {
            diagnostics.push(Diagnostic {
                span,
                message: format!(
                    "coherent impl conflict for `{}`: duplicate visible conformance (first seen at {:?})",
                    key, previous
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            seen.insert(key, span);
        }
    }

    diagnostics
}

/// Collects canonical keys for every positive source-level conformance.
fn collect_syntax_positive_trait_impl_keys(
    module: &SyntaxModuleOutput,
    imported_type_refs: &HashMap<String, String>,
) -> Vec<(String, Span)> {
    let mut keys = Vec::new();
    for declaration in &module.declarations {
        if let Some((type_name, implements)) = syntax_declared_implements(declaration) {
            for trait_ref in implements {
                let trait_ref_text = crate::terlan_hir::qualify_syntax_type_text(
                    &trait_ref.text,
                    imported_type_refs,
                );
                let for_type_text =
                    crate::terlan_hir::qualify_syntax_type_text(type_name, imported_type_refs);
                let Some(target) = parse_trait_instance_from_text(&trait_ref_text) else {
                    continue;
                };
                let Some(key) = syntax_trait_impl_key(&target, &for_type_text) else {
                    continue;
                };
                keys.push((key, trait_ref.span.into()));
            }
            continue;
        }

        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_negative: false,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let trait_ref =
            crate::terlan_hir::qualify_syntax_type_text(&trait_ref.text, imported_type_refs);
        let for_type =
            crate::terlan_hir::qualify_syntax_type_text(&for_type.text, imported_type_refs);
        let Some(target) = parse_trait_instance_from_text(&trait_ref) else {
            continue;
        };
        let Some(key) = syntax_trait_impl_key(&target, &for_type) else {
            continue;
        };
        keys.push((key, declaration.span.into()));
    }
    keys
}

/// Validates structured explicit `impl Trait for Type` method signatures.
///
/// Inputs:
/// - `module`: syntax-output module to scan for explicit trait impl blocks.
/// - `trait_map`: known local/imported trait signatures keyed by local trait
///   name.
///
/// Output:
/// - Diagnostics for unknown traits, trait arity mismatches, duplicate impl
///   methods, undeclared impl methods, missing required methods, and parameter
///   or return-type mismatches.
///
/// Transformation:
/// - Converts each structured impl payload into a parsed conformance summary,
///   specializes trait type parameters with the impl's type arguments, and
///   compares the adapter methods against inherited trait requirements.
pub(super) fn check_syntax_trait_impl_signatures(
    module: &SyntaxModuleOutput,
    trait_map: &HashMap<String, ParsedTraitSignature>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
    alias_names: &HashSet<String>,
    aliases: &HashMap<String, TypeAlias>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();
    let imported_type_refs = imported_type_text_refs(imported_type_names);

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            is_negative: false, ..
        } = &declaration.payload
        else {
            continue;
        };

        let Some(impl_decl) = syntax_trait_impl_to_parsed(declaration, &imported_type_refs) else {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: "unable to parse trait impl declaration".to_string(),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        check_parsed_trait_impl_signature(
            &impl_decl,
            declaration.span.into(),
            TraitImplSignatureContext {
                trait_map,
                alias_names,
                aliases,
                require_implementation: module.source_kind
                    != crate::terlan_syntax::SyntaxSourceKind::Interface,
            },
            &mut inheritance_cache,
            &mut diagnostics,
        );
    }

    diagnostics
}

/// Builds a stable conformance key for syntax-output trait impl checks.
///
/// Inputs:
/// - `target`: parsed trait instance such as `Show[User]`.
/// - `for_type`: source type that owns the explicit or declaration-site
///   conformance.
///
/// Output:
/// - `Some("Trait[Args] for Type")` when the trait name is non-empty.
/// - `None` for malformed trait instances.
///
/// Transformation:
/// - Reuses the existing trait-instance key and appends normalized owner type
///   text for coherence checks.
fn syntax_trait_impl_key(target: &ParsedTraitInstance, for_type: &str) -> Option<String> {
    trait_instance_key(target)
        .map(|trait_key| format!("{} for {}", trait_key, normalize_trait_type_text(for_type)))
}

/// Converts resolver-owned imported type identities into text qualification targets.
pub(super) fn imported_type_text_refs(
    imported_type_names: &HashMap<String, QualifiedTypeName>,
) -> HashMap<String, String> {
    let primitive_names = primitive_type_names();
    imported_type_names
        .iter()
        .filter(|(local_name, _)| !primitive_names.contains(*local_name))
        .map(|(local_name, qualified)| {
            (
                local_name.clone(),
                qualified.module.as_deref().map_or_else(
                    || qualified.name.clone(),
                    |module| crate::terlan_hir::public_type_path(module, &qualified.name),
                ),
            )
        })
        .collect()
}

/// Returns a declaration's type name and `implements` list when present.
///
/// Inputs:
/// - `declaration`: syntax-output declaration to inspect.
///
/// Output:
/// - `Some((type_name, implements))` for type/struct declarations with one or
///   more `implements` entries.
/// - `None` for declarations without declaration-site conformance obligations.
///
/// Transformation:
/// - Abstracts over type aliases and structs so conformance validation can use
///   one path for both declaration forms.
pub(super) fn syntax_declared_implements(
    declaration: &SyntaxDeclarationOutput,
) -> Option<(&str, &[SyntaxTypeOutput])> {
    match &declaration.payload {
        SyntaxDeclarationPayload::Type {
            name, implements, ..
        }
        | SyntaxDeclarationPayload::Struct {
            name, implements, ..
        } if !implements.is_empty() => Some((name.as_str(), implements.as_slice())),
        _ => None,
    }
}

/// Validates local receiver-method declaration identity and ownership.
///
/// Inputs:
/// - `module`: syntax-output module to inspect.
/// - `local_type_names`: type and struct names declared in the same module.
///
/// Output:
/// - Diagnostics for unsupported mutable receiver return declarations,
///   duplicate receiver-method identities, and receiver methods declared
///   outside the receiver type's owner module.
///
/// Transformation:
/// - Checks the source-level receiver annotation head without expanding aliases.
///   Mutable receiver methods may expose `Unit` for command-style rebinding or
///   the receiver type for fluent rebinding; other result types need the later
///   paired-result ABI. A method identity is `(receiver type text, method name,
///   non-receiver arity)`. Local declarations own local type/struct receiver
///   heads; the `std.core.String` module is the primitive declaration site for
///   the compiler-known `String` receiver surface.
pub(super) fn check_syntax_receiver_methods(
    module: &SyntaxModuleOutput,
    local_type_names: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<(String, String, usize), Span> = HashMap::new();

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

        let receiver_text = normalize_trait_type_text(&receiver.annotation.text);
        let return_text = normalize_trait_type_text(&return_type.text);
        if receiver.is_mutable
            && !is_unit_type_text(&return_type.text)
            && return_text != receiver_text
        {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "mutable receiver method `{}` for `{}` may return Unit or `{}`; result type `{}` needs the paired mutable receiver result ABI",
                    name,
                    receiver.annotation.text,
                    receiver_text,
                    return_type.text
                ),
                severity: DiagSeverity::Error,
            });
        }

        let key = (receiver_text.clone(), name.clone(), params.len());
        if let Some(previous) = seen.get(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "duplicate receiver method `{}` for `{}` / {} (first seen at {:?})",
                    name,
                    receiver_text,
                    params.len(),
                    previous
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            seen.insert(key, declaration.span.into());
        }

        let Some(receiver_head) = receiver_owner_type_name_from_text(&receiver_text) else {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "receiver method `{}` must use an owned named receiver type, found `{}`",
                    name, receiver_text
                ),
                severity: DiagSeverity::Error,
            });
            continue;
        };

        if !(local_type_names.contains(&receiver_head)
            || module.module_name == "std.core.String" && receiver_head == "String")
        {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "receiver method `{}` for `{}` must be declared in the defining module of `{}`",
                    name, receiver_text, receiver_head
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}
