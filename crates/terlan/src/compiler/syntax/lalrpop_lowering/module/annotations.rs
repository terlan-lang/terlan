use super::super::{
    super::{
        lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind as Kind},
        parse_tree::{Annotation, AnnotationEntry, AnnotationValue},
    },
    patterns::unquote,
    LalrpopLoweringContext, LalrpopLoweringResult,
};

pub(super) fn lower_annotation(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Annotation> {
    let path = node
        .text
        .as_deref()
        .unwrap_or_default()
        .split('.')
        .map(str::to_string)
        .collect();
    let mut entries = Vec::new();
    let mut values = Vec::new();
    for item in &node.children {
        if item.text.as_deref() == Some("entry") && item.children.len() == 2 {
            let key = annotation_name(context, &item.children[0]);
            entries.push(AnnotationEntry {
                key: key.split('.').map(str::to_string).collect(),
                value: lower_value(context, &item.children[1])?,
                span: item.span,
            });
        } else if item.text.as_deref() == Some("value") && item.children.len() == 1 {
            values.push(lower_value(context, &item.children[0])?);
        } else {
            values.push(lower_value(context, item)?);
        }
    }
    let source = context.text(node.span);
    let args = source
        .find('{')
        .map(|start| source[start..].trim().to_string())
        .filter(|args| !args.is_empty());
    Ok(Annotation {
        path,
        args,
        entries,
        values,
        span: node.span,
    })
}

pub(super) fn lower_value(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<AnnotationValue> {
    let source = context.text(node.span).trim();
    match node.text.as_deref() {
        Some("list") => Ok(AnnotationValue::List(
            node.children
                .iter()
                .map(|child| lower_value(context, child))
                .collect::<LalrpopLoweringResult<Vec<_>>>()?,
        )),
        Some("object") => Ok(AnnotationValue::Object(
            node.children
                .iter()
                .map(|item| {
                    if item.children.len() != 2 {
                        return Err(context.error(item, "annotation object entry is malformed"));
                    }
                    Ok(AnnotationEntry {
                        key: annotation_name(context, &item.children[0])
                            .split('.')
                            .map(str::to_string)
                            .collect(),
                        value: lower_value(context, &item.children[1])?,
                        span: item.span,
                    })
                })
                .collect::<LalrpopLoweringResult<Vec<_>>>()?,
        )),
        _ if source.starts_with('"') => unquote(source)
            .map(|_| AnnotationValue::String(source.to_string()))
            .ok_or_else(|| context.error(node, "invalid annotation string")),
        _ if source == "true" => Ok(AnnotationValue::Bool(true)),
        _ if source == "false" => Ok(AnnotationValue::Bool(false)),
        _ if source.parse::<i64>().is_ok() => Ok(AnnotationValue::Int(source.to_string())),
        _ if source.parse::<f64>().is_ok() => Ok(AnnotationValue::Float(source.to_string())),
        _ if node.kind == Kind::AnnotationValue => Ok(AnnotationValue::Name(
            source.split('.').map(str::to_string).collect(),
        )),
        _ => Err(context.error(node, "unsupported annotation value")),
    }
}

fn annotation_name(context: &LalrpopLoweringContext<'_>, node: &LalrpopSyntaxNode) -> String {
    node.text
        .clone()
        .unwrap_or_else(|| context.text(node.span).to_string())
}
