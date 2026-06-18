use super::*;

/// Local explicit constructor target.
///
/// Inputs:
/// - Constructor declaration selected by source name and arity.
///
/// Output:
/// - Generated Erlang function name, arity/default metadata, and varargs
///   shape used by local constructor call lowering.
///
/// Transformation:
/// - Converts Terlan constructor clauses into backend call metadata while
///   preserving default argument and varargs behavior.
#[derive(Debug, Clone)]
pub(super) struct SyntaxConstructorTarget {
    pub(super) function: String,
    pub(super) fixed_arity: usize,
    pub(super) min_arity: usize,
    pub(super) defaults: Vec<Option<SyntaxExprOutput>>,
    pub(super) varargs: bool,
}

/// Imported or remote constructor target.
///
/// Inputs:
/// - Provider module interface constructor signature.
///
/// Output:
/// - Erlang module/function identity plus arity shape.
///
/// Transformation:
/// - Stores the remote constructor ABI selected from imported interface
///   metadata so call lowering can emit remote Erlang calls.
#[derive(Debug, Clone)]
pub(super) struct SyntaxRemoteConstructorTarget {
    pub(super) module: String,
    pub(super) function: String,
    pub(super) fixed_arity: usize,
    pub(super) varargs: bool,
}

impl SyntaxRemoteConstructorTarget {
    /// Checks whether a remote constructor can accept a source-visible arity.
    ///
    /// Inputs:
    /// - `arity`: number of arguments supplied at the call site.
    ///
    /// Output:
    /// - `true` when the target accepts the arity.
    /// - `false` when the fixed or minimum varargs arity does not match.
    ///
    /// Transformation:
    /// - Treats fixed remote constructors as exact-arity calls and varargs
    ///   constructors as accepting the fixed head plus any number of rest args.
    pub(super) fn accepts_arity(&self, arity: usize) -> bool {
        if self.varargs {
            arity >= self.fixed_arity
        } else {
            arity == self.fixed_arity
        }
    }
}

/// Constructor-pattern target for deconstruction lowering.
///
/// Inputs:
/// - Constructor-like source shape selected for pattern position.
///
/// Output:
/// - Parameter names and body template used to lower constructor patterns.
///
/// Transformation:
/// - Preserves alias or constructor deconstruction metadata so pattern lowering
///   can match the generated runtime representation.
#[derive(Debug, Clone)]
pub(super) struct SyntaxConstructorPatternTarget {
    pub(super) params: Vec<String>,
    pub(super) body: SyntaxExprOutput,
}

/// Single-shape type alias constructor metadata.
///
/// Inputs:
/// - Type alias variants parsed from syntax output.
///
/// Output:
/// - Constructor parameter names and template expression body.
///
/// Transformation:
/// - Converts eligible singleton atom or tagged tuple aliases into the
///   constructor-like metadata used by expression and pattern lowering.
#[derive(Debug, Clone)]
pub(super) struct SyntaxAliasConstructorTarget {
    pub(super) params: Vec<String>,
    pub(super) body: SyntaxExprOutput,
}

/// Builds a remote constructor target from an imported constructor signature.
///
/// Inputs:
/// - `module_name`: resolved source module that owns the constructor.
/// - `name`: source constructor name.
/// - `signature`: interface constructor signature from HIR metadata.
///
/// Output:
/// - Remote constructor target carrying Erlang module/function identity and
///   arity shape.
///
/// Transformation:
/// - Converts source constructor identity into the backend constructor function
///   name while preserving fixed and varargs arity metadata for call routing.
pub(super) fn syntax_remote_constructor_target_from_signature(
    module_name: &str,
    name: &str,
    signature: &terlan_hir::ConstructorSignature,
) -> SyntaxRemoteConstructorTarget {
    let fixed_arity = signature.params.len();
    SyntaxRemoteConstructorTarget {
        module: module_name.to_string(),
        function: constructor_function_name(name, fixed_arity, signature.varargs),
        fixed_arity,
        varargs: signature.varargs,
    }
}

/// Parses a single-shape type alias into constructor metadata.
///
/// Inputs:
/// - `variants`: type-alias variant text from syntax output.
///
/// Output:
/// - Alias constructor target for singleton atom aliases or tagged tuple aliases.
/// - `None` for unions, unsupported shapes, or invalid parameter labels.
///
/// Transformation:
/// - Accepts only closed single-shape aliases. Atom aliases become nullary
///   values, while tagged tuple aliases turn named fields into constructor
///   parameters and keep a syntax-output body template for later lowering.
pub(super) fn parse_syntax_type_alias_constructor_target_texts(
    variants: &[String],
) -> Option<SyntaxAliasConstructorTarget> {
    if variants.len() != 1 {
        return None;
    }
    let src = compact_type_application(&compact_spaces(&variants[0]));
    if is_union(&src) {
        return None;
    }
    if let Some(atom) = parse_type_atom_literal(&src) {
        return Some(SyntaxAliasConstructorTarget {
            params: Vec::new(),
            body: syntax_alias_expr_leaf(SyntaxExprKind::Atom, atom),
        });
    }
    if !(src.starts_with('{') && src.ends_with('}')) {
        return None;
    }

    let inner = &src[1..src.len() - 1];
    let mut items = split_top_level_csv(inner).into_iter();
    let tag = parse_type_atom_literal(&items.next()?)?;
    let mut params = Vec::new();
    let mut body_items = vec![syntax_alias_expr_leaf(SyntaxExprKind::Atom, tag)];

    for item in items {
        let (label, _ty) = split_named_tuple_type_elem(&item)?;
        if !is_lower_identifier(label) {
            return None;
        }
        params.push(label.to_string());
        body_items.push(syntax_alias_expr_leaf(
            SyntaxExprKind::Var,
            label.to_string(),
        ));
    }

    Some(SyntaxAliasConstructorTarget {
        params,
        body: syntax_alias_expr_tuple(body_items),
    })
}

/// Creates a leaf syntax-expression template for an alias constructor body.
///
/// Inputs:
/// - `kind`: formal syntax expression kind for the leaf.
/// - `text`: source text carried by the leaf.
///
/// Output:
/// - Syntax-expression output node with no children and default span metadata.
///
/// Transformation:
/// - Builds a minimal formal syntax-output node used only as an internal
///   lowering template for type-alias constructor bodies.
fn syntax_alias_expr_leaf(kind: SyntaxExprKind, text: String) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind,
        arity: 0,
        text: Some(text),
        span: Default::default(),
        raw: None,
        operator: None,
        remote: None,
        children: Vec::new(),
        patterns: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

/// Creates a tuple syntax-expression template for an alias constructor body.
///
/// Inputs:
/// - `children`: tuple element expression templates.
///
/// Output:
/// - Syntax-expression output node representing a tuple with those children.
///
/// Transformation:
/// - Wraps prebuilt alias body leaf templates in a minimal tuple node so alias
///   constructor lowering can reuse ordinary syntax-expression recursion.
fn syntax_alias_expr_tuple(children: Vec<SyntaxExprOutput>) -> SyntaxExprOutput {
    SyntaxExprOutput {
        kind: SyntaxExprKind::Tuple,
        arity: children.len(),
        text: None,
        span: Default::default(),
        raw: None,
        operator: None,
        remote: None,
        children,
        patterns: Vec::new(),
        fields: Vec::new(),
        clauses: Vec::new(),
        catch_clauses: Vec::new(),
        try_after: None,
        html_nodes: Vec::new(),
    }
}

/// Lowers a constructor declaration into Erlang spec and function forms.
///
/// Inputs:
/// - `name`: source constructor owner/type name.
/// - `clauses`: constructor clauses from formal syntax output.
/// - `ctx`: active syntax lowering context.
///
/// Output:
/// - Erlang forms for every constructor clause.
/// - `None` when a constructor body cannot lower.
///
/// Transformation:
/// - Converts each source constructor clause into its generated Erlang
///   constructor function, including varargs list specs and receiver-local
///   parameter environment for body lowering.
pub(super) fn lower_syntax_constructor_decl(
    name: &str,
    clauses: &[SyntaxConstructorClauseOutput],
    ctx: &SyntaxLowerCtx,
) -> Option<Vec<ErlForm>> {
    clauses
        .iter()
        .map(|clause| {
            let env = lower_syntax_constructor_clause_env(&clause.params, ctx);
            let fixed_arity = clause
                .params
                .iter()
                .filter(|param| !param.is_varargs)
                .count();
            let varargs = clause.params.iter().any(|param| param.is_varargs);
            let function = constructor_function_name(name, fixed_arity, varargs);
            let args = clause
                .params
                .iter()
                .map(|param| {
                    if param.is_varargs {
                        ErlType::List(Box::new(lower_syntax_type_to_spec(
                            &param.annotation.text,
                            ctx,
                        )))
                    } else {
                        lower_syntax_type_to_spec(&param.annotation.text, ctx)
                    }
                })
                .collect();
            Some(vec![
                ErlForm::Spec(ErlSpec {
                    docs: Vec::new(),
                    name: function.clone(),
                    args,
                    ret: lower_syntax_type_to_spec(&clause.return_type.text, ctx),
                }),
                ErlForm::Function(ErlFunction {
                    docs: Vec::new(),
                    name: function,
                    clauses: vec![ErlFunctionClause {
                        patterns: clause
                            .params
                            .iter()
                            .map(|param| ErlPattern::Var(sanitize_erlang_var(&param.name)))
                            .collect(),
                        guard: None,
                        body: lower_syntax_expr_with_env(&clause.body, ctx, &env)?,
                    }],
                }),
            ])
        })
        .collect::<Option<Vec<_>>>()
        .map(|forms| forms.into_iter().flatten().collect())
}

/// Lowers a local explicit constructor call.
///
/// Inputs:
/// - `target`: local constructor metadata selected by source name and arity.
/// - `args`: source-visible constructor arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang call to the generated local constructor function.
/// - `None` when any argument or default expression cannot lower.
///
/// Transformation:
/// - Lowers fixed arguments directly, fills omitted default arguments for
///   defaulted constructors, and packages varargs rest arguments into the final
///   Erlang list parameter expected by the generated constructor function.
pub(super) fn lower_syntax_explicit_constructor_call(
    target: &SyntaxConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_args = if target.varargs {
        let mut lowered = args
            .iter()
            .take(target.fixed_arity)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        lowered.push(ErlExpr::List(
            args.iter()
                .skip(target.fixed_arity)
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        ));
        lowered
    } else {
        let mut lowered = args
            .iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        for default in target.defaults.iter().skip(args.len()).flatten() {
            lowered.push(lower_syntax_expr_with_env(default, ctx, env)?);
        }
        lowered
    };

    Some(ErlExpr::Call {
        module: None,
        function: target.function.clone(),
        args: lowered_args,
    })
}

/// Lowers an imported or remote constructor call.
///
/// Inputs:
/// - `target`: remote constructor metadata selected by module, name, and arity.
/// - `args`: source-visible constructor arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang remote call to the generated constructor function.
/// - `None` when any argument cannot lower.
///
/// Transformation:
/// - Lowers fixed arguments directly and packages varargs rest arguments into
///   the final Erlang list parameter expected by the remote constructor ABI.
pub(super) fn lower_syntax_remote_constructor_call(
    target: &SyntaxRemoteConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let lowered_args = if target.varargs {
        let mut lowered = args
            .iter()
            .take(target.fixed_arity)
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?;
        lowered.push(ErlExpr::List(
            args.iter()
                .skip(target.fixed_arity)
                .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        ));
        lowered
    } else {
        args.iter()
            .map(|arg| lower_syntax_expr_with_env(arg, ctx, env))
            .collect::<Option<Vec<_>>>()?
    };

    Some(ErlExpr::Call {
        module: Some(target.module.clone()),
        function: target.function.clone(),
        args: lowered_args,
    })
}

/// Lowers an alias constructor call or singleton alias value.
///
/// Inputs:
/// - `target`: alias constructor metadata selected from a single-shape type
///   alias.
/// - `args`: source-visible constructor arguments.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression representing the alias runtime shape.
/// - `None` when the alias template cannot lower.
///
/// Transformation:
/// - Binds source constructor arguments by alias field name, then recursively
///   lowers the stored alias body template into runtime atoms, tuples, lists,
///   cons cells, variables, or substituted argument expressions.
pub(super) fn lower_syntax_alias_constructor_expr(
    target: &SyntaxAliasConstructorTarget,
    args: &[SyntaxExprOutput],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let bindings = target
        .params
        .iter()
        .cloned()
        .zip(args.iter())
        .collect::<BTreeMap<_, _>>();
    syntax_expr_to_alias_constructor_expr(&target.body, &bindings, ctx, env)
}

/// Converts an alias constructor template node into an Erlang expression.
///
/// Inputs:
/// - `expr`: alias body syntax-expression template.
/// - `bindings`: source argument expressions keyed by alias parameter name.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression matching the alias runtime shape.
/// - `None` when the template shape is unsupported or a nested expression
///   cannot lower.
///
/// Transformation:
/// - Recurses through the small set of syntax-output nodes produced by alias
///   target parsing and substitutes bound constructor arguments at matching
///   variable leaves.
fn syntax_expr_to_alias_constructor_expr(
    expr: &SyntaxExprOutput,
    bindings: &BTreeMap<String, &SyntaxExprOutput>,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    match expr.kind {
        SyntaxExprKind::Atom => Some(ErlExpr::Atom(expr.text.clone()?)),
        SyntaxExprKind::Var => {
            let name = expr.text.as_deref()?;
            bindings
                .get(name)
                .and_then(|expr| lower_syntax_expr_with_env(expr, ctx, env))
                .or_else(|| Some(ErlExpr::Var(sanitize_erlang_var(name))))
        }
        SyntaxExprKind::Int => Some(ErlExpr::Int(expr.text.as_deref()?.parse().ok()?)),
        SyntaxExprKind::Float => Some(ErlExpr::Float(expr.text.clone()?)),
        SyntaxExprKind::Binary => Some(ErlExpr::Binary(expr.text.clone()?)),
        SyntaxExprKind::Tuple => Some(ErlExpr::Tuple(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_alias_constructor_expr(item, bindings, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::List => Some(ErlExpr::List(
            expr.children
                .iter()
                .map(|item| syntax_expr_to_alias_constructor_expr(item, bindings, ctx, env))
                .collect::<Option<Vec<_>>>()?,
        )),
        SyntaxExprKind::ListCons => Some(ErlExpr::ListCons(
            Box::new(syntax_expr_to_alias_constructor_expr(
                expr.children.first()?,
                bindings,
                ctx,
                env,
            )?),
            Box::new(syntax_expr_to_alias_constructor_expr(
                expr.children.get(1)?,
                bindings,
                ctx,
                env,
            )?),
        )),
        _ => None,
    }
}
