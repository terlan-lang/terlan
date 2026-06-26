use super::*;

/// Lowers an inline syntax HTML block into an Erlang iolist expression.
///
/// Inputs:
/// - `expr`: syntax-output expression containing parsed HTML nodes.
/// - `ctx`: syntax lowering context with template and type metadata.
/// - `env`: active lowering environment for locals and replacements.
///
/// Output:
/// - Erlang list expression containing static binaries and dynamic escaped
///   chunks, or `None` when a nested expression cannot be lowered.
///
/// Transformation:
/// - Lowers every parsed HTML node into iolist chunks and wraps the flattened
///   result in `ErlExpr::List`.
pub(super) fn lower_syntax_html_block_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let mut chunks = Vec::new();
    for node in &expr.html_nodes {
        chunks.extend(lower_syntax_html_node(node, ctx, env)?);
    }
    Some(ErlExpr::List(chunks))
}

/// Lowers one syntax HTML node into iolist chunks.
///
/// Inputs:
/// - `node`: parsed HTML node from syntax output.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - One or more Erlang iolist chunks for the node.
///
/// Transformation:
/// - Converts text to binaries, dynamic expressions to escaped values,
///   elements recursively, and named slots to their lowered child chunks.
fn lower_syntax_html_node(
    node: &SyntaxHtmlNodeOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    match node {
        SyntaxHtmlNodeOutput::Text { text } => Some(vec![html_binary(text)]),
        SyntaxHtmlNodeOutput::Expr { expr } => {
            Some(vec![lower_syntax_html_child_expr(expr, ctx, env)?])
        }
        SyntaxHtmlNodeOutput::Element { element } => lower_syntax_html_element(element, ctx, env),
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            let mut chunks = Vec::new();
            for child in &slot.children {
                chunks.extend(lower_syntax_html_node(child, ctx, env)?);
            }
            Some(chunks)
        }
    }
}

/// Lowers one parsed HTML element into opening tag, children, and closing tag.
///
/// Inputs:
/// - `element`: parsed syntax HTML element.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Iolist chunks for the complete element.
///
/// Transformation:
/// - Emits a partially static opening tag, lowers children recursively, and
///   appends a static closing tag binary.
fn lower_syntax_html_element(
    element: &SyntaxHtmlElementOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let mut chunks = Vec::new();
    chunks.extend(lower_syntax_html_open_tag(element, ctx, env)?);
    for child in &element.children {
        chunks.extend(lower_syntax_html_node(child, ctx, env)?);
    }
    chunks.push(html_binary(&format!("</{}>", element.name)));
    Some(chunks)
}

/// Lowers an HTML opening tag with static and dynamic attributes.
///
/// Inputs:
/// - `element`: parsed syntax HTML element.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Iolist chunks for the opening tag.
///
/// Transformation:
/// - Coalesces adjacent static attribute text into one binary and splits only
///   where dynamic attributes need runtime escaping.
fn lower_syntax_html_open_tag(
    element: &SyntaxHtmlElementOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let mut chunks = Vec::new();
    let mut static_text = format!("<{}", element.name);

    for attr in &element.attrs {
        if let Some(rendered) = render_static_syntax_html_attr(attr) {
            static_text.push_str(&rendered);
            continue;
        }

        if !static_text.is_empty() {
            chunks.push(html_binary(&static_text));
            static_text.clear();
        }
        chunks.extend(lower_dynamic_syntax_html_attr(attr, ctx, env)?);
    }

    static_text.push('>');
    chunks.push(html_binary(&static_text));
    Some(chunks)
}

/// Lowers one dynamic HTML attribute expression.
///
/// Inputs:
/// - `attr`: syntax HTML attribute with expression value.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Iolist chunks for the dynamic attribute, or an empty chunk set for
///   non-expression attributes.
///
/// Transformation:
/// - Wraps the lowered expression with `typer_html:escape/1` and static quote
///   delimiters.
fn lower_dynamic_syntax_html_attr(
    attr: &SyntaxHtmlAttrOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<Vec<ErlExpr>> {
    let Some(SyntaxHtmlAttrValueOutput::Expr { expr }) = &attr.value else {
        return Some(Vec::new());
    };

    Some(vec![
        html_binary(&format!(" {}=\"", attr.name)),
        ErlExpr::Call {
            module: Some("typer_html".to_string()),
            function: "escape".to_string(),
            args: vec![lower_syntax_expr_with_env(expr, ctx, env)?],
        },
        html_binary("\""),
    ])
}

/// Renders a syntax HTML attribute when its value is compile-time static.
///
/// Inputs:
/// - `attr`: parsed HTML attribute.
///
/// Output:
/// - Static attribute text, or `None` when runtime lowering is required.
///
/// Transformation:
/// - Emits boolean attributes, static text attributes, and selected static
///   expression forms such as string literals and class string lists.
fn render_static_syntax_html_attr(attr: &SyntaxHtmlAttrOutput) -> Option<String> {
    match &attr.value {
        None => Some(format!(" {}", attr.name)),
        Some(SyntaxHtmlAttrValueOutput::Text { text }) => {
            Some(format!(" {}=\"{}\"", attr.name, escape_html_attr(text)))
        }
        Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
            render_static_syntax_html_attr_expr(&attr.name, expr)
                .map(|value| format!(" {}=\"{}\"", attr.name, escape_html_attr(&value)))
        }
    }
}

/// Renders a static attribute expression into text.
///
/// Inputs:
/// - `name`: attribute name.
/// - `expr`: expression used as the attribute value.
///
/// Output:
/// - Static attribute value text, or `None` for dynamic expressions.
///
/// Transformation:
/// - Supports binary literals and `class` lists of binary literals so common
///   static markup stays compact in generated Erlang.
fn render_static_syntax_html_attr_expr(name: &str, expr: &SyntaxExprOutput) -> Option<String> {
    match (name, expr.kind) {
        ("class", SyntaxExprKind::List) => expr
            .children
            .iter()
            .map(|item| match item.kind {
                SyntaxExprKind::Binary => item.text.as_deref().map(static_html_attr_binary_text),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|items| items.join(" ")),
        (_, SyntaxExprKind::Binary) => expr.text.as_deref().map(static_html_attr_binary_text),
        _ => None,
    }
}

/// Lowers an expression used as an HTML child.
///
/// Inputs:
/// - `expr`: syntax expression inside an HTML child position.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Erlang expression that yields a child iolist or escaped value.
///
/// Transformation:
/// - Preserves list comprehensions and case expressions structurally, passes
///   raw/nested HTML through, and escapes ordinary expressions at runtime.
fn lower_syntax_html_child_expr(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if matches!(expr.kind, SyntaxExprKind::ListComprehension) {
        let value = expr.children.first()?;
        let source = expr.children.get(1)?;
        let pattern = expr.patterns.first()?;
        return Some(ErlExpr::ListComprehension {
            expr: Box::new(lower_syntax_html_child_expr(value, ctx, env)?),
            pattern: lower_syntax_pattern(pattern, ctx)?,
            source: Box::new(
                match lower_syntax_list_comprehension_source(source, ctx, env)? {
                    LoweredComprehensionSource::NativeList(source) => source,
                    LoweredComprehensionSource::IterableIterator(source) => source,
                },
            ),
            guard: match expr.children.get(2) {
                Some(guard) => Some(Box::new(lower_syntax_expr_with_env(guard, ctx, env)?)),
                None => None,
            },
        });
    }

    if matches!(expr.kind, SyntaxExprKind::Case) {
        let scrutinee = expr.children.first()?;
        return Some(ErlExpr::Case {
            scrutinee: Box::new(lower_syntax_expr_with_env(scrutinee, ctx, env)?),
            clauses: expr
                .clauses
                .iter()
                .map(|clause| {
                    let pattern = clause.patterns.first()?;
                    Some(ErlCaseClause {
                        pattern: lower_syntax_pattern(pattern, ctx)?,
                        guard: match clause.guard.as_deref() {
                            Some(guard) => Some(lower_syntax_expr_with_env(guard, ctx, env)?),
                            None => None,
                        },
                        body: lower_syntax_html_child_expr(&clause.body, ctx, env)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        });
    }

    if matches!(expr.kind, SyntaxExprKind::HtmlBlock) {
        return lower_syntax_expr_with_env(expr, ctx, env);
    }

    if let Some(raw) = lower_syntax_html_raw_expr_with_env(expr, ctx, env) {
        return Some(raw);
    }

    Some(ErlExpr::Call {
        module: Some("typer_html".to_string()),
        function: "escape".to_string(),
        args: vec![lower_syntax_expr_with_env(expr, ctx, env)?],
    })
}

/// Lowers a template instantiation expression.
///
/// Inputs:
/// - `expr`: syntax-output template instantiation expression.
/// - `ctx`: syntax lowering context containing loaded templates.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Erlang iolist expression for the instantiated template.
///
/// Transformation:
/// - Lowers field values into a slot-value map, then lowers each parsed
///   template node with slot substitution.
pub(super) fn lower_syntax_template_instantiation(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let name = expr.text.as_deref()?;
    let template = ctx.templates.get(name)?;
    let values = expr
        .fields
        .iter()
        .map(|field| {
            Some((
                field.key.clone(),
                lower_syntax_html_raw_expr_with_env(&field.value, ctx, env)
                    .or_else(|| lower_syntax_expr_with_env(&field.value, ctx, env))?,
            ))
        })
        .collect::<Option<BTreeMap<_, _>>>()?;
    let values = lower_syntax_template_default_values(values, template, ctx, env)?;

    Some(ErlExpr::List(
        template
            .nodes
            .iter()
            .flat_map(|node| lower_syntax_template_node(node, &values, template, ctx))
            .collect(),
    ))
}

/// Lowers a generated template function call such as `Page(title = value)`.
///
/// Inputs:
/// - `name`: template declaration name used as the call head.
/// - `args`: positional or named call arguments.
/// - `arg_names`: optional source names parallel to `args`.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang iolist expression for the rendered template, or `None` when the
///   call does not match a declared template function.
///
/// Transformation:
/// - Maps call arguments onto template properties using declaration order,
///   fills omitted defaulted properties, and delegates to the same parsed-node
///   rendering path used by `Page{ ... }` template instantiation.
pub(super) fn lower_syntax_template_call(
    name: &str,
    args: &[SyntaxExprOutput],
    arg_names: &[Option<String>],
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let template = ctx.templates.get(name)?;
    if args.len() > template.prop_order.len() {
        return None;
    }

    let mut values = BTreeMap::new();
    if arg_names.iter().any(Option::is_some) {
        for (index, arg) in args.iter().enumerate() {
            let key = match arg_names.get(index).and_then(Option::as_ref) {
                Some(name) => name.clone(),
                None => template.prop_order.get(index)?.clone(),
            };
            if !template.props.contains_key(&key) {
                return None;
            }
            values.insert(key, lower_syntax_template_arg(arg, ctx, env)?);
        }
    } else {
        for (index, arg) in args.iter().enumerate() {
            values.insert(
                template.prop_order.get(index)?.clone(),
                lower_syntax_template_arg(arg, ctx, env)?,
            );
        }
    }
    let values = lower_syntax_template_default_values(values, template, ctx, env)?;
    if template
        .prop_order
        .iter()
        .any(|name| !values.contains_key(name))
    {
        return None;
    }

    Some(lower_syntax_template_values(template, &values, ctx))
}

/// Lowers one template call argument in a slot-preserving way.
///
/// Inputs:
/// - `arg`: source expression supplied to a template property.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Erlang expression to bind to the property.
///
/// Transformation:
/// - Preserves `Html.raw(...)` values as trusted HTML and otherwise uses the
///   ordinary syntax expression lowerer.
fn lower_syntax_template_arg(
    arg: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    lower_syntax_html_raw_expr_with_env(arg, ctx, env)
        .or_else(|| lower_syntax_expr_with_env(arg, ctx, env))
}

/// Renders parsed template nodes from already-lowered property values.
///
/// Inputs:
/// - `template`: parsed template body and property metadata.
/// - `values`: lowered values keyed by property name.
/// - `ctx`: syntax lowering context for nested field access.
///
/// Output:
/// - Erlang iolist expression for the rendered template.
///
/// Transformation:
/// - Reuses the node renderer shared by constructor-like template
///   instantiation and generated template function calls.
fn lower_syntax_template_values(
    template: &LowerTemplate,
    values: &BTreeMap<String, ErlExpr>,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    ErlExpr::List(
        template
            .nodes
            .iter()
            .flat_map(|node| lower_syntax_template_node(node, values, template, ctx))
            .collect(),
    )
}

/// Fills omitted template properties from declaration defaults.
///
/// Inputs:
/// - `values`: lowered values supplied by the template instantiation.
/// - `template`: template metadata including optional default expressions.
/// - `ctx`, `env`: active syntax lowering context and lexical environment.
///
/// Output:
/// - Completed slot-value map containing source values plus lowered defaults
///   for omitted defaulted properties.
///
/// Transformation:
/// - Leaves supplied properties unchanged and lowers only missing properties
///   that declare defaults in the template signature.
fn lower_syntax_template_default_values(
    mut values: BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<BTreeMap<String, ErlExpr>> {
    for (name, prop) in &template.props {
        if values.contains_key(name) {
            continue;
        }
        let Some(default) = &prop.default else {
            continue;
        };
        let lowered = lower_syntax_html_raw_expr_with_env(default, ctx, env)
            .or_else(|| lower_syntax_expr_with_env(default, ctx, env))?;
        values.insert(name.clone(), lowered);
    }
    Some(values)
}

/// Lowers one parsed template node.
///
/// Inputs:
/// - `node`: parsed template node.
/// - `values`: lowered values supplied by the instantiation expression.
/// - `template`: template metadata and property types.
/// - `ctx`: syntax lowering context.
///
/// Output:
/// - Iolist chunks for the template node.
///
/// Transformation:
/// - Converts text/comment/doctype to static binaries, slots to substituted
///   values, and elements recursively.
fn lower_syntax_template_node(
    node: &terlan_html::HtmlNode,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> Vec<ErlExpr> {
    match node {
        terlan_html::HtmlNode::Text(text) => vec![html_binary(text)],
        terlan_html::HtmlNode::Comment(text) => vec![html_binary(&format!("<!--{}-->", text))],
        terlan_html::HtmlNode::Doctype(text) => {
            vec![html_binary(&format!("<!DOCTYPE {}>", text))]
        }
        terlan_html::HtmlNode::Slot(slot) => {
            vec![lower_syntax_template_slot_text(slot, values, template, ctx)]
        }
        terlan_html::HtmlNode::Element(element) => {
            let mut chunks = Vec::new();
            chunks.extend(lower_syntax_template_open_tag(
                element, values, template, ctx,
            ));
            for child in &element.children {
                chunks.extend(lower_syntax_template_node(child, values, template, ctx));
            }
            chunks.push(html_binary(&format!("</{}>", element.name)));
            chunks
        }
    }
}

/// Lowers a parsed template opening tag.
///
/// Inputs:
/// - `element`: parsed template element.
/// - `values`: lowered slot values.
/// - `template`: template metadata and property types.
/// - `ctx`: syntax lowering context.
///
/// Output:
/// - Iolist chunks for the opening tag.
///
/// Transformation:
/// - Emits static attributes directly and slot attributes through escaped
///   runtime expressions.
fn lower_syntax_template_open_tag(
    element: &terlan_html::HtmlElement,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> Vec<ErlExpr> {
    let mut chunks = Vec::new();
    let mut static_text = format!("<{}", element.name);

    for attr in &element.attrs {
        match &attr.value {
            None => static_text.push_str(&format!(" {}", attr.name)),
            Some(terlan_html::HtmlAttrValue::Text(value)) => {
                static_text.push_str(&format!(" {}=\"{}\"", attr.name, escape_html_attr(value)))
            }
            Some(terlan_html::HtmlAttrValue::Slot(slot)) => {
                if !static_text.is_empty() {
                    chunks.push(html_binary(&static_text));
                    static_text.clear();
                }
                chunks.push(html_binary(&format!(" {}=\"", attr.name)));
                chunks.push(lower_syntax_template_slot_escape(
                    slot, values, template, ctx,
                ));
                chunks.push(html_binary("\""));
            }
        }
    }

    static_text.push('>');
    chunks.push(html_binary(&static_text));
    chunks
}

/// Lowers one template slot value as escaped HTML text.
///
/// Inputs:
/// - `slot`: parsed slot path.
/// - `values`: lowered root slot values.
/// - `template`: template metadata and property types.
/// - `ctx`: syntax lowering context.
///
/// Output:
/// - Erlang expression that escapes the slot value at runtime.
///
/// Transformation:
/// - Resolves the slot value expression and wraps it in `typer_html:escape/1`.
fn lower_syntax_template_slot_escape(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    let value = lower_syntax_template_slot_value(slot, values, template, ctx);
    ErlExpr::Call {
        module: Some("typer_html".to_string()),
        function: "escape".to_string(),
        args: vec![value],
    }
}

/// Lowers one template slot for text position.
///
/// Inputs:
/// - `slot`: parsed slot path.
/// - `values`: lowered root slot values.
/// - `template`: template metadata and property types.
/// - `ctx`: syntax lowering context.
///
/// Output:
/// - Erlang expression for the slot value.
///
/// Transformation:
/// - Allows slots typed as `Html` to pass through unescaped and escapes all
///   ordinary values.
fn lower_syntax_template_slot_text(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    if slot.path.len() == 1
        && slot
            .path
            .first()
            .and_then(|root| template.props.get(root))
            .is_some_and(|prop| is_template_html_type(&prop.type_text))
    {
        return lower_syntax_template_slot_value(slot, values, template, ctx);
    }

    lower_syntax_template_slot_escape(slot, values, template, ctx)
}

/// Resolves a template slot path into an Erlang expression.
///
/// Inputs:
/// - `slot`: parsed slot path.
/// - `values`: lowered root slot values.
/// - `template`: template metadata and property types.
/// - `ctx`: syntax lowering context.
///
/// Output:
/// - Erlang expression for the root value or nested record field.
///
/// Transformation:
/// - Starts from the root slot value and follows path segments through known
///   struct field metadata, emitting Erlang record access for each segment.
fn lower_syntax_template_slot_value(
    slot: &terlan_html::HtmlSlot,
    values: &BTreeMap<String, ErlExpr>,
    template: &LowerTemplate,
    ctx: &SyntaxLowerCtx,
) -> ErlExpr {
    let Some(root) = slot.path.first() else {
        return html_binary("");
    };
    let mut value = values
        .get(root)
        .cloned()
        .unwrap_or_else(|| ErlExpr::Atom("undefined".to_string()));
    let mut current_type = template
        .props
        .get(root)
        .and_then(|prop| simple_template_type_name(&prop.type_text))
        .map(str::to_string);

    for field in slot.path.iter().skip(1) {
        let Some(record_name) = current_type.clone() else {
            break;
        };
        value = ErlExpr::RecordAccess {
            value: Box::new(value),
            name: record_name.clone(),
            field: field.clone(),
        };
        current_type = ctx
            .struct_field_types
            .get(&record_name)
            .and_then(|fields| fields.get(field))
            .and_then(|type_text| simple_template_type_name(type_text))
            .map(str::to_string);
    }

    value
}

/// Lowers `Html.raw(value)` without escaping.
///
/// Inputs:
/// - `expr`: syntax-output call expression.
/// - `ctx`: syntax lowering context.
/// - `env`: active expression lowering environment.
///
/// Output:
/// - Lowered trusted expression for `Html.raw(value)`, or `None` for other
///   call shapes.
///
/// Transformation:
/// - Recognizes the canonical raw HTML helper and lowers only its trusted value
///   argument, bypassing the ordinary child escaping path.
pub(super) fn lower_syntax_html_raw_expr_with_env(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let SyntaxExprKind::Call = expr.kind else {
        return None;
    };
    let callee = expr.children.first()?;
    if expr.remote.as_deref() != Some("Html") || syntax_expr_name(callee)? != "raw" {
        return None;
    }
    let [trusted] = &expr.children.get(1..)? else {
        return None;
    };
    lower_syntax_expr_with_env(trusted, ctx, env)
}
