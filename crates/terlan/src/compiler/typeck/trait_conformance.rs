use super::*;

mod impls;
mod syntax;

use impls::check_parsed_trait_impl_signature;
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

/// Collects imported struct fields visible under local import names.
///
/// Inputs:
/// - `resolved`: resolved module context with imported type items and provider
///   interfaces.
///
/// Output:
/// - Map from local imported type name to ordered syntax-output-like field
///   metadata.
///
/// Transformation:
/// - Reads public struct field signatures from the provider interface and
///   converts them back into syntax-output field metadata with empty spans and
///   no defaults. This lets derive expansion treat local and imported parent
///   structs uniformly without loading provider source files.
pub(super) fn collect_imported_syntax_struct_fields(
    resolved: &ResolvedModule,
) -> HashMap<String, Vec<SyntaxStructFieldOutput>> {
    let mut structs = HashMap::new();

    for (local_name, imported) in &resolved.imported_types {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };
        let Some(fields) = interface.struct_fields.get(&imported.source_name) else {
            continue;
        };

        structs.insert(
            local_name.clone(),
            fields
                .iter()
                .map(|field| SyntaxStructFieldOutput {
                    name: field.name.clone(),
                    annotation: SyntaxTypeOutput {
                        text: field.annotation.clone(),
                        span: Default::default(),
                    },
                    is_private: field.is_private,
                    docs: Vec::new(),
                    has_default: false,
                    default: None,
                    span: Default::default(),
                })
                .collect(),
        );
    }

    structs
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
/// - Keeps struct inclusion validation separate from trait lookup. Imported
///   names are accepted only when their provider interface exposes structured
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
    let imported_structs = collect_imported_syntax_struct_fields(resolved);

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
                    type_name,
                    &implemented_trait,
                    signature,
                    &method_name,
                    &expected_method,
                    receiver_methods.get(&(type_name.to_string(), method_name.clone())),
                    trait_ref.span.into(),
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
pub(super) fn check_syntax_trait_impl_coherence(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen: HashMap<String, Span> = HashMap::new();

    for declaration in &module.declarations {
        if let Some((type_name, implements)) = syntax_declared_implements(declaration) {
            for trait_ref in implements {
                let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
                    continue;
                };
                let Some(key) = syntax_trait_impl_key(&target, type_name) else {
                    continue;
                };
                if let Some(previous) = seen.get(&key) {
                    diagnostics.push(Diagnostic {
                        span: trait_ref.span.into(),
                        message: format!(
                            "coherent impl conflict for `{}`: duplicate visible conformance (first seen at {:?})",
                            key, previous
                        ),
                        severity: DiagSeverity::Error,
                    });
                } else {
                    seen.insert(key, trait_ref.span.into());
                }
            }
            continue;
        }

        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            ..
        } = &declaration.payload
        else {
            continue;
        };

        let Some(target) = parse_trait_instance_from_text(&trait_ref.text) else {
            continue;
        };
        let Some(key) = syntax_trait_impl_key(&target, &for_type.text) else {
            continue;
        };
        if let Some(previous) = seen.get(&key) {
            diagnostics.push(Diagnostic {
                span: declaration.span.into(),
                message: format!(
                    "coherent impl conflict for `{}`: duplicate visible conformance (first seen at {:?})",
                    key, previous
                ),
                severity: DiagSeverity::Error,
            });
        } else {
            seen.insert(key, declaration.span.into());
        }
    }

    diagnostics
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
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inheritance_cache: HashMap<String, Option<HashMap<String, TraitMethodSignature>>> =
        HashMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl { .. } = &declaration.payload else {
            continue;
        };

        let Some(impl_decl) = syntax_trait_impl_to_parsed(declaration) else {
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
            trait_map,
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

/// Converts structured syntax-output impl declarations into checker summaries.
///
/// Inputs:
/// - `declaration`: syntax-output declaration expected to hold a
///   `TraitImpl` payload.
///
/// Output:
/// - Parsed trait impl summary with target trait, owner type, and method
///   signatures, or `None` when the payload is not a trait impl or its trait
///   reference cannot be parsed.
///
/// Transformation:
/// - Reads the structured `trait_ref`, `for_type`, and impl methods directly
///   from syntax output, avoiding raw source reparsing for the formal compiler
///   path.
pub(super) fn syntax_trait_impl_to_parsed(
    declaration: &SyntaxDeclarationOutput,
) -> Option<ParsedTraitImpl> {
    let SyntaxDeclarationPayload::TraitImpl {
        trait_ref,
        for_type,
        methods,
        ..
    } = &declaration.payload
    else {
        return None;
    };

    let target = parse_trait_instance_from_text(&trait_ref.text)?;
    Some(ParsedTraitImpl {
        target,
        for_type: Some(normalize_trait_type_text(&for_type.text)),
        methods: methods.iter().map(syntax_impl_method_signature).collect(),
    })
}

/// Converts one structured impl method into a comparable signature.
///
/// Inputs:
/// - `method`: syntax-output impl method payload.
///
/// Output:
/// - Parsed method signature containing name, parameter type texts, return
///   type text, and source span.
///
/// Transformation:
/// - Drops method bodies and keeps only the type-level information needed for
///   conformance validation.
fn syntax_impl_method_signature(method: &SyntaxImplMethodOutput) -> ParsedMethodSignature {
    ParsedMethodSignature {
        name: method.name.clone(),
        params: method
            .params
            .iter()
            .map(|param| normalize_trait_type_text(&param.annotation.text))
            .collect(),
        mutable_params: method.params.iter().map(|param| param.is_mutable).collect(),
        return_type: normalize_trait_type_text(&method.return_type.text),
        span: method.span.into(),
    }
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

        if !local_type_names.contains(&receiver_head)
            && !(module.module_name == "std.core.String" && receiver_head == "String")
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

/// Extracts the unqualified owner type head from receiver annotation text.
///
/// Inputs:
/// - `text`: normalized receiver annotation text.
///
/// Output:
/// - The unqualified receiver type head for simple named receiver types.
/// - `None` for qualified/imported, tuple, list, map, function, or malformed
///   receiver annotations.
///
/// Transformation:
/// - Reads identifier characters up to a type-argument delimiter and rejects
///   annotations whose owner cannot be represented as a single local type name.
fn receiver_owner_type_name_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('.') {
        return None;
    }
    let head = trimmed
        .split(['[', ' ', '\t', '\r', '\n'])
        .next()
        .unwrap_or_default();
    if head
        .chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
        && head
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(head.to_string())
    } else {
        None
    }
}

/// Checks one required method for an `implements` conformance.
///
/// Inputs:
/// - `type_name`: type declaring the `implements` clause.
/// - `implemented_trait`: parsed trait reference from the conformance clause.
/// - `trait_signature`: declared trait type parameters.
/// - `method_name`: required trait method name.
/// - `expected_method`: trait method signature before substitution.
/// - `receiver_method`: matching receiver method, if one exists.
/// - `fallback_span`: span for diagnostics when no method-specific span exists.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Substitutes trait type parameters with conformance type arguments and then
///   compares the resulting method signature with the receiver-method shape:
///   the first trait method parameter maps to the receiver, and remaining
///   parameters map to ordinary method arguments.
fn check_declared_implements_method(
    type_name: &str,
    implemented_trait: &ParsedTraitInstance,
    trait_signature: &ParsedTraitSignature,
    method_name: &str,
    expected_method: &TraitMethodSignature,
    receiver_method: Option<&ReceiverMethodSignature>,
    fallback_span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let specialized_params = expected_method
        .params
        .iter()
        .map(|param| {
            specialize_trait_type_text(
                &param.ty,
                &trait_signature.type_params,
                &implemented_trait.type_args,
            )
        })
        .collect::<Vec<_>>();
    let specialized_return = specialize_trait_type_text(
        &expected_method.return_type,
        &trait_signature.type_params,
        &implemented_trait.type_args,
    );

    if specialized_params.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` must declare a receiver parameter for `implements`",
                method_name, implemented_trait.name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let expected_receiver = &specialized_params[0];
    if !trait_type_text_equal(expected_receiver, type_name) {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` expects receiver {}, but `{}` implements it",
                method_name, implemented_trait.name, expected_receiver, type_name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let Some(receiver_method) = receiver_method else {
        if !expected_method.has_default {
            diagnostics.push(Diagnostic {
                span: fallback_span,
                message: format!(
                    "missing receiver method `{}` for `{}` implementing `{}`",
                    method_name, type_name, implemented_trait.name
                ),
                severity: DiagSeverity::Error,
            });
        }
        return;
    };

    if expected_method
        .params
        .first()
        .is_some_and(|param| param.is_mutable)
        && !receiver_method.receiver_mutable
    {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` for `{}` must use a mutable receiver",
                method_name, type_name
            ),
            severity: DiagSeverity::Error,
        });
    }

    let expected_args = &specialized_params[1..];
    if receiver_method.params.len() != expected_args.len() {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` for `{}` has arity {}, expected {}",
                method_name,
                type_name,
                receiver_method.params.len(),
                expected_args.len()
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for (idx, (expected, found)) in expected_args
        .iter()
        .zip(receiver_method.params.iter())
        .enumerate()
    {
        if expected_method
            .params
            .get(idx + 1)
            .is_some_and(|param| param.is_mutable)
            && !receiver_method
                .param_mutability
                .get(idx)
                .copied()
                .unwrap_or(false)
        {
            diagnostics.push(Diagnostic {
                span: receiver_method.span,
                message: format!(
                    "receiver method `{}` parameter {} for `{}` must be mutable",
                    method_name,
                    idx + 1,
                    type_name
                ),
                severity: DiagSeverity::Error,
            });
        }
        if !trait_type_text_equal(expected, found) {
            diagnostics.push(Diagnostic {
                span: receiver_method.span,
                message: format!(
                    "receiver method `{}` parameter {} for `{}` expects {}, found {}",
                    method_name,
                    idx + 1,
                    type_name,
                    expected,
                    found
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    if !trait_type_text_equal(&specialized_return, &receiver_method.return_type) {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` return type for `{}` expects {}, found {}",
                method_name, type_name, specialized_return, receiver_method.return_type
            ),
            severity: DiagSeverity::Error,
        });
    }
}
