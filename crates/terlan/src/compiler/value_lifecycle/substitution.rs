use super::*;

pub(super) fn substitute_module(
    module: &mut SyntaxModuleOutput,
    evaluator: &mut Evaluator,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    for declaration in &mut module.declarations {
        evaluate_declaration_defaults(&mut declaration.payload, evaluator, diagnostics);
    }
    let values = evaluator.values.clone();
    let const_functions = evaluator.functions.clone();
    for declaration in &mut module.declarations {
        match &mut declaration.payload {
            SyntaxDeclarationPayload::Constant { name, value, .. } => {
                if let Some(resolved) = values.get(name) {
                    *value = value_to_expr(resolved, value.span);
                }
            }
            SyntaxDeclarationPayload::ConstFunction { .. } => {}
            SyntaxDeclarationPayload::Type {
                name, valued_arms, ..
            } => {
                for arm in valued_arms {
                    if let Some(value) = values.get(&format!("{name}.{}", arm.name)) {
                        arm.value = value_to_expr(value, arm.span);
                    }
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    if let Some(default) = &mut field.default {
                        substitute_expr(default, &values, &const_functions, diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    for param in &mut clause.params {
                        if let Some(default) = &mut param.default {
                            substitute_expr(default, &values, &const_functions, diagnostics);
                        }
                    }
                    substitute_expr(&mut clause.body, &values, &const_functions, diagnostics);
                }
            }
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            } => {
                substitute_params(params, &values, &const_functions, diagnostics);
                for clause in clauses {
                    for pattern in &mut clause.patterns {
                        substitute_pattern(pattern, &values);
                    }
                    if let Some(guard) = &mut clause.guard {
                        substitute_expr(guard, &values, &const_functions, diagnostics);
                    }
                    substitute_expr(&mut clause.body, &values, &const_functions, diagnostics);
                }
            }
            SyntaxDeclarationPayload::Method {
                receiver,
                params,
                clauses,
                ..
            } => {
                if let Some(default) = &mut receiver.default {
                    substitute_expr(default, &values, &const_functions, diagnostics);
                }
                substitute_params(params, &values, &const_functions, diagnostics);
                for clause in clauses {
                    if let Some(guard) = &mut clause.guard {
                        substitute_expr(guard, &values, &const_functions, diagnostics);
                    }
                    substitute_expr(&mut clause.body, &values, &const_functions, diagnostics);
                }
            }
            SyntaxDeclarationPayload::Trait {
                constants, methods, ..
            } => {
                for constant in constants {
                    if let Some(default) = &mut constant.default {
                        substitute_expr(default, &values, &const_functions, diagnostics);
                    }
                }
                for method in methods {
                    substitute_params(&mut method.params, &values, &const_functions, diagnostics);
                    if let Some(body) = &mut method.default_body {
                        substitute_expr(body, &values, &const_functions, diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::TraitImpl {
                constants, methods, ..
            } => {
                for constant in constants {
                    substitute_expr(&mut constant.value, &values, &const_functions, diagnostics);
                }
                for method in methods {
                    substitute_params(&mut method.params, &values, &const_functions, diagnostics);
                    for clause in &mut method.clauses {
                        if let Some(guard) = &mut clause.guard {
                            substitute_expr(guard, &values, &const_functions, diagnostics);
                        }
                        substitute_expr(&mut clause.body, &values, &const_functions, diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                for prop in props {
                    if let Some(default) = &mut prop.default {
                        substitute_expr(default, &values, &const_functions, diagnostics);
                    }
                }
            }
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }
}

pub(super) fn evaluate_declaration_defaults(
    payload: &mut SyntaxDeclarationPayload,
    evaluator: &mut Evaluator,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    let mut evaluate =
        |expr: &mut SyntaxExprOutput| match evaluator.evaluate_expr(expr, &HashMap::new()) {
            Ok(value) => *expr = value_to_expr(&value, expr.span),
            Err(error) => diagnostics.push(error),
        };
    match payload {
        SyntaxDeclarationPayload::Struct { fields, .. } => {
            for field in fields {
                if let Some(default) = &mut field.default {
                    evaluate(default);
                }
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &mut clause.params {
                    if let Some(default) = &mut param.default {
                        evaluate(default);
                    }
                }
            }
        }
        SyntaxDeclarationPayload::Function { params, .. }
        | SyntaxDeclarationPayload::Method { params, .. } => {
            for param in params {
                if let Some(default) = &mut param.default {
                    evaluate(default);
                }
            }
        }
        SyntaxDeclarationPayload::Trait {
            constants, methods, ..
        } => {
            for constant in constants {
                if let Some(default) = &mut constant.default {
                    evaluate(default);
                }
            }
            for method in methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        evaluate(default);
                    }
                }
            }
        }
        SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
            for method in methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        evaluate(default);
                    }
                }
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                if let Some(default) = &mut prop.default {
                    evaluate(default);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn substitute_params(
    params: &mut [crate::terlan_syntax::SyntaxParamOutput],
    values: &HashMap<String, ConstValue>,
    functions: &HashMap<(String, usize), ConstFunction>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    for param in params {
        if let Some(default) = &mut param.default {
            substitute_expr(default, values, functions, diagnostics);
        }
    }
}

pub(super) fn substitute_expr(
    expr: &mut SyntaxExprOutput,
    values: &HashMap<String, ConstValue>,
    functions: &HashMap<(String, usize), ConstFunction>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    if expr.kind == SyntaxExprKind::Var {
        if let Some(value) = expr.text.as_ref().and_then(|name| values.get(name)) {
            *expr = value_to_expr(value, expr.span);
            return;
        }
    }
    if expr.kind == SyntaxExprKind::FieldAccess {
        let qualified = qualified_expr_name(expr);
        if let Some(value) = qualified.as_ref().and_then(|name| values.get(name)) {
            *expr = value_to_expr(value, expr.span);
            return;
        }
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
        if name.is_some_and(|name| functions.contains_key(&(name.clone(), arity))) {
            diagnostics.push(diagnostic(
                "CONST_FUNCTION_RUNTIME_USE",
                "const functions may only be called from compile-time contexts",
                expr.span,
            ));
        }
    }
    for child in &mut expr.children {
        substitute_expr(child, values, functions, diagnostics);
    }
    for field in &mut expr.fields {
        substitute_expr(&mut field.value, values, functions, diagnostics);
    }
    for pattern in &mut expr.patterns {
        substitute_pattern(pattern, values);
    }
    for guard in expr.let_guards.iter_mut().flatten() {
        substitute_expr(guard, values, functions, diagnostics);
    }
    substitute_clauses(&mut expr.clauses, values, functions, diagnostics);
    substitute_clauses(&mut expr.catch_clauses, values, functions, diagnostics);
    if let Some(after) = &mut expr.try_after {
        substitute_expr(&mut after.trigger, values, functions, diagnostics);
        substitute_expr(&mut after.body, values, functions, diagnostics);
    }
}

pub(super) fn qualified_expr_name(expr: &SyntaxExprOutput) -> Option<String> {
    match expr.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => expr.text.clone(),
        SyntaxExprKind::FieldAccess => Some(format!(
            "{}.{}",
            qualified_expr_name(expr.children.first()?)?,
            expr.text.as_ref()?
        )),
        SyntaxExprKind::Index if expr.children.len() == 2 => Some(format!(
            "{}[{}]",
            qualified_expr_name(&expr.children[0])?,
            qualified_expr_name(&expr.children[1])?
        )),
        _ => None,
    }
}

pub(super) fn const_function_from_signature(
    signature: &crate::terlan_hir::ConstFunctionSignature,
) -> ConstFunction {
    ConstFunction {
        params: signature
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect(),
        return_type: signature.return_type.clone(),
        body: signature.body.clone(),
    }
}

pub(super) fn substitute_clauses(
    clauses: &mut [SyntaxClauseOutput],
    values: &HashMap<String, ConstValue>,
    functions: &HashMap<(String, usize), ConstFunction>,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    for clause in clauses {
        for pattern in &mut clause.patterns {
            substitute_pattern(pattern, values);
        }
        if let Some(guard) = &mut clause.guard {
            substitute_expr(guard, values, functions, diagnostics);
        }
        substitute_expr(&mut clause.body, values, functions, diagnostics);
    }
}

pub(super) fn substitute_pattern(
    pattern: &mut SyntaxPatternOutput,
    values: &HashMap<String, ConstValue>,
) {
    if pattern.kind == SyntaxPatternKind::Constructor && pattern.children.is_empty() {
        if let Some(value) = pattern.text.as_ref().and_then(|name| values.get(name)) {
            if let Some(replacement) = value_to_pattern(value) {
                *pattern = replacement;
                return;
            }
        }
    }
    for child in &mut pattern.children {
        substitute_pattern(child, values);
    }
    for field in &mut pattern.fields {
        substitute_pattern(&mut field.value, values);
    }
}

pub(super) fn value_to_expr(value: &ConstValue, span: EbnfSourceSpan) -> SyntaxExprOutput {
    let (kind, text, children, fields) = match value {
        ConstValue::Int(value) => (
            SyntaxExprKind::Int,
            Some(value.to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Float(bits) => (
            SyntaxExprKind::Float,
            Some(f64::from_bits(*bits).to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Bool(value) => (
            SyntaxExprKind::Atom,
            Some(value.to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Atom(value) => (
            SyntaxExprKind::Atom,
            Some(value.clone()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Binary(value) => (
            SyntaxExprKind::Binary,
            Some(value.clone()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Tuple(values) => (
            SyntaxExprKind::Tuple,
            None,
            values
                .iter()
                .map(|value| value_to_expr(value, span))
                .collect(),
            Vec::new(),
        ),
        ConstValue::List(values) => (
            SyntaxExprKind::List,
            None,
            values
                .iter()
                .map(|value| value_to_expr(value, span))
                .collect(),
            Vec::new(),
        ),
        ConstValue::FixedArray(values) => (
            SyntaxExprKind::FixedArray,
            None,
            values
                .iter()
                .map(|value| value_to_expr(value, span))
                .collect(),
            Vec::new(),
        ),
        ConstValue::Map(values) => (
            SyntaxExprKind::Map,
            None,
            Vec::new(),
            values
                .iter()
                .map(|(key, value)| SyntaxExprFieldOutput {
                    key: key.clone(),
                    required: true,
                    value: Box::new(value_to_expr(value, span)),
                })
                .collect(),
        ),
        ConstValue::Record { name, fields } => (
            SyntaxExprKind::RecordConstruct,
            Some(name.clone()),
            Vec::new(),
            fields
                .iter()
                .map(|(key, value)| SyntaxExprFieldOutput {
                    key: key.clone(),
                    required: true,
                    value: Box::new(value_to_expr(value, span)),
                })
                .collect(),
        ),
        ConstValue::Union {
            name,
            arm,
            representation,
        } => {
            let mut expr = value_to_expr(representation, span);
            expr.raw = Some(format!("const_union:{name}.{arm}"));
            return expr;
        }
    };
    SyntaxExprOutput {
        kind,
        arity: children.len(),
        text,
        span,
        raw: None,
        comprehension_lift: None,
        type_args: Vec::new(),
        operator: None,
        remote: None,
        arg_names: Vec::new(),
        children,
        patterns: Vec::new(),
        let_guards: Vec::new(),
        fields,
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

pub(super) fn literal_expr_value(expr: &SyntaxExprOutput) -> Option<ConstValue> {
    match expr.kind {
        SyntaxExprKind::Int => expr.text.as_deref()?.parse().ok().map(ConstValue::Int),
        SyntaxExprKind::Float => expr
            .text
            .as_deref()?
            .parse::<f64>()
            .ok()
            .map(|value| ConstValue::Float(value.to_bits())),
        SyntaxExprKind::Atom => match expr.text.as_deref()? {
            "true" => Some(ConstValue::Bool(true)),
            "false" => Some(ConstValue::Bool(false)),
            value => Some(ConstValue::Atom(value.to_string())),
        },
        SyntaxExprKind::Binary => Some(ConstValue::Binary(expr.text.clone()?)),
        SyntaxExprKind::Tuple => expr
            .children
            .iter()
            .map(literal_expr_value)
            .collect::<Option<Vec<_>>>()
            .map(ConstValue::Tuple),
        SyntaxExprKind::List => expr
            .children
            .iter()
            .map(literal_expr_value)
            .collect::<Option<Vec<_>>>()
            .map(ConstValue::List),
        SyntaxExprKind::FixedArray => expr
            .children
            .iter()
            .map(literal_expr_value)
            .collect::<Option<Vec<_>>>()
            .map(ConstValue::FixedArray),
        SyntaxExprKind::Map => expr
            .fields
            .iter()
            .map(|field| Some((field.key.clone(), literal_expr_value(&field.value)?)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(ConstValue::Map),
        SyntaxExprKind::RecordConstruct => expr
            .fields
            .iter()
            .map(|field| Some((field.key.clone(), literal_expr_value(&field.value)?)))
            .collect::<Option<BTreeMap<_, _>>>()
            .map(|fields| ConstValue::Record {
                name: expr.text.clone().unwrap_or_default(),
                fields,
            }),
        _ => None,
    }
}

pub(super) fn value_to_pattern(value: &ConstValue) -> Option<SyntaxPatternOutput> {
    let (kind, text, children, fields) = match value {
        ConstValue::Int(value) => (
            SyntaxPatternKind::Int,
            Some(value.to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Float(bits) => (
            SyntaxPatternKind::Float,
            Some(f64::from_bits(*bits).to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Bool(value) => (
            SyntaxPatternKind::Atom,
            Some(value.to_string()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Atom(value) => (
            SyntaxPatternKind::Atom,
            Some(value.clone()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Binary(value) => (
            SyntaxPatternKind::String,
            Some(value.clone()),
            Vec::new(),
            Vec::new(),
        ),
        ConstValue::Tuple(values) => (
            SyntaxPatternKind::Tuple,
            None,
            values
                .iter()
                .map(value_to_pattern)
                .collect::<Option<Vec<_>>>()?,
            Vec::new(),
        ),
        ConstValue::List(values) => (
            SyntaxPatternKind::List,
            None,
            values
                .iter()
                .map(value_to_pattern)
                .collect::<Option<Vec<_>>>()?,
            Vec::new(),
        ),
        ConstValue::Map(values) => (
            SyntaxPatternKind::Map,
            None,
            Vec::new(),
            values
                .iter()
                .map(|(key, value)| {
                    Some(SyntaxPatternFieldOutput {
                        key: key.clone(),
                        required: true,
                        value: Box::new(value_to_pattern(value)?),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        ConstValue::Record { name, fields } => (
            SyntaxPatternKind::Record,
            Some(name.clone()),
            Vec::new(),
            fields
                .iter()
                .map(|(key, value)| {
                    Some(SyntaxPatternFieldOutput {
                        key: key.clone(),
                        required: true,
                        value: Box::new(value_to_pattern(value)?),
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        ConstValue::Union {
            name,
            arm,
            representation,
        } => {
            return Some(SyntaxPatternOutput {
                kind: SyntaxPatternKind::Constructor,
                arity: 1,
                text: Some(format!("$const:{name}.{arm}")),
                children: vec![value_to_pattern(representation)?],
                fields: Vec::new(),
            });
        }
        ConstValue::FixedArray(_) => return None,
    };
    Some(SyntaxPatternOutput {
        kind,
        arity: children.len(),
        text,
        children,
        fields,
    })
}

pub(super) fn match_pattern(
    pattern: &SyntaxPatternOutput,
    value: &ConstValue,
    bindings: &mut HashMap<String, ConstValue>,
) -> bool {
    match pattern.kind {
        SyntaxPatternKind::Wildcard | SyntaxPatternKind::Ignore => true,
        SyntaxPatternKind::Var => {
            if let Some(name) = &pattern.text {
                bindings.insert(name.clone(), value.clone());
            }
            true
        }
        SyntaxPatternKind::Int => {
            matches!(value, ConstValue::Int(actual) if pattern.text.as_deref() == Some(&actual.to_string()))
        }
        SyntaxPatternKind::Float => {
            matches!(value, ConstValue::Float(actual) if pattern.text.as_deref() == Some(&f64::from_bits(*actual).to_string()))
        }
        SyntaxPatternKind::String => {
            matches!(value, ConstValue::Binary(actual) if pattern.text.as_ref() == Some(actual))
        }
        SyntaxPatternKind::Atom => match value {
            ConstValue::Bool(actual) => {
                pattern.text.as_deref() == Some(if *actual { "true" } else { "false" })
            }
            ConstValue::Atom(actual) => pattern.text.as_ref() == Some(actual),
            _ => false,
        },
        SyntaxPatternKind::Tuple => {
            sequence_pattern_matches(&pattern.children, value, bindings, false)
        }
        SyntaxPatternKind::List => {
            sequence_pattern_matches(&pattern.children, value, bindings, true)
        }
        _ => false,
    }
}

pub(super) fn sequence_pattern_matches(
    patterns: &[SyntaxPatternOutput],
    value: &ConstValue,
    bindings: &mut HashMap<String, ConstValue>,
    list: bool,
) -> bool {
    let values = match (list, value) {
        (true, ConstValue::List(values)) | (false, ConstValue::Tuple(values)) => values,
        _ => return false,
    };
    patterns.len() == values.len()
        && patterns
            .iter()
            .zip(values)
            .all(|(pattern, value)| match_pattern(pattern, value, bindings))
}

pub(super) fn ensure_type(
    expected: &str,
    value: &ConstValue,
    span: EbnfSourceSpan,
) -> Result<(), ValueLifecycleDiagnostic> {
    let expected = expected.trim();
    let matches = expected == value.type_name()
        || (expected.starts_with("List[") && matches!(value, ConstValue::List(_)))
        || (expected.starts_with("FixedArray[") && matches!(value, ConstValue::FixedArray(_)))
        || (expected.starts_with('{') && matches!(value, ConstValue::Tuple(_)))
        || (expected.starts_with("Map[") && matches!(value, ConstValue::Map(_)))
        || matches!(value, ConstValue::Record { name, .. } if name == expected);
    if matches {
        Ok(())
    } else {
        Err(diagnostic(
            "CONST_TYPE_MISMATCH",
            format!(
                "constant declared as `{expected}` evaluated to `{}`",
                value.type_name()
            ),
            span,
        ))
    }
}

pub(super) fn required_child(
    expr: &SyntaxExprOutput,
    index: usize,
) -> Result<&SyntaxExprOutput, ValueLifecycleDiagnostic> {
    expr.children.get(index).ok_or_else(|| {
        diagnostic(
            "CONST_INVALID_EXPRESSION",
            "malformed constant expression",
            expr.span,
        )
    })
}

pub(super) fn checked_int(
    value: Option<i64>,
    span: EbnfSourceSpan,
) -> Result<ConstValue, ValueLifecycleDiagnostic> {
    value
        .map(ConstValue::Int)
        .ok_or_else(|| diagnostic("CONST_OVERFLOW", "integer overflow", span))
}

pub(super) fn const_forbidden_form(kind: SyntaxExprKind) -> &'static str {
    match kind {
        SyntaxExprKind::RawMacro | SyntaxExprKind::Macro => "macro execution after expansion",
        SyntaxExprKind::IndexAssign | SyntaxExprKind::RecordUpdate => "mutation",
        SyntaxExprKind::Fun | SyntaxExprKind::RemoteFunRef => "closure or function identity",
        SyntaxExprKind::Try => "exception handling",
        SyntaxExprKind::HtmlBlock | SyntaxExprKind::TemplateInstantiate => {
            "asset or template execution"
        }
        _ => "runtime-dependent expression",
    }
}

pub(super) fn not_const(
    span: EbnfSourceSpan,
    form: impl std::fmt::Display,
) -> ValueLifecycleDiagnostic {
    diagnostic(
        "CONST_FORBIDDEN_EFFECT",
        format!(
            "constant evaluation cannot perform {form}; environment, files, clocks, randomness, networks, native resources, runtime configuration, and secrets are unavailable"
        ),
        span,
    )
}

pub(super) fn diagnostic(
    code: &'static str,
    message: impl Into<String>,
    span: EbnfSourceSpan,
) -> ValueLifecycleDiagnostic {
    ValueLifecycleDiagnostic {
        code,
        message: message.into(),
        span,
    }
}

pub(super) fn stable_fingerprint(text: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}
