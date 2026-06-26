use super::*;
use terlan_syntax::TokenKind;

/// Collects syntax-level kind diagnostics across type annotations.
///
/// Inputs:
/// - `module`: syntax-output module containing declarations with type
///   annotations.
/// - `trait_signatures`: visible trait signatures keyed by source-visible
///   trait name.
/// - `aliases`: visible type aliases keyed by source-visible type name.
///
/// Output:
/// - Diagnostics for kind-level mismatches detected directly from annotation
///   text.
///
/// Transformation:
/// - Walks declaration annotations with declaration-local type parameter kind
///   metadata and checks visible trait applications against their declared
///   type-parameter kind arity.
pub(crate) fn collect_syntax_kind_diagnostics(
    module: &SyntaxModuleOutput,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    aliases: &HashMap<String, TypeAlias>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let type_constructor_arities = type_constructor_arities(module, aliases);
    let type_constructor_variances = type_constructor_variances(module, aliases);
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                params,
                return_type,
                generic_bounds,
                ..
            } => {
                let local_kinds = type_param_kind_arities(generic_bounds);
                let local_variances = type_param_kind_variances(generic_bounds);
                for param in params {
                    collect_kind_diagnostic_for_syntax_type(
                        &param.annotation,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
                collect_kind_diagnostic_for_syntax_type(
                    return_type,
                    trait_signatures,
                    &type_constructor_arities,
                    &type_constructor_variances,
                    &local_kinds,
                    &local_variances,
                    &mut diagnostics,
                );
            }
            SyntaxDeclarationPayload::Type { variants, .. } => {
                let local_kinds = HashMap::new();
                let local_variances = HashMap::new();
                for variant in variants {
                    collect_kind_diagnostic_for_syntax_type(
                        variant,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                let local_kinds = HashMap::new();
                let local_variances = HashMap::new();
                for field in fields {
                    collect_kind_diagnostic_for_syntax_type(
                        &field.annotation,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    let local_kinds = type_param_kind_arities(&[]);
                    let local_variances = type_param_kind_variances(&[]);
                    for param in &clause.params {
                        collect_kind_diagnostic_for_syntax_type(
                            &param.annotation,
                            trait_signatures,
                            &type_constructor_arities,
                            &type_constructor_variances,
                            &local_kinds,
                            &local_variances,
                            &mut diagnostics,
                        );
                    }
                    collect_kind_diagnostic_for_syntax_type(
                        &clause.return_type,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Trait {
                params: trait_params,
                methods,
                super_traits,
                ..
            } => {
                let local_kinds = type_param_kind_arities(trait_params);
                let local_variances = type_param_kind_variances(trait_params);
                for super_trait in super_traits {
                    collect_kind_diagnostics_for_trait_text(
                        super_trait,
                        declaration.span.into(),
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
                for method in methods {
                    let method_kinds = local_kinds
                        .iter()
                        .map(|(name, arity)| (name.clone(), *arity))
                        .chain(type_param_kind_arities(&method.generic_bounds))
                        .collect::<HashMap<_, _>>();
                    let method_variances = local_variances
                        .iter()
                        .map(|(name, variance)| (name.clone(), variance.clone()))
                        .chain(type_param_kind_variances(&method.generic_bounds))
                        .collect::<HashMap<_, _>>();
                    for param in &method.params {
                        collect_kind_diagnostic_for_syntax_type(
                            &param.annotation,
                            trait_signatures,
                            &type_constructor_arities,
                            &type_constructor_variances,
                            &method_kinds,
                            &method_variances,
                            &mut diagnostics,
                        );
                    }
                    collect_kind_diagnostic_for_syntax_type(
                        &method.return_type,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &method_kinds,
                        &method_variances,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                let local_kinds = HashMap::new();
                let local_variances = HashMap::new();
                for prop in props {
                    collect_kind_diagnostic_for_syntax_type(
                        &prop.annotation,
                        trait_signatures,
                        &type_constructor_arities,
                        &type_constructor_variances,
                        &local_kinds,
                        &local_variances,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                ..
            } => {
                let local_kinds = HashMap::new();
                let local_variances = HashMap::new();
                collect_kind_diagnostic_for_syntax_type(
                    trait_ref,
                    trait_signatures,
                    &type_constructor_arities,
                    &type_constructor_variances,
                    &local_kinds,
                    &local_variances,
                    &mut diagnostics,
                );
                collect_kind_diagnostic_for_syntax_type(
                    for_type,
                    trait_signatures,
                    &type_constructor_arities,
                    &type_constructor_variances,
                    &local_kinds,
                    &local_variances,
                    &mut diagnostics,
                );
            }
            _ => {}
        }
    }
    diagnostics
}

/// Adds one kind diagnostic for a syntax type annotation when needed.
///
/// Inputs:
/// - `ty`: syntax-output type annotation to inspect.
/// - `trait_signatures`: visible trait signatures.
/// - `type_constructor_arities`: visible named type constructors and arities.
/// - `type_constructor_variances`: visible named type constructor variances.
/// - `local_kinds`: type parameters visible in the current declaration.
/// - `local_variances`: local HKT parameter slot variance requirements.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Scans trait applications inside the annotation text and appends a stable
///   diagnostic when an argument kind does not match the trait parameter kind.
fn collect_kind_diagnostic_for_syntax_type(
    ty: &SyntaxTypeOutput,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    type_constructor_arities: &HashMap<String, usize>,
    type_constructor_variances: &HashMap<String, Vec<Variance>>,
    local_kinds: &HashMap<String, usize>,
    local_variances: &HashMap<String, Vec<Option<Variance>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    collect_kind_diagnostics_for_trait_text(
        &ty.text,
        ty.span.into(),
        trait_signatures,
        type_constructor_arities,
        type_constructor_variances,
        local_kinds,
        local_variances,
        diagnostics,
    );
}

/// Collects all kind diagnostics inside one type-expression text.
///
/// Inputs:
/// - `text`: source type-expression text to scan.
/// - `span`: diagnostic span to report for any mismatch in this annotation.
/// - `trait_signatures`: visible trait signatures.
/// - `type_constructor_arities`: visible named type constructors and arities.
/// - `type_constructor_variances`: visible named type constructor variances.
/// - `local_kinds`: type parameters visible in the current declaration.
/// - `local_variances`: local HKT parameter slot variance requirements.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Scans every bracketed type application, checks applications whose head is
///   a trait, then recurses into each argument so nested trait references are
///   validated too.
fn collect_kind_diagnostics_for_trait_text(
    text: &str,
    span: Span,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    type_constructor_arities: &HashMap<String, usize>,
    type_constructor_variances: &HashMap<String, Vec<Variance>>,
    local_kinds: &HashMap<String, usize>,
    local_variances: &HashMap<String, Vec<Option<Variance>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for application in type_applications_in_text(text) {
        collect_type_application_arity_diagnostic(
            &application,
            span,
            trait_signatures,
            type_constructor_arities,
            local_kinds,
            diagnostics,
        );

        if let Some(signature) = trait_signatures.get(&application.name) {
            for (index, (param, arg)) in signature
                .type_params
                .iter()
                .zip(application.type_args.iter())
                .enumerate()
            {
                let expected = type_param_kind_arity(param);
                let found = type_argument_kind_arity(arg, type_constructor_arities, local_kinds);
                if expected != found {
                    diagnostics.push(Diagnostic {
                        span,
                        message: format!(
                            "kind mismatch: {} expects type argument {} of kind {}, found {} of kind {}",
                            application.name,
                            index + 1,
                            kind_arity_display(expected),
                            normalize_trait_type_text(arg),
                            kind_arity_display(found)
                        ),
                        severity: DiagSeverity::Error,
                    });
                }
                collect_type_application_variance_diagnostic(
                    &application.name,
                    index,
                    param,
                    arg,
                    span,
                    type_constructor_variances,
                    local_variances,
                    diagnostics,
                );
            }
        }

        for arg in application.type_args {
            collect_kind_diagnostics_for_trait_text(
                &arg,
                span,
                trait_signatures,
                type_constructor_arities,
                type_constructor_variances,
                local_kinds,
                local_variances,
                diagnostics,
            );
        }
    }
}

/// Adds a variance diagnostic for one higher-kinded trait argument.
///
/// Inputs:
/// - `trait_name`: trait application head used for diagnostics.
/// - `param_index`: zero-based trait parameter index.
/// - `param`: trait parameter declaration such as `F[+_]`.
/// - `arg`: supplied type argument text.
/// - `span`: source span for the enclosing annotation.
/// - `type_constructor_variances`: visible concrete constructor variance map.
/// - `local_variances`: visible local HKT constructor variance map.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Reads variance requirements from HKT slot markers, resolves the supplied
///   bare constructor's declared variance, and emits a stable diagnostic when
///   a required covariant or contravariant slot is not satisfied.
fn collect_type_application_variance_diagnostic(
    trait_name: &str,
    param_index: usize,
    param: &str,
    arg: &str,
    span: Span,
    type_constructor_variances: &HashMap<String, Vec<Variance>>,
    local_variances: &HashMap<String, Vec<Option<Variance>>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let requirements = type_param_kind_variance_requirements(param);
    if requirements.iter().all(Option::is_none) {
        return;
    }

    let Some(arg_name) = bare_type_constructor_name(arg) else {
        return;
    };
    let actual = local_variances
        .get(&arg_name)
        .map(|slots| {
            slots
                .iter()
                .map(|slot| slot.unwrap_or(Variance::Invariant))
                .collect::<Vec<_>>()
        })
        .or_else(|| type_constructor_variances.get(&arg_name).cloned())
        .unwrap_or_else(|| vec![Variance::Invariant; requirements.len()]);

    for (slot_index, requirement) in requirements.iter().enumerate() {
        let Some(required) = requirement else {
            continue;
        };
        let actual = actual
            .get(slot_index)
            .copied()
            .unwrap_or(Variance::Invariant);
        if actual == *required {
            continue;
        }
        diagnostics.push(Diagnostic {
            span,
            message: format!(
                "{} expects type argument {} slot {} to be {}, found {} constructor {}",
                trait_name,
                param_index + 1,
                slot_index + 1,
                variance_display(*required),
                variance_display(actual),
                normalize_trait_type_text(arg)
            ),
            severity: DiagSeverity::Error,
        });
    }
}

/// Adds an arity diagnostic for one visible type-constructor application.
///
/// Inputs:
/// - `application`: parsed type application such as `F[A]` or `Option[Int]`.
/// - `span`: source span for the enclosing annotation.
/// - `trait_signatures`: visible traits keyed by source-visible name.
/// - `type_constructor_arities`: visible concrete type constructors and arities.
/// - `local_kinds`: declaration-local type parameters and constructor arities.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Resolves the expected arity from trait parameters, local HKT parameters,
///   or concrete type aliases/constructors, then reports when the source
///   applies too many or too few type arguments.
fn collect_type_application_arity_diagnostic(
    application: &ParsedTraitInstance,
    span: Span,
    trait_signatures: &HashMap<String, ParsedTraitSignature>,
    type_constructor_arities: &HashMap<String, usize>,
    local_kinds: &HashMap<String, usize>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let expected = trait_signatures
        .get(&application.name)
        .map(|signature| signature.type_params.len())
        .or_else(|| local_kinds.get(&application.name).copied())
        .or_else(|| type_constructor_arities.get(&application.name).copied());

    let Some(expected) = expected else {
        return;
    };

    if expected == application.type_args.len() {
        return;
    }

    diagnostics.push(Diagnostic {
        span,
        message: format!(
            "type constructor `{}` expects {} type argument(s), found {}",
            application.name,
            expected,
            application.type_args.len()
        ),
        severity: DiagSeverity::Error,
    });
}

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
fn type_constructor_arities(
    module: &SyntaxModuleOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, usize> {
    let mut arities = HashMap::from([
        ("List".to_string(), 1usize),
        ("Map".to_string(), 2usize),
        ("FixedArray".to_string(), 2usize),
    ]);

    for (name, alias) in aliases {
        arities.insert(name.clone(), alias.params.len());
        if let Some(last) = name.rsplit('.').next() {
            arities
                .entry(last.to_string())
                .or_insert(alias.params.len());
        }
    }

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, params, .. }
            | SyntaxDeclarationPayload::Constructor { name, params, .. } => {
                arities.insert(name.clone(), params.len());
            }
            _ => {}
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
fn type_constructor_variances(
    module: &SyntaxModuleOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> HashMap<String, Vec<Variance>> {
    let mut variances = HashMap::from([
        ("List".to_string(), vec![Variance::Covariant]),
        (
            "Map".to_string(),
            vec![Variance::Covariant, Variance::Covariant],
        ),
        ("FixedArray".to_string(), vec![Variance::Covariant]),
    ]);

    for (name, alias) in aliases {
        variances.insert(name.clone(), alias.param_variance.clone());
        if let Some(last) = name.rsplit('.').next() {
            variances
                .entry(last.to_string())
                .or_insert_with(|| alias.param_variance.clone());
        }
    }

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, params, .. }
            | SyntaxDeclarationPayload::Constructor { name, params, .. } => {
                variances.insert(name.clone(), type_param_variances(params));
            }
            _ => {}
        }
    }

    variances
}

/// Builds a kind-arity map from type parameter declarations.
///
/// Inputs:
/// - `params`: source type parameter texts such as `T`, `-E`, or `F[_]`.
///
/// Output:
/// - Map from normalized type parameter name to higher-kind arity.
///
/// Transformation:
/// - Strips variance markers and counts `_` slots; first-order parameters have
///   arity zero.
fn type_param_kind_arities(params: &[String]) -> HashMap<String, usize> {
    params
        .iter()
        .filter_map(|param| {
            let name = type_param_base_name(param)?;
            Some((name, type_param_kind_arity(param)))
        })
        .collect()
}

/// Builds a kind-variance map from type parameter declarations.
///
/// Inputs:
/// - `params`: source type parameter texts such as `T`, `F[_]`, or `F[+_]`.
///
/// Output:
/// - Map from normalized type parameter name to HKT slot variance requirements.
///
/// Transformation:
/// - Strips outer parameter variance and records each HKT slot as `None` for
///   `_`, covariant for `+_`, or contravariant for `-_`.
fn type_param_kind_variances(params: &[String]) -> HashMap<String, Vec<Option<Variance>>> {
    params
        .iter()
        .filter_map(|param| {
            let name = type_param_base_name(param)?;
            Some((name, type_param_kind_variance_requirements(param)))
        })
        .collect()
}

/// Returns the base name for a type parameter declaration.
///
/// Inputs:
/// - `param`: type parameter text, optionally variance-marked and HKT-slotted.
///
/// Output:
/// - Base upper-case parameter name, or `None` for malformed text.
///
/// Transformation:
/// - Removes leading variance and trailing bracketed slot metadata.
fn type_param_base_name(param: &str) -> Option<String> {
    let trimmed = param.trim().trim_start_matches(['+', '-']);
    let base = trimmed.split_once('[').map_or(trimmed, |(base, _)| base);
    (!base.is_empty()).then(|| base.to_string())
}

/// Returns the kind arity for one type parameter declaration.
///
/// Inputs:
/// - `param`: type parameter text, optionally containing `_` slots.
///
/// Output:
/// - Zero for first-order parameters, or the number of `_` slots.
///
/// Transformation:
/// - Counts slot markers inside the bracketed HKT parameter declaration.
fn type_param_kind_arity(param: &str) -> usize {
    let Some((_, slots)) = param.split_once('[') else {
        return 0;
    };
    slots.chars().filter(|ch| *ch == '_').count()
}

/// Returns HKT slot variance requirements for a type parameter declaration.
///
/// Inputs:
/// - `param`: type parameter text, optionally containing `_`, `+_`, or `-_`
///   slots.
///
/// Output:
/// - One optional variance requirement per HKT slot.
///
/// Transformation:
/// - Parses only the bracketed HKT slot list. Plain `_` means no variance
///   requirement, while `+_` and `-_` require covariance and contravariance.
fn type_param_kind_variance_requirements(param: &str) -> Vec<Option<Variance>> {
    let Some((_, slots)) = param.split_once('[') else {
        return Vec::new();
    };
    let Some((slots, _)) = slots.rsplit_once(']') else {
        return Vec::new();
    };
    slots
        .split(',')
        .map(|slot| match compact_spaces(slot).as_str() {
            "+_" => Some(Variance::Covariant),
            "-_" => Some(Variance::Contravariant),
            _ => None,
        })
        .collect()
}

/// Determines the kind arity of one trait type argument.
///
/// Inputs:
/// - `arg`: type argument text from a trait application.
/// - `type_constructor_arities`: visible named type constructors and arities.
/// - `local_kinds`: type parameters visible in the current declaration.
///
/// Output:
/// - Zero for ordinary types, or the constructor arity for bare constructor
///   arguments such as `Option` or `F`.
///
/// Transformation:
/// - Treats applied forms like `Option[Int]` as first-order `Type`, and only
///   reports higher kind for bare visible constructors or HKT parameters.
fn type_argument_kind_arity(
    arg: &str,
    type_constructor_arities: &HashMap<String, usize>,
    local_kinds: &HashMap<String, usize>,
) -> usize {
    let compact = compact_spaces(arg);
    if compact.contains('[')
        || compact.contains('(')
        || compact.contains('{')
        || compact.contains('|')
        || compact.contains("->")
    {
        return 0;
    }
    if let Some(arity) = local_kinds.get(&compact) {
        return *arity;
    }
    type_constructor_arities.get(&compact).copied().unwrap_or(0)
}

/// Extracts a bare type-constructor argument name.
///
/// Inputs:
/// - `arg`: source type argument text from a trait application.
///
/// Output:
/// - Constructor name when `arg` is a bare name, otherwise `None`.
///
/// Transformation:
/// - Mirrors kind-arity detection by rejecting applied, structural, union, and
///   function type expressions.
fn bare_type_constructor_name(arg: &str) -> Option<String> {
    let compact = compact_spaces(arg);
    if compact.contains('[')
        || compact.contains('(')
        || compact.contains('{')
        || compact.contains('|')
        || compact.contains("->")
    {
        return None;
    }
    (!compact.is_empty()).then_some(compact)
}

/// Renders variance for diagnostics.
///
/// Inputs:
/// - `variance`: variance marker from a source type parameter.
///
/// Output:
/// - Stable lower-case diagnostic text.
///
/// Transformation:
/// - Keeps diagnostics independent from Rust enum debug formatting.
fn variance_display(variance: Variance) -> &'static str {
    match variance {
        Variance::Invariant => "invariant",
        Variance::Covariant => "covariant",
        Variance::Contravariant => "contravariant",
    }
}

/// Renders a kind arity for diagnostics.
///
/// Inputs:
/// - `arity`: zero for ordinary `Type`, or a higher-kind constructor arity.
///
/// Output:
/// - Stable source-facing kind text.
///
/// Transformation:
/// - Writes `Type`, `Type -> Type`, or a repeated arrow chain.
fn kind_arity_display(arity: usize) -> String {
    if arity == 0 {
        return "Type".to_string();
    }
    std::iter::repeat("Type")
        .take(arity + 1)
        .collect::<Vec<_>>()
        .join(" -> ")
}

/// Extracts type applications from source type text.
///
/// Inputs:
/// - `text`: source type-expression text.
///
/// Output:
/// - Parsed trait-instance-like applications found in the text.
///
/// Transformation:
/// - Uses the canonical lexer, then scans for name tokens followed by balanced
///   brackets. Each balanced slice is parsed by the existing trait-instance
///   parser so nested argument commas remain source-compatible.
fn type_applications_in_text(text: &str) -> Vec<ParsedTraitInstance> {
    let Ok(tokens) = terlan_syntax::lexer::lex(text) else {
        return Vec::new();
    };
    let mut applications = Vec::new();
    let mut index = 0usize;
    while index + 1 < tokens.len() {
        if tokens[index + 1].kind != TokenKind::LBracket {
            index += 1;
            continue;
        }
        let mut end = index + 2;
        let mut depth = 1i32;
        while end < tokens.len() {
            match tokens[end].kind {
                TokenKind::LBracket => depth += 1,
                TokenKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        end += 1;
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        if depth == 0 {
            if let Some(instance) = parse_trait_instance_from_text(
                &tokens[index..end]
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
            ) {
                applications.push(instance);
            }
            index = end;
        } else {
            break;
        }
    }
    applications
}

/// Checks public constructor signatures for private local return-type leaks.
///
/// Inputs:
/// - `module`: syntax-output module containing constructor declarations.
/// - `resolved`: resolved module carrying local type visibility.
/// - `alias_names`: visible type names used to parse constructor return
///   annotations.
///
/// Output:
/// - One error diagnostic for each public constructor clause whose return type
///   exposes a private local type.
///
/// Transformation:
/// - Parses constructor return annotations into the type model and recursively
///   scans compound return types for private local type names. Imported or
///   module-qualified names are not treated as local private leaks here.
pub(crate) fn check_syntax_public_constructor_return_visibility(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    alias_names: &HashSet<String>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Constructor {
            name,
            is_public,
            clauses,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !*is_public {
            continue;
        }

        for clause in clauses {
            let mut vars = HashMap::new();
            let mut next_var: TypeVarId = 0;
            let ret = parse_type_expr(
                &clause.return_type.text,
                alias_names,
                &mut vars,
                &mut next_var,
            )
            .unwrap_or(Type::Dynamic);

            if let Some(private_type) =
                first_private_local_type_name(&ret, &resolved.local_type_names)
            {
                diagnostics.push(Diagnostic {
                    span: clause.return_type.span.into(),
                    message: format!(
                        "public constructor {} exposes private return type {}",
                        name, private_type
                    ),
                    severity: DiagSeverity::Error,
                });
            }
        }
    }

    diagnostics
}

/// Finds the first private local type mentioned by a parsed type expression.
///
/// Inputs:
/// - `ty`: parsed type expression to inspect.
/// - `local_type_names`: resolver map of local type names to visibility.
///
/// Output:
/// - `Some(name)` for the first private unqualified local type reference.
/// - `None` when the type does not expose a private local type.
///
/// Transformation:
/// - Recursively walks lists, tuples, unions, maps, function types, fixed
///   arrays, and named type arguments while ignoring primitives, variables,
///   literals, and qualified/imported type names.
fn first_private_local_type_name(
    ty: &Type,
    local_type_names: &HashMap<String, TypeVisibility>,
) -> Option<String> {
    match ty {
        Type::Named {
            module: None,
            name,
            args,
        } => {
            if local_type_names.get(name) == Some(&TypeVisibility::Private) {
                return Some(name.clone());
            }
            args.iter()
                .find_map(|arg| first_private_local_type_name(arg, local_type_names))
        }
        Type::Named { args, .. } => args
            .iter()
            .find_map(|arg| first_private_local_type_name(arg, local_type_names)),
        Type::Apply { args, .. } => args
            .iter()
            .find_map(|arg| first_private_local_type_name(arg, local_type_names)),
        Type::Existential { body, .. } => first_private_local_type_name(body, local_type_names),
        Type::List(inner) => first_private_local_type_name(inner, local_type_names),
        Type::Tuple(items) | Type::Union(items) => items
            .iter()
            .find_map(|item| first_private_local_type_name(item, local_type_names)),
        Type::Map(fields) => fields
            .iter()
            .find_map(|field| first_private_local_type_name(&field.value, local_type_names)),
        Type::Function { params, ret } => params
            .iter()
            .chain(std::iter::once(ret.as_ref()))
            .find_map(|item| first_private_local_type_name(item, local_type_names)),
        Type::FixedArray { elem, .. } => first_private_local_type_name(elem, local_type_names),
        Type::Int
        | Type::Float
        | Type::Number
        | Type::Binary
        | Type::Atom
        | Type::Bool
        | Type::Term
        | Type::Dynamic
        | Type::Never
        | Type::Placeholder
        | Type::LiteralAtom(_)
        | Type::LiteralInt(_)
        | Type::Var(_) => None,
    }
}

/// Validates macro function return signatures.
///
/// Inputs:
/// - `module`: syntax-output module containing function declarations.
///
/// Output:
/// - Diagnostics for macro declarations whose return type is not `Ast[T]`.
///
/// Transformation:
/// - Scans only functions marked as macros and validates their return
///   annotation with the macro return-type shape helper.
pub(crate) fn check_syntax_macro_decl_signatures(module: &SyntaxModuleOutput) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Function {
            name,
            return_type,
            is_macro,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !is_macro {
            continue;
        }

        if !is_valid_macro_return_type(&return_type.text) {
            diagnostics.push(Diagnostic {
                span: return_type.span.into(),
                message: format!(
                    "macro `{}` must return Ast[T], found {}",
                    name, return_type.text
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Checks whether a macro return annotation has the required `Ast[T]` shape.
///
/// Inputs:
/// - `annotation`: source return type annotation text.
///
/// Output:
/// - `true` when the annotation is an `Ast` application with exactly one
///   non-empty type argument.
///
/// Transformation:
/// - Compacts whitespace, splits a named type application, and validates only
///   the structural return-type shape required for macro declarations.
fn is_valid_macro_return_type(annotation: &str) -> bool {
    let src = compact_spaces(annotation);
    let Some((base, args)) = split_named_type(&src) else {
        return false;
    };
    if base != "Ast" {
        return false;
    }

    let args = split_top_level_csv(&args);
    args.len() == 1 && !args[0].trim().is_empty()
}
