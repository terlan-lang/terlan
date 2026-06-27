use std::collections::HashMap;

use crate::terlan_hir::{identifier_to_snake, FunctionSymbol};
use crate::terlan_syntax::{
    SyntaxExprKind, SyntaxExprOutput, SyntaxHtmlAttrOutput, SyntaxHtmlAttrValueOutput,
    SyntaxHtmlElementOutput, SyntaxHtmlNodeOutput,
};

use super::{
    expand_type_aliases, infer_function_scheme_overload, infer_syntax_expr, pretty_type,
    ExprInferContext, Type, TypeAlias, TypeVarId,
};

/// Infers the type of an HTML block expression.
///
/// Inputs:
/// - `expr`: syntax-output HTML block expression.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - `Html[Dynamic]`, the current syntax-level HTML value shape.
///
/// Transformation:
/// - Walks every top-level HTML node, validates child expressions,
///   component calls, and known attribute types, and leaves diagnostics in
///   `errors` while preserving the HTML block as a typed expression value.
pub(super) fn infer_syntax_html_block(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for node in &expr.html_nodes {
        check_syntax_html_node(node, locals, ctx, subst, errors);
    }
    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}

/// Checks one HTML node for type errors.
///
/// Inputs:
/// - `node`: syntax-output HTML node.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - No direct value. Any failures are appended to `errors`.
///
/// Transformation:
/// - Recursively checks ordinary elements, named slots, embedded
///   expressions, and component-style uppercase elements.
fn check_syntax_html_node(
    node: &SyntaxHtmlNodeOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => {}
        SyntaxHtmlNodeOutput::Expr { expr } => {
            let child_type = infer_syntax_html_child_expr(expr, locals, ctx, subst, errors);
            if !is_renderable_html_child_type(&child_type, ctx.aliases) {
                errors.push("expression is not renderable as HTML".to_string());
            }
        }
        SyntaxHtmlNodeOutput::Element { element } => {
            if is_component_element_name(&element.name) {
                let component_type =
                    infer_syntax_html_component_call(element, locals, ctx, subst, errors);

                if !is_html_expression_type(&component_type, ctx.aliases) {
                    errors.push(format!(
                        "component `{}` must return Html[Msg], found {}",
                        element.name,
                        pretty_type(&component_type)
                    ));
                }
                return;
            }
            for attr in &element.attrs {
                check_syntax_html_attr(attr, locals, ctx, subst, errors);
            }
            for child in &element.children {
                check_syntax_html_node(child, locals, ctx, subst, errors);
            }
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            for child in &slot.children {
                check_syntax_html_node(child, locals, ctx, subst, errors);
            }
        }
    }
}

/// Standard HTML attribute type category used by syntax-level checks.
///
/// Inputs:
/// - HTML attribute name from syntax output.
///
/// Output:
/// - Compact type category used for diagnostics and renderability checks.
///
/// Transformation:
/// - Groups common HTML attributes into the current Terlan type categories
///   without introducing a full DOM type model into the core typechecker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HtmlAttrType {
    Text,
    Bool,
    Class,
}

impl HtmlAttrType {
    /// Returns the human-readable type label used in diagnostics.
    ///
    /// Inputs:
    /// - `self`: standard HTML attribute type category.
    ///
    /// Output:
    /// - Diagnostic label for that category.
    ///
    /// Transformation:
    /// - Maps the compact enum variant to the Terlan-facing type text.
    fn label(self) -> &'static str {
        match self {
            HtmlAttrType::Text => "Text",
            HtmlAttrType::Bool => "Bool",
            HtmlAttrType::Class => "Text | List[Text]",
        }
    }
}

/// Looks up the built-in type category for a known HTML attribute.
///
/// Inputs:
/// - `name`: HTML attribute name.
///
/// Output:
/// - Expected attribute category when the checker knows one.
///
/// Transformation:
/// - Classifies a conservative set of common attributes and ignores unknown
///   names so custom attributes remain allowed.
fn standard_html_attr_type(name: &str) -> Option<HtmlAttrType> {
    match name {
        "href" | "src" | "id" | "name" | "type" | "value" => Some(HtmlAttrType::Text),
        "disabled" | "checked" => Some(HtmlAttrType::Bool),
        "class" => Some(HtmlAttrType::Class),
        _ => None,
    }
}

/// Checks one HTML attribute against its standard category when known.
///
/// Inputs:
/// - `attr`: syntax-output attribute.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - No direct value. Any failure is appended to `errors`.
///
/// Transformation:
/// - Infers the attribute value type, widens literals to their base types,
///   and compares it with the known HTML attribute category.
fn check_syntax_html_attr(
    attr: &SyntaxHtmlAttrOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    let actual = syntax_html_attr_type(attr, locals, ctx, subst, errors);
    let actual = widen_html_attr_type(actual);

    let Some(expected) = standard_html_attr_type(&attr.name) else {
        return;
    };

    if !html_attr_type_accepts(expected, &actual, ctx.aliases) {
        errors.push(format!(
            "attribute {} expects {}\nfound {}",
            attr.name,
            expected.label(),
            pretty_type(&actual)
        ));
    }
}

/// Infers the type of an HTML attribute value.
///
/// Inputs:
/// - `attr`: syntax-output attribute.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - Inferred attribute value type.
///
/// Transformation:
/// - Treats expression attributes as normal expressions, text attributes as
///   strings, and valueless attributes as booleans.
fn syntax_html_attr_type(
    attr: &SyntaxHtmlAttrOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    match &attr.value {
        Some(SyntaxHtmlAttrValueOutput::Expr { expr }) => {
            infer_syntax_expr(expr, locals, ctx, subst, errors)
        }
        Some(SyntaxHtmlAttrValueOutput::Text { .. }) => Type::Binary,
        None => Type::Bool,
    }
}

/// Widens literal types before HTML attribute compatibility checks.
///
/// Inputs:
/// - `ty`: inferred attribute type.
///
/// Output:
/// - Widened attribute type.
///
/// Transformation:
/// - Converts literal int and atom types to their primitive categories while
///   leaving all other types untouched.
fn widen_html_attr_type(ty: Type) -> Type {
    match ty {
        Type::LiteralInt(_) => Type::Int,
        Type::LiteralAtom(_) => Type::Atom,
        other => other,
    }
}

/// Returns whether an HTML attribute category accepts an inferred type.
///
/// Inputs:
/// - `expected`: standard HTML attribute category.
/// - `actual`: inferred attribute type.
/// - `aliases`: local type aliases used to expand structural aliases.
///
/// Output:
/// - `true` when the inferred type is compatible.
///
/// Transformation:
/// - Expands aliases and applies the current conservative HTML attribute
///   compatibility rules.
fn html_attr_type_accepts(
    expected: HtmlAttrType,
    actual: &Type,
    aliases: &HashMap<String, TypeAlias>,
) -> bool {
    let actual = expand_type_aliases(actual, aliases);
    match expected {
        HtmlAttrType::Text => matches!(actual, Type::Binary | Type::Dynamic),
        HtmlAttrType::Bool => matches!(actual, Type::Bool | Type::Dynamic),
        HtmlAttrType::Class => {
            matches!(actual, Type::Binary | Type::Dynamic)
                || matches!(
                    actual,
                    Type::List(item) if matches!(
                        expand_type_aliases(&item, aliases),
                        Type::Binary | Type::Dynamic
                    )
                )
        }
    }
}

/// Infers the renderable type of an embedded HTML child expression.
///
/// Inputs:
/// - `expr`: embedded expression node.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - Inferred child value type.
///
/// Transformation:
/// - Infers the expression normally. List comprehensions are treated as a
///   stream of rendered children, so the element type is returned.
fn infer_syntax_html_child_expr(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let ty = infer_syntax_expr(expr, locals, ctx, subst, errors);
    if expr.kind == SyntaxExprKind::ListComprehension {
        return match expand_type_aliases(&ty, ctx.aliases) {
            Type::List(elem) => *elem,
            other => other,
        };
    }
    ty
}

/// Returns whether an HTML element name denotes a component.
///
/// Inputs:
/// - `name`: element tag name.
///
/// Output:
/// - `true` when the first character is uppercase ASCII.
///
/// Transformation:
/// - Applies Terlan's current component naming convention.
fn is_component_element_name(name: &str) -> bool {
    matches!(name.chars().next(), Some(ch) if ch.is_ascii_uppercase())
}

/// Infers the return type of an HTML component call.
///
/// Inputs:
/// - `element`: uppercase HTML element treated as a component call.
/// - `locals`, `ctx`, and `subst`: active expression-inference environment.
/// - `errors`: mutable type diagnostic text sink.
///
/// Output:
/// - Inferred component return type, or `Dynamic` when no component matches.
///
/// Transformation:
/// - Finds a local function matching the component name/arity, orders
///   attributes by function parameter order when possible, infers attribute
///   expression types, and dispatches through normal function-bound inference.
fn infer_syntax_html_component_call(
    element: &SyntaxHtmlElementOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let arity = element.attrs.len();
    for name in component_function_names(&element.name) {
        if let Some(schemes) = ctx.signatures.get(&(name.clone(), arity)) {
            let arg_types =
                syntax_component_arg_types(element, ctx.local_fns.get(&(name.clone(), arity)))
                    .into_iter()
                    .map(|attr| syntax_html_attr_type(attr, locals, ctx, subst, errors))
                    .collect::<Vec<_>>();

            return match infer_function_scheme_overload(schemes, &name, &arg_types, ctx, subst) {
                Ok(ty) => ty,
                Err(message) => {
                    errors.push(message);
                    Type::Dynamic
                }
            };
        }
    }

    Type::Dynamic
}

/// Returns candidate local function names for an HTML component tag.
///
/// Inputs:
/// - `tag_name`: component tag name as written in HTML syntax.
///
/// Output:
/// - Candidate function names in lookup order.
///
/// Transformation:
/// - Tries the tag name itself and, when different, its snake-case form.
fn component_function_names(tag_name: &str) -> Vec<String> {
    let snake_case = identifier_to_snake(tag_name);
    if snake_case == tag_name {
        vec![tag_name.to_string()]
    } else {
        vec![tag_name.to_string(), snake_case]
    }
}

/// Orders component attributes to match function parameters when possible.
///
/// Inputs:
/// - `element`: component element with source attributes.
/// - `symbol`: matching function symbol, when available.
///
/// Output:
/// - Attribute references in call argument order.
///
/// Transformation:
/// - Reorders by parameter names if every parameter can be matched; otherwise
///   preserves source attribute order.
fn syntax_component_arg_types<'a>(
    element: &'a SyntaxHtmlElementOutput,
    symbol: Option<&FunctionSymbol>,
) -> Vec<&'a SyntaxHtmlAttrOutput> {
    if let Some(symbol) = symbol {
        let mut ordered = Vec::with_capacity(element.attrs.len());
        for param in &symbol.params {
            let Some(attr) = element
                .attrs
                .iter()
                .find(|attr| component_prop_matches_param(&attr.name, &param.name))
            else {
                return element.attrs.iter().collect();
            };
            ordered.push(attr);
        }
        ordered
    } else {
        element.attrs.iter().collect()
    }
}

/// Returns whether an HTML prop name matches a function parameter name.
///
/// Inputs:
/// - `prop_name`: HTML attribute name.
/// - `param_name`: Terlan function parameter name.
///
/// Output:
/// - `true` when the names are equivalent under supported conventions.
///
/// Transformation:
/// - Compares exact, case-insensitive, and snake-case forms.
fn component_prop_matches_param(prop_name: &str, param_name: &str) -> bool {
    prop_name == param_name
        || prop_name.eq_ignore_ascii_case(param_name)
        || prop_name == identifier_to_snake(param_name)
}

/// Returns whether a type can be rendered as an HTML child.
///
/// Inputs:
/// - `ty`: inferred child expression type.
/// - `aliases`: local type aliases used to expand structural aliases.
///
/// Output:
/// - `true` when the type is currently renderable.
///
/// Transformation:
/// - Expands aliases, recursively checks union members, and accepts primitive
///   renderable values plus `Html[Msg]`.
fn is_renderable_html_child_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    let ty = expand_type_aliases(ty, aliases);
    if let Type::Union(items) = &ty {
        return items
            .iter()
            .all(|item| is_renderable_html_child_type(item, aliases));
    }
    matches!(ty, Type::Binary | Type::Int | Type::Bool | Type::Dynamic)
        || matches!(
            ty,
            Type::Named {
                module: None,
                name,
                args,
            } if name == "Html" && args.len() == 1
        )
}

/// Returns whether a type is an HTML expression value.
///
/// Inputs:
/// - `ty`: inferred expression type.
/// - `aliases`: local type aliases used to expand structural aliases.
///
/// Output:
/// - `true` when the type is `Html[_]`.
///
/// Transformation:
/// - Expands aliases and checks the canonical local `Html` type shape.
fn is_html_expression_type(ty: &Type, aliases: &HashMap<String, TypeAlias>) -> bool {
    let ty = expand_type_aliases(ty, aliases);
    matches!(
        ty,
        Type::Named {
            module: None,
            name,
            args,
        } if name == "Html" && args.len() == 1
    )
}
