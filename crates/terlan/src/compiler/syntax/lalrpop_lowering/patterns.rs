use super::{
    super::{
        lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind},
        parse_tree::{
            BinaryLayoutField, MapField, Pattern, StringPatternCapture, StringPatternSegment,
            TypeExpr,
        },
    },
    binary_layout, LalrpopLoweringContext, LalrpopLoweringResult,
};

pub(super) fn lower_pattern(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Pattern> {
    use LalrpopSyntaxNodeKind as Kind;
    match node.kind {
        Kind::Pattern
            if node
                .text
                .as_deref()
                .is_some_and(|text| text.starts_with("alias:")) =>
        {
            let alias = node
                .text
                .as_deref()
                .and_then(|text| text.strip_prefix("alias:"))
                .unwrap_or_default()
                .to_string();
            let pattern = node
                .children
                .first()
                .ok_or_else(|| context.error(node, "alias pattern is missing its target"))?;
            Ok(Pattern::Alias {
                alias,
                pattern: Box::new(lower_pattern(context, pattern)?),
            })
        }
        Kind::Pattern | Kind::Binding | Kind::Int | Kind::Float | Kind::String => {
            lower_leaf_pattern(context, node)
        }
        Kind::AtomLiteral => {
            let value = node
                .text
                .as_deref()
                .and_then(unquote)
                .or_else(|| node.text.clone())
                .ok_or_else(|| context.error(node, "invalid atom pattern literal"))?;
            Ok(Pattern::AtomLiteral(value))
        }
        Kind::Index
            if node.children.len() == 2
                && node.children[0].kind == Kind::Binding
                && node.children[0].text.as_deref() == Some("Atom")
                && node.children[1].kind == Kind::String =>
        {
            let value = node.children[1]
                .text
                .as_deref()
                .and_then(unquote)
                .ok_or_else(|| context.error(node, "invalid atom pattern literal"))?;
            Ok(Pattern::AtomLiteral(value))
        }
        Kind::PatternTuple | Kind::Tuple => {
            Ok(Pattern::Tuple(lower_patterns(context, &node.children)?))
        }
        Kind::PatternList | Kind::List => {
            Ok(Pattern::List(lower_patterns(context, &node.children)?))
        }
        Kind::PatternListCons | Kind::ListCons => {
            require_children(context, node, 2)?;
            Ok(Pattern::ListCons(
                Box::new(lower_pattern(context, &node.children[0])?),
                Box::new(lower_pattern(context, &node.children[1])?),
            ))
        }
        Kind::PatternMap | Kind::Map => Ok(Pattern::Map(lower_fields(context, &node.children)?)),
        Kind::PatternConstructor => {
            let name = node
                .text
                .clone()
                .ok_or_else(|| context.error(node, "constructor pattern is missing its name"))?;
            if context.text(node.span).contains('(') && node.children.is_empty() {
                Ok(Pattern::NullaryConstructorCall(name))
            } else if !context.text(node.span).contains('(')
                && node
                    .children
                    .iter()
                    .all(|child| matches!(child.kind, Kind::PatternField | Kind::MapField))
            {
                Ok(Pattern::Record {
                    name,
                    fields: lower_fields(context, &node.children)?,
                })
            } else {
                let mut parts = Vec::with_capacity(node.children.len() + 1);
                parts.push(Pattern::Atom(name));
                parts.extend(lower_patterns(context, &node.children)?);
                Ok(Pattern::Tuple(parts))
            }
        }
        Kind::Call => {
            let (callee, arguments) = node.children.split_first().ok_or_else(|| {
                context.error(node, "constructor pattern call is missing its name")
            })?;
            let name = callee
                .text
                .clone()
                .unwrap_or_else(|| context.text(callee.span).to_string());
            if arguments.is_empty() {
                return Ok(Pattern::NullaryConstructorCall(name));
            }
            let mut parts = Vec::with_capacity(arguments.len() + 1);
            parts.push(Pattern::Atom(name));
            parts.extend(lower_patterns(context, arguments)?);
            Ok(Pattern::Tuple(parts))
        }
        Kind::BinaryLayout => {
            let endian = node
                .text
                .as_deref()
                .and_then(|text| text.split_once('['))
                .and_then(|(_, tail)| tail.strip_suffix(']'))
                .unwrap_or_default()
                .to_string();
            let fields = node
                .children
                .iter()
                .map(|field| {
                    let descriptor = field.children.first().ok_or_else(|| {
                        context.error(field, "binary pattern field is missing its descriptor")
                    })?;
                    Ok(BinaryLayoutField {
                        name: field.text.clone().unwrap_or_default(),
                        descriptor: TypeExpr {
                            text: context.type_text(descriptor),
                            span: descriptor.span,
                        },
                    })
                })
                .collect::<LalrpopLoweringResult<Vec<_>>>()?;
            binary_layout::validate(context, node, &endian, &fields)?;
            Ok(Pattern::BinaryLayout { endian, fields })
        }
        _ => Err(context.error(
            node,
            format!("generated node {:?} is not a pattern", node.kind),
        )),
    }
}

fn lower_leaf_pattern(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Pattern> {
    let text = node
        .text
        .as_deref()
        .unwrap_or_else(|| context.text(node.span));
    if node.kind == LalrpopSyntaxNodeKind::Pattern
        && is_formal_atom_literal_source(context.text(node.span))
    {
        let value = node
            .text
            .as_deref()
            .and_then(unquote)
            .ok_or_else(|| context.error(node, "invalid atom pattern literal"))?;
        return Ok(Pattern::AtomLiteral(value));
    }
    if text == "_" {
        return Ok(Pattern::Wildcard);
    }
    if let Some(value) = parse_int(text) {
        return Ok(Pattern::Int(value));
    }
    if has_float_literal_shape(text) {
        if let Ok(value) = text.parse::<f64>() {
            return Ok(Pattern::Float(value));
        }
    }
    if text.starts_with('"') {
        let value =
            unquote(text).ok_or_else(|| context.error(node, "invalid string pattern literal"))?;
        return lower_string_pattern(context, node, &value);
    }
    if text.starts_with("Atom[") {
        let value = text
            .strip_prefix("Atom[")
            .and_then(|value| value.strip_suffix(']'))
            .and_then(unquote)
            .ok_or_else(|| context.error(node, "invalid atom pattern literal"))?;
        return Ok(Pattern::AtomLiteral(value));
    }
    if matches!(text, "true" | "false") {
        return Ok(Pattern::Atom(text.to_string()));
    }
    if text.contains('.') || text.chars().next().is_some_and(char::is_uppercase) {
        Ok(Pattern::Tuple(vec![Pattern::Atom(text.to_string())]))
    } else {
        Ok(Pattern::Var(text.to_string()))
    }
}

/// Reports whether pattern text has the lexical shape of a float literal.
///
/// Rust accepts names such as `inf`, `infinity`, and `nan` when parsing an
/// `f64`. Terlan treats those spellings as identifiers, so only source that
/// begins with a decimal digit and contains a float marker reaches the parser.
fn has_float_literal_shape(text: &str) -> bool {
    text.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && (text.contains('.') || text.contains('e') || text.contains('E'))
}

/// Reports whether a pattern source slice is canonical `Atom["..."]` syntax.
///
/// Shape signatures are deliberately reserialized with token spacing before
/// import, so this recognizes both source-tight and token-spaced brackets.
fn is_formal_atom_literal_source(source: &str) -> bool {
    source
        .trim_start()
        .strip_prefix("Atom")
        .is_some_and(|rest| rest.trim_start().starts_with('['))
}

fn lower_string_pattern(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    value: &str,
) -> LalrpopLoweringResult<Pattern> {
    if !value.contains("${") {
        return Ok(Pattern::String(value.to_string()));
    }
    let mut segments = Vec::new();
    let mut rest = value;
    let mut previous_capture = false;
    while let Some(start) = rest.find("${") {
        let literal = &rest[..start];
        if !literal.is_empty() {
            segments.push(StringPatternSegment::Literal(literal.to_string()));
        } else if previous_capture {
            return Err(context.error(node, "adjacent string captures require a literal separator"));
        }
        let tail = &rest[start + 2..];
        let end = tail
            .find('}')
            .ok_or_else(|| context.error(node, "unterminated string capture pattern"))?;
        segments.push(StringPatternSegment::Capture(lower_string_capture(
            context,
            node,
            tail[..end].trim(),
        )?));
        previous_capture = true;
        rest = &tail[end + 1..];
    }
    if !rest.is_empty() {
        segments.push(StringPatternSegment::Literal(rest.to_string()));
    }
    Ok(Pattern::StringSegments(segments))
}

fn lower_string_capture(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    source: &str,
) -> LalrpopLoweringResult<StringPatternCapture> {
    if source.is_empty() {
        return Err(context.error(node, "empty string capture pattern"));
    }
    let (name, annotation) = match source.split_once(':') {
        Some((_name, annotation)) if annotation.trim().is_empty() => {
            return Err(context.error(node, "string capture type annotation cannot be empty"));
        }
        Some((name, annotation)) => (
            name.trim(),
            Some(TypeExpr {
                text: annotation.trim().to_string(),
                span: node.span,
            }),
        ),
        None => (source.trim(), None),
    };
    let mut chars = name.chars();
    if chars
        .next()
        .is_none_or(|first| first != '_' && !first.is_ascii_lowercase())
        || !chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
    {
        return Err(context.error(node, "string capture names must be lower-case bindings"));
    }
    Ok(StringPatternCapture {
        name: name.to_string(),
        annotation,
    })
}

fn lower_patterns(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<Vec<Pattern>> {
    nodes
        .iter()
        .map(|node| lower_pattern(context, node))
        .collect()
}

fn lower_fields(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<Vec<MapField>> {
    nodes
        .iter()
        .map(|field| {
            let value = field.children.first().ok_or_else(|| {
                context.error(field, "generated pattern field is missing its value")
            })?;
            let raw_key = field.text.clone().unwrap_or_default();
            Ok(MapField {
                key: unquote(&raw_key).unwrap_or(raw_key),
                value: Box::new(lower_pattern(context, value)?),
                required: true,
            })
        })
        .collect()
}

fn require_children(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    expected: usize,
) -> LalrpopLoweringResult<()> {
    if node.children.len() == expected {
        Ok(())
    } else {
        Err(context.error(
            node,
            format!(
                "generated {:?} node has {} children; expected {expected}",
                node.kind,
                node.children.len()
            ),
        ))
    }
}

fn parse_int(text: &str) -> Option<i64> {
    if let Some(value) = text.strip_prefix("0b") {
        i64::from_str_radix(value, 2).ok()
    } else if let Some(value) = text.strip_prefix("0x") {
        i64::from_str_radix(value, 16).ok()
    } else if let Some(value) = text.strip_prefix("0o") {
        i64::from_str_radix(value, 8).ok()
    } else {
        text.parse().ok()
    }
}

pub(super) fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        match chars.next()? {
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => output.push(other),
        }
    }
    Some(output)
}
