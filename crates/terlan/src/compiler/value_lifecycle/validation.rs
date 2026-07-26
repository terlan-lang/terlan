fn validate_valued_union_case_exhaustiveness(
    module: &SyntaxModuleOutput,
) -> Vec<ValueLifecycleDiagnostic> {
    let unions = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                representation: Some(_),
                valued_arms,
                ..
            } => Some((
                name.clone(),
                valued_arms
                    .iter()
                    .map(|arm| arm.name.clone())
                    .collect::<HashSet<_>>(),
            )),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        for_each_runtime_expr(declaration, |expr| {
            if expr.kind != SyntaxExprKind::Case {
                return;
            }
            for (union, required) in &unions {
                let mut covered = HashSet::new();
                let mut mentions_union = false;
                let mut catch_all = false;
                for clause in &expr.clauses {
                    let Some(pattern) = clause.patterns.first() else {
                        continue;
                    };
                    catch_all |= matches!(
                        pattern.kind,
                        SyntaxPatternKind::Wildcard
                            | SyntaxPatternKind::Var
                            | SyntaxPatternKind::Ignore
                            | SyntaxPatternKind::Placeholder
                    );
                    if pattern.kind == SyntaxPatternKind::Constructor {
                        if let Some(member) = pattern
                            .text
                            .as_deref()
                            .and_then(|name| name.strip_prefix(&format!("{union}.")))
                        {
                            mentions_union = true;
                            covered.insert(member.to_string());
                        }
                    }
                }
                if mentions_union && !catch_all && &covered != required {
                    let mut missing = required.difference(&covered).cloned().collect::<Vec<_>>();
                    missing.sort();
                    diagnostics.push(diagnostic(
                        "NON_EXHAUSTIVE_VALUED_UNION",
                        format!(
                            "case over valued union `{union}` is missing arms: {}",
                            missing.join(", ")
                        ),
                        expr.span,
                    ));
                }
            }
        });
    }
    diagnostics
}

fn for_each_runtime_expr(
    declaration: &crate::terlan_syntax::SyntaxDeclarationOutput,
    mut visit: impl FnMut(&SyntaxExprOutput),
) {
    fn walk(expr: &SyntaxExprOutput, visit: &mut dyn FnMut(&SyntaxExprOutput)) {
        visit(expr);
        for child in &expr.children {
            walk(child, visit);
        }
        for field in &expr.fields {
            walk(&field.value, visit);
        }
        for clause in expr.clauses.iter().chain(&expr.catch_clauses) {
            if let Some(guard) = &clause.guard {
                walk(guard, visit);
            }
            walk(&clause.body, visit);
        }
    }
    match &declaration.payload {
        SyntaxDeclarationPayload::Function { clauses, .. } => {
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    walk(guard, &mut visit);
                }
                walk(&clause.body, &mut visit);
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                walk(&clause.body, &mut visit);
            }
        }
        _ => {}
    }
}

fn validate_runtime_constant_reflection(
    module: &SyntaxModuleOutput,
) -> Vec<ValueLifecycleDiagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        for_each_runtime_expr(declaration, |expr| {
            if expr.kind != SyntaxExprKind::Call {
                return;
            }
            let name = expr
                .children
                .first()
                .and_then(|callee| callee.text.as_deref())
                .or(expr.text.as_deref())
                .unwrap_or_default();
            if matches!(name, "constants" | "constant_by_name" | "reflect_constant") {
                diagnostics.push(diagnostic(
                    "CONST_RUNTIME_REFLECTION_FORBIDDEN",
                    format!(
                        "runtime constant reflection `{name}` is not available; declare an explicit constant aggregate"
                    ),
                    expr.span,
                ));
            }
        });
    }
    diagnostics
}

/// Reports whether a module needs lifecycle preparation even when it declares
/// no constants of its own.
///
/// Const-generic surfaces and forbidden reflection calls are lifecycle syntax
/// too. Keeping the predicate beside their validators prevents typechecking's
/// stack-safety fast path from silently bypassing those checks.
pub(crate) fn module_requires_value_lifecycle_pass(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> bool {
    let has_const_params = |params: &[String]| {
        params
            .iter()
            .any(|param| param.trim_start().starts_with("const "))
    };
    if module
        .declarations
        .iter()
        .any(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constant { .. }
            | SyntaxDeclarationPayload::ConstFunction { .. } => true,
            SyntaxDeclarationPayload::Type {
                params,
                valued_arms,
                ..
            } => !valued_arms.is_empty() || has_const_params(params),
            SyntaxDeclarationPayload::Struct { generic_params, .. }
            | SyntaxDeclarationPayload::Function { generic_params, .. }
            | SyntaxDeclarationPayload::Method { generic_params, .. }
            | SyntaxDeclarationPayload::TraitImpl { generic_params, .. } => {
                has_const_params(generic_params)
            }
            SyntaxDeclarationPayload::Trait {
                params, constants, ..
            } => !constants.is_empty() || has_const_params(params),
            _ => false,
        })
    {
        return true;
    }
    let mut imported_modules = HashSet::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Import {
            module_name, items, ..
        } = &declaration.payload
        {
            if interfaces.contains_key(module_name) {
                imported_modules.insert(module_name.clone());
            }
            for item in items {
                let nested = format!("{module_name}.{}", item.name);
                if interfaces.contains_key(&nested) {
                    imported_modules.insert(nested);
                }
            }
        }
    }
    if interfaces.iter().any(|(module_name, interface)| {
        imported_modules.contains(module_name)
            && (!interface.constants.is_empty()
                || !interface.const_functions.is_empty()
                || !interface.valued_unions.is_empty()
                || !interface.associated_constants.is_empty()
                || interface
                    .type_params
                    .values()
                    .any(|params| has_const_params(params))
                || interface
                    .traits
                    .values()
                    .any(|signature| has_const_params(&signature.type_params))
                || interface
                    .functions
                    .values()
                    .any(|signature| has_const_params(&signature.generic_params)))
    }) {
        return true;
    }
    module.declarations.iter().any(|declaration| {
        let mut found = false;
        for_each_runtime_expr(declaration, |expr| {
            if expr.kind != SyntaxExprKind::Call {
                return;
            }
            let name = expr
                .children
                .first()
                .and_then(|callee| callee.text.as_deref())
                .or(expr.text.as_deref())
                .unwrap_or_default();
            found |= matches!(name, "constants" | "constant_by_name" | "reflect_constant");
        });
        found
    })
}

fn validate_nominal_valued_union_uses(
    module: &SyntaxModuleOutput,
) -> Vec<ValueLifecycleDiagnostic> {
    let unions = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                representation: Some(_),
                ..
            } => Some(name.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    if unions.is_empty() {
        return Vec::new();
    }
    let constant_types = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constant {
                name, annotation, ..
            } => Some((name.clone(), annotation.text.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let functions = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                ..
            } => Some((
                (name.clone(), params.len()),
                (
                    params
                        .iter()
                        .map(|param| param.annotation.text.clone())
                        .collect::<Vec<_>>(),
                    return_type.text.clone(),
                ),
            )),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                return_type,
                clauses,
                ..
            } => {
                for clause in clauses {
                    validate_union_expected_expr(
                        &clause.body,
                        &return_type.text,
                        &unions,
                        &constant_types,
                        &functions,
                        &mut diagnostics,
                    );
                    validate_union_call_arguments(
                        &clause.body,
                        &unions,
                        &constant_types,
                        &functions,
                        &mut diagnostics,
                    );
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    if let Some(default) = &field.default {
                        validate_union_expected_expr(
                            default,
                            &field.annotation.text,
                            &unions,
                            &constant_types,
                            &functions,
                            &mut diagnostics,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    diagnostics
}

fn validate_union_expected_expr(
    expr: &SyntaxExprOutput,
    expected: &str,
    unions: &HashSet<String>,
    constant_types: &HashMap<String, String>,
    functions: &HashMap<(String, usize), (Vec<String>, String)>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    let expected = expected.trim();
    if !unions.contains(expected) || expr_produces_union(expr, expected, constant_types, functions)
    {
        return;
    }
    diagnostics.push(diagnostic(
        "IMPLICIT_VALUED_UNION_CONVERSION",
        format!(
            "representation values do not implicitly convert to valued union `{expected}`; use a `{expected}.ARM` constant or checked parsing"
        ),
        expr.span,
    ));
}

fn expr_produces_union(
    expr: &SyntaxExprOutput,
    union: &str,
    constant_types: &HashMap<String, String>,
    functions: &HashMap<(String, usize), (Vec<String>, String)>,
) -> bool {
    if expr
        .raw
        .as_deref()
        .is_some_and(|raw| raw == format!("checked_valued_union_parse:{union}"))
    {
        return true;
    }
    if expr.kind == SyntaxExprKind::Case {
        return !expr.clauses.is_empty()
            && expr.clauses.iter().all(|clause| {
                clause
                    .body
                    .raw
                    .as_deref()
                    .is_some_and(|raw| raw.starts_with(&format!("const_union:{union}.")))
            });
    }
    if expr.kind == SyntaxExprKind::FieldAccess {
        return qualified_expr_name(expr)
            .is_some_and(|name| name.starts_with(&format!("{union}.")));
    }
    if expr.kind == SyntaxExprKind::Var {
        if expr
            .text
            .as_deref()
            .is_some_and(|name| name.starts_with(&format!("{union}.")))
        {
            return true;
        }
        return expr
            .text
            .as_ref()
            .and_then(|name| constant_types.get(name))
            .is_some_and(|ty| ty.trim() == union);
    }
    if matches!(
        expr.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) {
        let name = expr
            .children
            .first()
            .and_then(|callee| callee.text.as_ref());
        let arity = expr.children.len().saturating_sub(1);
        return name
            .and_then(|name| functions.get(&(name.clone(), arity)))
            .is_some_and(|(_, return_type)| return_type.trim() == union);
    }
    false
}

fn validate_union_call_arguments(
    expr: &SyntaxExprOutput,
    unions: &HashSet<String>,
    constant_types: &HashMap<String, String>,
    functions: &HashMap<(String, usize), (Vec<String>, String)>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    if matches!(
        expr.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) {
        let name = expr
            .children
            .first()
            .and_then(|callee| callee.text.as_ref());
        let arity = expr.children.len().saturating_sub(1);
        if let Some((params, _)) = name.and_then(|name| functions.get(&(name.clone(), arity))) {
            for (arg, expected) in expr.children.iter().skip(1).zip(params) {
                validate_union_expected_expr(
                    arg,
                    expected,
                    unions,
                    constant_types,
                    functions,
                    diagnostics,
                );
            }
        }
    }
    for child in &expr.children {
        validate_union_call_arguments(child, unions, constant_types, functions, diagnostics);
    }
    for field in &expr.fields {
        validate_union_call_arguments(&field.value, unions, constant_types, functions, diagnostics);
    }
}

fn validate_constant_namespaces(module: &SyntaxModuleOutput) -> Vec<ValueLifecycleDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut values = HashMap::<String, (&'static str, EbnfSourceSpan)>::new();
    let mut ordinary_functions = HashSet::new();
    let mut const_functions = HashSet::new();
    for declaration in &module.declarations {
        let value = match &declaration.payload {
            SyntaxDeclarationPayload::Constant { name, .. } => {
                Some((name, "constant", declaration.span))
            }
            SyntaxDeclarationPayload::Constructor { name, .. } => {
                Some((name, "constructor", declaration.span))
            }
            SyntaxDeclarationPayload::Function { name, params, .. } => {
                ordinary_functions.insert((name.clone(), params.len()));
                None
            }
            SyntaxDeclarationPayload::ConstFunction { name, params, .. } => {
                const_functions.insert((name.clone(), params.len()));
                None
            }
            _ => None,
        };
        if let Some((name, category, span)) = value {
            if let Some((previous, _)) = values.insert(name.clone(), (category, span)) {
                diagnostics.push(diagnostic(
                    "AMBIGUOUS_CONSTANT_NAME",
                    format!(
                        "`{name}` is declared as both {previous} and {category} in the value/pattern namespace"
                    ),
                    span,
                ));
            }
        }
    }
    for (name, arity) in ordinary_functions.intersection(&const_functions) {
        diagnostics.push(diagnostic(
            "AMBIGUOUS_CONST_FUNCTION",
            format!("const function and runtime function cannot both declare `{name}/{arity}`"),
            EbnfSourceSpan::default(),
        ));
    }
    diagnostics
}

fn validate_forbidden_constant_contexts(
    module: &SyntaxModuleOutput,
    evaluator: &Evaluator,
) -> Vec<ValueLifecycleDiagnostic> {
    let mut names = evaluator
        .definitions
        .keys()
        .chain(evaluator.values.keys())
        .cloned()
        .collect::<HashSet<_>>();
    names.extend(
        evaluator
            .definitions
            .keys()
            .filter_map(|name| name.rsplit('.').next().map(str::to_string)),
    );
    let mut diagnostics = Vec::new();
    let mut type_copy = module.clone();
    for declaration in &mut type_copy.declarations {
        visit_declaration_type_texts_mut(&mut declaration.payload, |ty| {
            if let Some(name) = referenced_constant_name(&ty.text, &names) {
                diagnostics.push(forbidden_constant_context(name, "type annotation", ty.span));
            }
        });
    }

    for declaration in &module.declarations {
        for annotation in &declaration.annotations {
            if annotation
                .args
                .as_deref()
                .and_then(|text| referenced_constant_name(text, &names))
                .is_some()
                || annotation
                    .values
                    .iter()
                    .any(|value| annotation_value_references_constant(value, &names))
                || annotation
                    .entries
                    .iter()
                    .any(|entry| annotation_value_references_constant(&entry.value, &names))
            {
                diagnostics.push(forbidden_constant_context(
                    "constant",
                    "annotation metadata",
                    annotation.span,
                ));
            }
        }
        match &declaration.payload {
            SyntaxDeclarationPayload::AnnotationSchema { entries, .. } => {
                for entry in entries {
                    if let SyntaxAnnotationSchemaEntryOutput::Key {
                        value_type, span, ..
                    } = entry
                    {
                        if let Some(name) = referenced_constant_name(value_type, &names) {
                            diagnostics.push(forbidden_constant_context(
                                name,
                                "annotation schema",
                                *span,
                            ));
                        }
                    }
                }
            }
            SyntaxDeclarationPayload::Config { text, entries, .. } => {
                if referenced_constant_name(text, &names).is_some()
                    || entries
                        .iter()
                        .any(|entry| config_value_references_constant(&entry.value, &names))
                {
                    diagnostics.push(forbidden_constant_context(
                        "constant",
                        "target/native/machine/static configuration",
                        declaration.span,
                    ));
                }
            }
            SyntaxDeclarationPayload::Import {
                source_path: Some(path),
                ..
            }
            | SyntaxDeclarationPayload::Template {
                source_path: path, ..
            } => {
                if let Some(name) = referenced_constant_name(path, &names) {
                    diagnostics.push(forbidden_constant_context(
                        name,
                        "asset or template path",
                        declaration.span,
                    ));
                }
            }
            _ => {}
        }
    }
    diagnostics
}

fn referenced_constant_name<'a>(text: &str, names: &'a HashSet<String>) -> Option<&'a str> {
    names.iter().find_map(|name| {
        text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
            .any(|word| word == name)
            .then_some(name.as_str())
    })
}

fn annotation_value_references_constant(
    value: &SyntaxAnnotationValueOutput,
    names: &HashSet<String>,
) -> bool {
    match value {
        SyntaxAnnotationValueOutput::Name { segments } => {
            let qualified = segments.join(".");
            names.contains(&qualified) || segments.last().is_some_and(|name| names.contains(name))
        }
        SyntaxAnnotationValueOutput::List { values } => values
            .iter()
            .any(|value| annotation_value_references_constant(value, names)),
        SyntaxAnnotationValueOutput::Object { entries } => entries
            .iter()
            .any(|entry| annotation_value_references_constant(&entry.value, names)),
        _ => false,
    }
}

fn config_value_references_constant(
    value: &SyntaxConfigValueOutput,
    names: &HashSet<String>,
) -> bool {
    match value {
        SyntaxConfigValueOutput::Symbol { value } => {
            referenced_constant_name(value, names).is_some()
        }
        SyntaxConfigValueOutput::List { values } => values
            .iter()
            .any(|value| config_value_references_constant(value, names)),
        SyntaxConfigValueOutput::Map { entries } => entries
            .iter()
            .any(|entry| config_value_references_constant(&entry.value, names)),
        _ => false,
    }
}

fn forbidden_constant_context(
    name: impl std::fmt::Display,
    context: &str,
    span: EbnfSourceSpan,
) -> ValueLifecycleDiagnostic {
    diagnostic(
        "CONSTANT_FORBIDDEN_CONTEXT",
        format!("constant reference `{name}` is not allowed in {context}"),
        span,
    )
}

pub(crate) fn evaluate_and_substitute_module_constants(
    module: &mut SyntaxModuleOutput,
) -> ValueLifecycleReport {
    evaluate_and_substitute_module_constants_with_interfaces(module, &HashMap::new())
}

pub(crate) fn expression_is_const_safe(expr: &SyntaxExprOutput) -> bool {
    let mut evaluator = Evaluator {
        definitions: HashMap::new(),
        functions: HashMap::new(),
        values: HashMap::new(),
        active: Vec::new(),
        steps: 0,
    };
    evaluator.evaluate_expr(expr, &HashMap::new()).is_ok()
}

fn validate_valued_unions(
    module: &SyntaxModuleOutput,
    values: &HashMap<String, ConstValue>,
) -> Vec<ValueLifecycleDiagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Type {
            name,
            representation: Some(_),
            valued_arms,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        let mut names = HashSet::new();
        let mut representations = HashMap::<String, String>::new();
        for arm in valued_arms {
            if !names.insert(arm.name.clone()) {
                diagnostics.push(diagnostic(
                    "DUPLICATE_VALUED_UNION_ARM",
                    format!("duplicate valued-union arm `{}.{}`", name, arm.name),
                    arm.span,
                ));
            }
            let Some(ConstValue::Union { representation, .. }) =
                values.get(&format!("{name}.{}", arm.name))
            else {
                continue;
            };
            let text = representation.stable_text();
            if let Some(previous) = representations.insert(text, arm.name.clone()) {
                diagnostics.push(diagnostic(
                    "DUPLICATE_VALUED_UNION_VALUE",
                    format!(
                        "valued-union arms `{}.{previous}` and `{}.{}` have the same value",
                        name, name, arm.name
                    ),
                    arm.span,
                ));
            }
        }
    }
    diagnostics
}

fn validate_trait_constants(
    module: &SyntaxModuleOutput,
    evaluator: &mut Evaluator,
) -> Vec<ValueLifecycleDiagnostic> {
    let traits = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Trait {
                name, constants, ..
            } => Some((name.clone(), constants.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            constants,
            is_negative,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if *is_negative {
            if !constants.is_empty() {
                diagnostics.push(diagnostic(
                    "NEGATIVE_IMPL_CONSTANT",
                    "negative trait implementations cannot provide constants",
                    declaration.span,
                ));
            }
            continue;
        }
        let trait_name = trait_ref.text.split('[').next().unwrap_or(&trait_ref.text);
        let Some(required) = traits.get(trait_name) else {
            continue;
        };
        let provided = constants
            .iter()
            .map(|constant| constant.name.as_str())
            .collect::<HashSet<_>>();
        for constant in constants {
            let Some(contract) = required.iter().find(|item| item.name == constant.name) else {
                diagnostics.push(diagnostic(
                    "UNDECLARED_TRAIT_CONSTANT",
                    format!(
                        "implementation provides undeclared associated constant `{}`",
                        constant.name
                    ),
                    constant.span,
                ));
                continue;
            };
            match evaluator.evaluate_expr(&constant.value, &HashMap::new()) {
                Ok(value) => {
                    if let Err(error) =
                        ensure_type(&contract.annotation.text, &value, constant.span)
                    {
                        diagnostics.push(error);
                    }
                }
                Err(error) => diagnostics.push(error),
            }
        }
        for contract in required {
            if contract.default.is_none() && !provided.contains(contract.name.as_str()) {
                diagnostics.push(diagnostic(
                    "MISSING_TRAIT_CONSTANT",
                    format!(
                        "implementation is missing associated constant `{}`",
                        contract.name
                    ),
                    declaration.span,
                ));
            }
        }
    }
    diagnostics
}

fn associated_constant_owner(trait_ref: &str, for_type: &str) -> String {
    if trait_ref.contains('[') {
        trait_ref.to_string()
    } else {
        format!("{trait_ref}[{for_type}]")
    }
}
