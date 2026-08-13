use super::*;

#[derive(Clone, Debug)]
pub(super) struct ConstGenericParam {
    name: String,
    kind: String,
}

pub(super) fn resolve_const_generic_arguments(
    module: &mut SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
    evaluator: &mut Evaluator,
) -> Vec<ValueLifecycleDiagnostic> {
    let mut type_specs = HashMap::<String, Vec<Option<ConstGenericParam>>>::new();
    let mut callable_specs = HashMap::<(String, usize), Vec<Option<ConstGenericParam>>>::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type { name, params, .. }
            | SyntaxDeclarationPayload::Trait { name, params, .. } => {
                type_specs.insert(name.clone(), const_generic_params(params));
            }
            SyntaxDeclarationPayload::Struct {
                name,
                generic_params,
                ..
            } => {
                type_specs.insert(name.clone(), const_generic_params(generic_params));
            }
            SyntaxDeclarationPayload::Function {
                name,
                generic_params,
                params,
                ..
            } => {
                callable_specs.insert(
                    (name.clone(), params.len()),
                    const_generic_params(generic_params),
                );
            }
            _ => {}
        }
    }
    for (module_name, interface) in interfaces {
        for (name, params) in &interface.type_params {
            let spec = const_generic_params(params);
            type_specs.insert(format!("{module_name}.{name}"), spec.clone());
            type_specs.entry(name.clone()).or_insert(spec);
        }
        for (name, signature) in &interface.traits {
            let spec = const_generic_params(&signature.type_params);
            type_specs.insert(format!("{module_name}.{name}"), spec.clone());
            type_specs.entry(name.clone()).or_insert(spec);
        }
        for ((name, arity), signature) in &interface.functions {
            let spec = const_generic_params(&signature.generic_params);
            callable_specs.insert((format!("{module_name}.{name}"), *arity), spec.clone());
            callable_specs.entry((name.clone(), *arity)).or_insert(spec);
        }
    }

    let mut diagnostics = Vec::new();
    for declaration in &mut module.declarations {
        let in_scope = declaration_const_params(&declaration.payload);
        visit_declaration_type_texts_mut(&mut declaration.payload, |ty| {
            match rewrite_const_generic_type_text(
                &ty.text,
                &type_specs,
                &in_scope,
                evaluator,
                ty.span,
            ) {
                Ok(text) => ty.text = text,
                Err(error) => diagnostics.push(error),
            }
        });
        visit_declaration_exprs_mut(&mut declaration.payload, |expr| {
            resolve_const_generic_call_type_args(
                expr,
                &callable_specs,
                &in_scope,
                evaluator,
                &mut diagnostics,
            );
        });
    }
    diagnostics
}

pub(super) fn visit_declaration_exprs_mut(
    payload: &mut SyntaxDeclarationPayload,
    mut visit: impl FnMut(&mut SyntaxExprOutput),
) {
    fn walk(expr: &mut SyntaxExprOutput, visit: &mut dyn FnMut(&mut SyntaxExprOutput)) {
        visit(expr);
        for child in &mut expr.children {
            walk(child, visit);
        }
        for field in &mut expr.fields {
            walk(&mut field.value, visit);
        }
        for clause in expr.clauses.iter_mut().chain(&mut expr.catch_clauses) {
            if let Some(guard) = &mut clause.guard {
                walk(guard, visit);
            }
            walk(&mut clause.body, visit);
        }
    }
    match payload {
        SyntaxDeclarationPayload::Constant { value, .. }
        | SyntaxDeclarationPayload::ConstFunction { body: value, .. } => walk(value, &mut visit),
        SyntaxDeclarationPayload::Type { valued_arms, .. } => {
            for arm in valued_arms {
                walk(&mut arm.value, &mut visit);
            }
        }
        SyntaxDeclarationPayload::Struct { fields, .. } => {
            for field in fields {
                if let Some(default) = &mut field.default {
                    walk(default, &mut visit);
                }
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &mut clause.params {
                    if let Some(default) = &mut param.default {
                        walk(default, &mut visit);
                    }
                }
                walk(&mut clause.body, &mut visit);
            }
        }
        SyntaxDeclarationPayload::Function {
            params, clauses, ..
        }
        | SyntaxDeclarationPayload::Method {
            params, clauses, ..
        } => {
            for param in params {
                if let Some(default) = &mut param.default {
                    walk(default, &mut visit);
                }
            }
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    walk(guard, &mut visit);
                }
                walk(&mut clause.body, &mut visit);
            }
        }
        SyntaxDeclarationPayload::Trait {
            methods, constants, ..
        } => {
            for constant in constants {
                if let Some(default) = &mut constant.default {
                    walk(default, &mut visit);
                }
            }
            for method in methods {
                if let Some(body) = &mut method.default_body {
                    walk(body, &mut visit);
                }
            }
        }
        SyntaxDeclarationPayload::TraitImpl {
            methods, constants, ..
        } => {
            for constant in constants {
                walk(&mut constant.value, &mut visit);
            }
            for method in methods {
                for clause in &mut method.clauses {
                    if let Some(guard) = &mut clause.guard {
                        walk(guard, &mut visit);
                    }
                    walk(&mut clause.body, &mut visit);
                }
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                if let Some(default) = &mut prop.default {
                    walk(default, &mut visit);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn resolve_const_generic_call_type_args(
    expr: &mut SyntaxExprOutput,
    specs: &HashMap<(String, usize), Vec<Option<ConstGenericParam>>>,
    in_scope: &HashMap<String, String>,
    evaluator: &mut Evaluator,
    diagnostics: &mut Vec<ValueLifecycleDiagnostic>,
) {
    if !matches!(
        expr.kind,
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall
    ) || expr.type_args.is_empty()
    {
        return;
    }
    let Some(local_name) = expr.children.first().and_then(qualified_expr_name) else {
        return;
    };
    let name = expr.remote.as_ref().map_or(local_name.clone(), |module| {
        format!("{module}.{local_name}")
    });
    let arity = expr.children.len().saturating_sub(1);
    let Some(parameters) = specs.get(&(name, arity)) else {
        return;
    };
    if parameters.len() != expr.type_args.len() {
        diagnostics.push(diagnostic(
            "CONST_GENERIC_ARITY",
            format!(
                "generic callable expects {} type/value arguments, found {}",
                parameters.len(),
                expr.type_args.len()
            ),
            expr.span,
        ));
        return;
    }
    for (argument, parameter) in expr.type_args.iter_mut().zip(parameters) {
        let Some(parameter) = parameter else {
            continue;
        };
        match resolve_const_generic_argument(
            &argument.text,
            parameter,
            in_scope,
            evaluator,
            argument.span,
        ) {
            Ok(text) => argument.text = text,
            Err(error) => diagnostics.push(error),
        }
    }
}

pub(super) fn const_generic_params(params: &[String]) -> Vec<Option<ConstGenericParam>> {
    params
        .iter()
        .map(|param| {
            let rest = param.trim().strip_prefix("const ")?;
            let (name, kind) = rest.split_once(':')?;
            Some(ConstGenericParam {
                name: name.trim().to_string(),
                kind: kind.trim().to_string(),
            })
        })
        .collect()
}

pub(super) fn declaration_const_params(
    payload: &SyntaxDeclarationPayload,
) -> HashMap<String, String> {
    let params = match payload {
        SyntaxDeclarationPayload::Type { params, .. }
        | SyntaxDeclarationPayload::Trait { params, .. } => params,
        SyntaxDeclarationPayload::Struct { generic_params, .. }
        | SyntaxDeclarationPayload::Function { generic_params, .. }
        | SyntaxDeclarationPayload::Method { generic_params, .. }
        | SyntaxDeclarationPayload::TraitImpl { generic_params, .. } => generic_params,
        _ => return HashMap::new(),
    };
    const_generic_params(params)
        .into_iter()
        .flatten()
        .map(|param| (param.name, param.kind))
        .collect()
}

pub(super) fn visit_declaration_type_texts_mut(
    payload: &mut SyntaxDeclarationPayload,
    mut visit: impl FnMut(&mut crate::terlan_syntax::SyntaxTypeOutput),
) {
    match payload {
        SyntaxDeclarationPayload::Constant { annotation, .. } => visit(annotation),
        SyntaxDeclarationPayload::ConstFunction {
            params,
            return_type,
            ..
        }
        | SyntaxDeclarationPayload::Function {
            params,
            return_type,
            ..
        } => {
            for param in params {
                visit(&mut param.annotation);
            }
            visit(return_type);
        }
        SyntaxDeclarationPayload::Type {
            implements,
            variants,
            representation,
            ..
        } => {
            for ty in implements.iter_mut().chain(variants) {
                visit(ty);
            }
            if let Some(ty) = representation {
                visit(ty);
            }
        }
        SyntaxDeclarationPayload::Struct {
            implements, fields, ..
        } => {
            for ty in implements {
                visit(ty);
            }
            for field in fields {
                visit(&mut field.annotation);
            }
        }
        SyntaxDeclarationPayload::Constructor { clauses, .. } => {
            for clause in clauses {
                for param in &mut clause.params {
                    visit(&mut param.annotation);
                }
                visit(&mut clause.return_type);
            }
        }
        SyntaxDeclarationPayload::Method {
            receiver,
            params,
            return_type,
            ..
        } => {
            visit(&mut receiver.annotation);
            for param in params {
                visit(&mut param.annotation);
            }
            visit(return_type);
        }
        SyntaxDeclarationPayload::Trait {
            methods, constants, ..
        } => {
            for constant in constants {
                visit(&mut constant.annotation);
            }
            for method in methods {
                for param in &mut method.params {
                    visit(&mut param.annotation);
                }
                visit(&mut method.return_type);
            }
        }
        SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            methods,
            ..
        } => {
            visit(trait_ref);
            visit(for_type);
            for method in methods {
                for param in &mut method.params {
                    visit(&mut param.annotation);
                }
                visit(&mut method.return_type);
            }
        }
        SyntaxDeclarationPayload::Template { props, .. } => {
            for prop in props {
                visit(&mut prop.annotation);
            }
        }
        _ => {}
    }
}

pub(super) fn rewrite_const_generic_type_text(
    text: &str,
    specs: &HashMap<String, Vec<Option<ConstGenericParam>>>,
    in_scope: &HashMap<String, String>,
    evaluator: &mut Evaluator,
    span: EbnfSourceSpan,
) -> Result<String, ValueLifecycleDiagnostic> {
    let Some(open) = text.find('[') else {
        return Ok(text.to_string());
    };
    let Some(close) = matching_bracket(text, open) else {
        return Ok(text.to_string());
    };
    let owner_start = text[..open]
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .map_or(0, |index| index + 1);
    let owner = &text[owner_start..open];
    let mut args = split_top_level_const_args(&text[open + 1..close]);
    if let Some(spec) = specs.get(owner) {
        if spec.len() != args.len() {
            return Err(diagnostic(
                "CONST_GENERIC_ARITY",
                format!(
                    "const-generic type `{owner}` expects {} arguments, found {}",
                    spec.len(),
                    args.len()
                ),
                span,
            ));
        }
        for (arg, parameter) in args.iter_mut().zip(spec) {
            if let Some(parameter) = parameter {
                *arg = resolve_const_generic_argument(arg, parameter, in_scope, evaluator, span)?;
            } else {
                *arg = rewrite_const_generic_type_text(arg, specs, in_scope, evaluator, span)?;
            }
        }
    } else {
        for arg in &mut args {
            *arg = rewrite_const_generic_type_text(arg, specs, in_scope, evaluator, span)?;
        }
    }
    let rebuilt = format!(
        "{}[{}]{}",
        &text[..open],
        args.join(", "),
        &text[close + 1..]
    );
    if rebuilt[close.min(rebuilt.len())..].contains('[') {
        rewrite_const_generic_type_text(&rebuilt, specs, in_scope, evaluator, span)
    } else {
        Ok(rebuilt)
    }
}

pub(super) fn resolve_const_generic_argument(
    text: &str,
    parameter: &ConstGenericParam,
    in_scope: &HashMap<String, String>,
    evaluator: &mut Evaluator,
    span: EbnfSourceSpan,
) -> Result<String, ValueLifecycleDiagnostic> {
    let text = text.trim();
    if let Some(kind) = in_scope.get(text) {
        if kind == &parameter.kind {
            return Ok(text.to_string());
        }
        return Err(const_generic_kind_error(parameter, kind, span));
    }
    let source = format!(
        "module lifecycle.const_argument.\nconst ARG: {} = {}.\n",
        parameter.kind, text
    );
    let parsed = parse_module_as_syntax_output(&source).map_err(|_| {
        diagnostic(
            "INVALID_CONST_GENERIC_ARGUMENT",
            "const generic arguments accept literals, constants, const parameters, or const-function calls",
            span,
        )
    })?;
    let expression = parsed
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constant { value, .. } => Some(value),
            _ => None,
        })
        .ok_or_else(|| {
            diagnostic(
                "INVALID_CONST_GENERIC_ARGUMENT",
                "missing const generic argument",
                span,
            )
        })?;
    if !const_generic_argument_shape_is_allowed(expression, evaluator) {
        return Err(diagnostic(
            "INVALID_CONST_GENERIC_ARGUMENT",
            "const generic arguments accept literals, constants, const parameters, or const-function calls; inline arithmetic and runtime expressions are not supported",
            span,
        ));
    }
    let value = evaluator.evaluate_expr(expression, &HashMap::new())?;
    match (&parameter.kind[..], value) {
        ("Int", ConstValue::Int(value)) => Ok(value.to_string()),
        ("Bool", ConstValue::Bool(value)) => Ok(value.to_string()),
        ("Atom", ConstValue::Atom(value)) => Ok(format!("Atom[{value:?}]")),
        (_, value) => Err(const_generic_kind_error(parameter, value.type_name(), span)),
    }
}

pub(super) fn const_generic_argument_shape_is_allowed(
    expr: &SyntaxExprOutput,
    evaluator: &Evaluator,
) -> bool {
    match expr.kind {
        SyntaxExprKind::Int
        | SyntaxExprKind::Float
        | SyntaxExprKind::Atom
        | SyntaxExprKind::Binary => true,
        SyntaxExprKind::Var if matches!(expr.text.as_deref(), Some("true" | "false")) => true,
        SyntaxExprKind::Var | SyntaxExprKind::FieldAccess => {
            qualified_expr_name(expr).is_some_and(|name| {
                evaluator.values.contains_key(&name) || evaluator.definitions.contains_key(&name)
            })
        }
        SyntaxExprKind::Call | SyntaxExprKind::FunctionCall => {
            let Some(name) = expr.children.first().and_then(qualified_expr_name) else {
                return false;
            };
            evaluator
                .functions
                .contains_key(&(name, expr.children.len().saturating_sub(1)))
                && expr
                    .children
                    .iter()
                    .skip(1)
                    .all(|arg| const_generic_argument_shape_is_allowed(arg, evaluator))
        }
        _ => false,
    }
}

pub(super) fn const_generic_kind_error(
    parameter: &ConstGenericParam,
    actual: &str,
    span: EbnfSourceSpan,
) -> ValueLifecycleDiagnostic {
    diagnostic(
        "CONST_GENERIC_KIND_MISMATCH",
        format!(
            "const parameter `{}` requires `{}`, found `{actual}`",
            parameter.name, parameter.kind
        ),
        span,
    )
}

pub(super) fn matching_bracket(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn split_top_level_const_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut start = 0usize;
    let mut square = 0usize;
    let mut round = 0usize;
    let mut brace = 0usize;
    for (index, ch) in text.char_indices() {
        match ch {
            '[' => square += 1,
            ']' => square = square.saturating_sub(1),
            '(' => round += 1,
            ')' => round = round.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            ',' if square == 0 && round == 0 && brace == 0 => {
                args.push(text[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < text.len() || !text.trim().is_empty() {
        args.push(text[start..].trim().to_string());
    }
    args
}
