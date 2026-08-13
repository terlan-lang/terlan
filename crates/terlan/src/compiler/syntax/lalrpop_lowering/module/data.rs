use super::{
    super::{
        super::{
            lalrpop_syntax::{LalrpopSyntaxNode, LalrpopSyntaxNodeKind as Kind},
            lexer::lex,
            parse_tree::{
                AnnotationKeyOption, AnnotationSchemaDecl, AnnotationSchemaEntry,
                AnnotationValueType, ConstantDecl, Decl, ImplConstDecl, ImportDecl, ImportItem,
                ImportKind, ShapeDecl, StructDecl, StructFieldDecl, TemplateDecl, TemplatePropDecl,
                TraitConstDecl, TraitDecl, TraitImplDecl, TraitMethodDecl, TypeDecl, TypeExpr,
                UnsupportedDecl, ValuedUnionArmDecl,
            },
            token::TokenKind,
        },
        patterns::unquote,
        LalrpopLoweringContext, LalrpopLoweringResult,
    },
    annotations, callables, declaration_identity, head_constraint_texts, metadata_bool,
    metadata_count, without_annotations,
};

pub(super) fn lower_import(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
) -> LalrpopLoweringResult<Decl> {
    let body = node.text.as_deref().unwrap_or_default().trim();
    let (kind, body) = if let Some(rest) = body.strip_prefix("file ") {
        (ImportKind::File, rest)
    } else if let Some(rest) = body.strip_prefix("css ") {
        (ImportKind::Css, rest)
    } else if let Some(rest) = body.strip_prefix("markdown ") {
        (ImportKind::Markdown, rest)
    } else {
        (ImportKind::Module, body)
    };
    if kind != ImportKind::Module {
        let (path, alias) = body
            .split_once(" as ")
            .ok_or_else(|| context.error(node, "asset import is missing its alias"))?;
        return Ok(Decl::Import(ImportDecl {
            kind,
            module_name: alias.trim().to_string(),
            items: vec![ImportItem {
                name: alias.trim().to_string(),
                as_alias: None,
                span: node.span,
            }],
            is_type: false,
            is_selected: false,
            source_path: unquote(path.trim()).or_else(|| Some(path.trim().to_string())),
            span: node.span,
        }));
    }
    let (is_type, body) = body
        .strip_prefix("type ")
        .map_or((false, body), |body| (true, body));
    let (module_name, selector) = body
        .split_once(".{")
        .map_or((body.trim_end_matches('.'), None), |(module, selector)| {
            (module, Some(selector.trim_end_matches('}')))
        });
    let items = selector
        .map(|selector| {
            if selector == "*" {
                return vec![ImportItem {
                    name: "*".to_string(),
                    as_alias: None,
                    span: node.span,
                }];
            }
            selector
                .split(',')
                .map(|item| {
                    let (name, alias) = item
                        .trim()
                        .split_once(" as ")
                        .map_or((item.trim(), None), |(name, alias)| {
                            (name.trim(), Some(alias.trim().to_string()))
                        });
                    ImportItem {
                        name: name.to_string(),
                        as_alias: alias,
                        span: import_item_span(context, node, name),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let (module_name, items) = if selector.is_none() {
        let (path, alias) = module_name
            .rsplit_once(" as ")
            .map_or((module_name, None), |(path, alias)| {
                (path.trim(), Some(alias.trim().to_string()))
            });
        let (module, item) = path
            .rsplit_once('.')
            .ok_or_else(|| context.error(node, "expected import module"))?;
        (
            module.to_string(),
            vec![ImportItem {
                name: item.to_string(),
                as_alias: alias,
                span: import_item_span(context, node, item),
            }],
        )
    } else {
        (module_name.to_string(), items)
    };
    Ok(Decl::Import(ImportDecl {
        kind,
        module_name,
        items,
        is_type,
        is_selected: selector.is_some(),
        source_path: None,
        span: node.span,
    }))
}

fn import_item_span(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    name: &str,
) -> crate::terlan_syntax::span::Span {
    let source = context.text(node.span);
    let search_start = source.find(".{").map_or(0, |start| start + 2);
    source[search_start..]
        .find(name)
        .map_or(node.span, |start| {
            let start = node.span.start + search_start + start;
            crate::terlan_syntax::span::Span::new(start, start + name.len())
        })
}

pub(super) fn lower_constant(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let (name, is_public, _, _) = declaration_identity(node);
    if !name.chars().all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(context.error(node, "constant names must use SCREAMING_SNAKE_CASE"));
    }
    if children.len() != 2 {
        return Err(context.error(node, "constant declaration is malformed"));
    }
    Ok(Decl::Constant(ConstantDecl {
        name,
        annotation: context.type_expression(&children[0]),
        value: context.expression(&children[1])?,
        is_public,
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_type(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
    is_interface: bool,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let (identity, is_public, _, _) = declaration_identity(node);
    let is_opaque = identity.starts_with("opaque ");
    let is_valued = identity.starts_with("valued ");
    let name = identity
        .strip_prefix("opaque ")
        .or_else(|| identity.strip_prefix("valued "))
        .unwrap_or(&identity)
        .to_string();
    if is_valued {
        let parameter_count = metadata_count(node, "params");
        let params = children[..parameter_count]
            .iter()
            .map(|parameter| context.type_text(parameter))
            .collect();
        let base = children
            .get(parameter_count)
            .ok_or_else(|| context.error(node, "valued union base type is missing"))?;
        let valued_arms =
            children[parameter_count + 1..]
                .iter()
                .map(|arm| {
                    Ok(ValuedUnionArmDecl {
                        name: arm.text.clone().unwrap_or_default(),
                        value: context.expression(arm.children.first().ok_or_else(|| {
                            context.error(arm, "valued union arm is malformed")
                        })?)?,
                        span: arm.span,
                    })
                })
                .collect::<LalrpopLoweringResult<Vec<_>>>()?;
        return Ok(Decl::Type(TypeDecl {
            name,
            params,
            variants: vec![context.type_expression(base)],
            representation: Some(context.type_expression(base)),
            valued_arms,
            implements: Vec::new(),
            is_public,
            is_opaque: false,
            docs,
            span: node.span,
        }));
    }
    let parameter_count = metadata_count(node, "params");
    let implement_count = metadata_count(node, "implements");
    let params = children[..parameter_count]
        .iter()
        .map(|parameter| context.type_text(parameter))
        .collect::<Vec<_>>();
    if let Some(parameter) = params.iter().find(|parameter| {
        parameter.starts_with("const ")
            && !["Int", "Bool", "Atom"]
                .iter()
                .any(|kind| parameter.ends_with(kind))
    }) {
        return Err(context.error(
            node,
            format!("const generic parameter `{parameter}` must use Int, Bool, or Atom"),
        ));
    }
    let implements = children[parameter_count..parameter_count + implement_count]
        .iter()
        .map(|implementation| context.type_expression(implementation))
        .collect();
    let declared_body = metadata_bool(node, "representation")
        .then(|| {
            children
                .get(parameter_count + implement_count)
                .map(|child| context.type_expression(child))
        })
        .flatten();
    if declared_body.is_none() && parameter_count > 0 && !is_opaque && !is_interface {
        return Err(context.error(node, "expected `=` in type declaration"));
    }
    let implicit_atom = if declared_body.is_none() && !is_opaque {
        let atom = format!("Atom[\"{}\"]", identifier_to_snake(&name));
        Some(TypeExpr {
            text: atom,
            span: node.span,
        })
    } else {
        None
    };
    if declared_body
        .as_ref()
        .is_some_and(|body| body.text.contains("=>"))
    {
        return Err(context.error(
            node,
            "implication arrows are only valid on generic parameters",
        ));
    }
    let variants = children
        .get(parameter_count + implement_count)
        .filter(|_| metadata_bool(node, "representation"))
        .map(|representation_node| {
            if representation_node.kind == Kind::TypeUnion {
                representation_node
                    .children
                    .iter()
                    .map(|child| context.type_expression(child))
                    .collect()
            } else {
                vec![context.type_expression(representation_node)]
            }
        })
        .unwrap_or_else(|| implicit_atom.into_iter().collect());
    Ok(Decl::Type(TypeDecl {
        name,
        params,
        variants,
        representation: None,
        valued_arms: Vec::new(),
        implements,
        is_public,
        is_opaque,
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_struct(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let parameter_count = metadata_count(node, "params");
    let include_count = metadata_count(node, "includes");
    let implement_count = metadata_count(node, "implements");
    let field_start = parameter_count + include_count + implement_count;
    let field_nodes = &children[field_start..];
    let field_docs = struct_field_docs(context, node, field_nodes);
    let fields = field_nodes
        .iter()
        .zip(field_docs)
        .map(|(field, docs)| lower_struct_field(context, field, docs))
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let (name, is_public, _, _) = declaration_identity(node);
    Ok(Decl::Struct(StructDecl {
        name,
        generic_params: text_nodes(context, &children[..parameter_count]),
        includes: text_nodes(
            context,
            &children[parameter_count..parameter_count + include_count],
        ),
        implements: children[parameter_count + include_count..field_start]
            .iter()
            .map(|child| context.type_expression(child))
            .collect(),
        fields,
        is_public,
        docs,
        span: node.span,
    }))
}

fn lower_struct_field(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<StructFieldDecl> {
    let name = node.text.clone().unwrap_or_default();
    let annotation = node
        .children
        .first()
        .map(|child| context.type_expression(child))
        .ok_or_else(|| context.error(node, "struct field type is missing"))?;
    let default = node
        .children
        .get(1)
        .map(|child| context.expression(child))
        .transpose()?;
    if annotation.text.contains("=>") {
        return Err(context.error(
            node,
            "implication arrows are only valid on generic parameters",
        ));
    }
    Ok(StructFieldDecl {
        is_private: name.starts_with('#'),
        name: name.trim_start_matches('#').to_string(),
        annotation,
        default,
        docs,
        span: node.span,
    })
}

pub(super) fn lower_template(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let identity = node.text.as_deref().unwrap_or_default();
    let (name, path) = identity
        .split_once(" from ")
        .ok_or_else(|| context.error(node, "template source path is missing"))?;
    let props = node
        .children
        .iter()
        .map(|field| {
            let lowered = lower_struct_field(context, field, Vec::new())?;
            Ok(TemplatePropDecl {
                name: lowered.name,
                annotation: lowered.annotation,
                default: lowered.default,
                span: lowered.span,
            })
        })
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    let mut saw_default = false;
    for property in &props {
        if saw_default && property.default.is_none() {
            return Err(context.error(node, "template default properties must be trailing"));
        }
        saw_default |= property.default.is_some();
    }
    Ok(Decl::Template(TemplateDecl {
        name: name.to_string(),
        source_path: unquote(path).unwrap_or_else(|| path.to_string()),
        props,
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_shape(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let (name, is_public, _, _) = declaration_identity(node);
    let parameter_count = metadata_count(node, "params");
    let body = node
        .children
        .get(parameter_count)
        .ok_or_else(|| context.error(node, "shape body is missing"))?;
    let guard = metadata_bool(node, "guard")
        .then(|| node.children.get(parameter_count + 1))
        .flatten()
        .map(|guard| context.text(guard.span).to_string());
    Ok(Decl::Shape(ShapeDecl {
        name,
        params: text_nodes(context, &node.children[..parameter_count]),
        body: spaced_source(context, body),
        guard: guard.map(|_| {
            node.children
                .get(parameter_count + 1)
                .map(|guard| spaced_source(context, guard))
                .unwrap_or_default()
        }),
        text: spaced_source(context, node),
        docs,
        is_public,
        span: node.span,
    }))
}

pub(super) fn lower_trait(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let parameter_count = metadata_count(node, "params");
    let super_count = metadata_count(node, "supers");
    let mut methods = Vec::new();
    let mut constants = Vec::new();
    let members = &children[parameter_count + super_count..];
    let member_docs = struct_field_docs(context, node, members);
    for (member, docs) in members.iter().zip(member_docs) {
        match member.kind {
            Kind::ConstantDeclaration => {
                let annotation = member
                    .children
                    .first()
                    .ok_or_else(|| context.error(member, "trait constant type is missing"))?;
                constants.push(TraitConstDecl {
                    name: member.text.clone().unwrap_or_default(),
                    annotation: context.type_expression(annotation),
                    default: member
                        .children
                        .get(1)
                        .map(|child| context.expression(child))
                        .transpose()?,
                    docs,
                    span: member.span,
                });
            }
            Kind::FunctionDeclaration => methods.push(lower_trait_method(context, member, docs)?),
            _ => return Err(context.error(member, "unsupported trait member")),
        }
    }
    let (name, is_public, _, _) = declaration_identity(node);
    Ok(Decl::Trait(TraitDecl {
        name,
        params: text_nodes(context, &children[..parameter_count]),
        super_traits: text_nodes(
            context,
            &children[parameter_count..parameter_count + super_count],
        ),
        methods,
        constants,
        is_public,
        docs,
        span: node.span,
    }))
}

fn lower_trait_method(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<TraitMethodDecl> {
    let annotation_count = metadata_count(node, "annotations");
    let annotations = &node.children[..annotation_count];
    let pure_annotations = annotations
        .iter()
        .filter(|annotation| annotation.text.as_deref() == Some("pure"))
        .count();
    if pure_annotations > 1 {
        return Err(context.error(node, "duplicate @pure trait method annotation"));
    }
    if let Some(annotation) = annotations
        .iter()
        .find(|annotation| annotation.text.as_deref() != Some("pure"))
    {
        return Err(context.error(
            annotation,
            format!(
                "annotation @{} is not supported on trait methods",
                annotation.text.as_deref().unwrap_or_default()
            ),
        ));
    }
    if annotations
        .iter()
        .any(|annotation| !annotation.children.is_empty())
    {
        return Err(context.error(node, "@pure does not accept metadata"));
    }
    let generic_count = metadata_count(node, "generics");
    let head_constraint_count = metadata_count(node, "head_constraints");
    let parameter_count = metadata_count(node, "params");
    let constraint_count = metadata_count(node, "constraints");
    let head_constraints_start = annotation_count + generic_count;
    let params_start = head_constraints_start + head_constraint_count;
    let constraints_start = params_start + parameter_count;
    let return_index = constraints_start + constraint_count;
    Ok(TraitMethodDecl {
        name: node
            .text
            .as_deref()
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .to_string(),
        generic_params: text_nodes(context, &node.children[annotation_count..params_start]),
        params: node.children[params_start..constraints_start]
            .iter()
            .map(|parameter| callables::lower_parameter(context, parameter))
            .collect::<LalrpopLoweringResult<Vec<_>>>()?,
        return_type: context.type_expression(&node.children[return_index]),
        generic_bounds: {
            let mut bounds = head_constraint_texts(
                context,
                &node.children[head_constraints_start..params_start],
            );
            bounds.extend(text_nodes(
                context,
                &node.children[constraints_start..return_index],
            ));
            bounds
        },
        default_body: metadata_bool(node, "body")
            .then(|| node.children.get(return_index + 1))
            .flatten()
            .map(|body| context.expression(body))
            .transpose()?,
        is_pure: node.children[..annotation_count].iter().any(|annotation| {
            annotation
                .text
                .as_deref()
                .is_some_and(|text| text == "pure")
        }),
        docs,
        is_public: false,
        span: node.span,
    })
}

pub(super) fn lower_trait_impl(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let children = without_annotations(node);
    let raw_trait_ref = children
        .first()
        .map(|child| context.type_expression(child))
        .ok_or_else(|| context.error(node, "implementation trait reference is missing"))?;
    let (split_trait_ref, generic_params) =
        crate::terlan_syntax::split_trait_impl_ref(&raw_trait_ref.text);
    let raw_trait_ref = TypeExpr {
        text: split_trait_ref,
        span: raw_trait_ref.span,
    };
    let has_body = metadata_bool(node, "body");
    let (trait_ref, negative_target) = if !has_body {
        split_negative_trait_target(&raw_trait_ref)
    } else {
        (raw_trait_ref.clone(), None)
    };
    let for_type = has_body
        .then(|| children.get(1))
        .flatten()
        .map(|child| context.type_expression(child))
        .or(negative_target)
        .unwrap_or_else(|| trait_ref.clone());
    let member_start = if has_body { 2 } else { 1 };
    let mut methods = Vec::new();
    let mut constants = Vec::new();
    for member in &children[member_start..] {
        if member.kind == Kind::FunctionDeclaration {
            let lowered = callables::lower_function(context, member, Vec::new())?;
            let Decl::Function(function) = lowered else {
                return Err(context.error(member, "implementation method lowered incorrectly"));
            };
            methods.push(function);
        } else if member.kind == Kind::ConstantDeclaration {
            let (annotation, value) = match member.children.as_slice() {
                [value] => (None, value),
                [annotation, value] => (Some(context.type_expression(annotation)), value),
                _ => return Err(context.error(member, "implementation constant is malformed")),
            };
            constants.push(ImplConstDecl {
                name: member.text.clone().unwrap_or_default(),
                annotation,
                value: context.expression(value)?,
                span: member.span,
            });
        }
    }
    let head = node.text.as_deref().unwrap_or_default();
    Ok(Decl::TraitImpl(TraitImplDecl {
        trait_ref,
        generic_params,
        for_type,
        methods,
        constants,
        is_negative: head.contains("not impl"),
        is_public: head.starts_with("pub "),
        docs,
        span: node.span,
    }))
}

pub(super) fn lower_annotation_schema(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let head = node.text.as_deref().unwrap_or_default();
    let is_public = head.starts_with("pub ");
    let path = head
        .strip_prefix("pub ")
        .unwrap_or(head)
        .strip_prefix("schema ")
        .unwrap_or(head)
        .split('.')
        .map(str::to_string)
        .collect();
    let entries = node
        .children
        .iter()
        .filter(|child| child.kind == Kind::AnnotationValue)
        .map(|entry| {
            let value = entry
                .children
                .first()
                .ok_or_else(|| context.error(entry, "annotation schema value is missing"))?;
            if entry.text.as_deref() == Some("applies_to") {
                let targets = if value.children.is_empty() {
                    vec![context.text(value.span).to_string()]
                } else {
                    value
                        .children
                        .iter()
                        .map(|target| context.text(target.span).to_string())
                        .collect()
                };
                return Ok(AnnotationSchemaEntry::AppliesTo {
                    targets,
                    span: entry.span,
                });
            }
            Ok(AnnotationSchemaEntry::Key {
                key: entry
                    .text
                    .as_deref()
                    .unwrap_or_default()
                    .split('.')
                    .map(str::to_string)
                    .collect(),
                value_type: AnnotationValueType {
                    text: context.text(value.span).to_string(),
                },
                options: lower_schema_options(context, &entry.children[1..])?,
                span: entry.span,
            })
        })
        .collect::<LalrpopLoweringResult<Vec<_>>>()?;
    Ok(Decl::AnnotationSchema(AnnotationSchemaDecl {
        path,
        entries,
        is_public,
        docs,
        span: node.span,
    }))
}

fn lower_schema_options(
    context: &LalrpopLoweringContext<'_>,
    nodes: &[LalrpopSyntaxNode],
) -> LalrpopLoweringResult<Vec<AnnotationKeyOption>> {
    nodes
        .iter()
        .filter_map(|node| {
            let source = context.text(node.span);
            let value = node.children.last()?;
            if source.starts_with("required") {
                Some(Ok(AnnotationKeyOption::Required {
                    value: context.text(value.span) == "true",
                    span: node.span,
                }))
            } else if source.starts_with("repeatable") {
                Some(Ok(AnnotationKeyOption::Repeatable {
                    value: context.text(value.span) == "true",
                    span: node.span,
                }))
            } else if source.starts_with("default") {
                Some(annotations::lower_value(context, value).map(|value| {
                    AnnotationKeyOption::Default {
                        value,
                        span: node.span,
                    }
                }))
            } else if source.starts_with("applies_to") {
                let targets = if value.text.as_deref() == Some("list") {
                    value
                        .children
                        .iter()
                        .map(|target| context.text(target.span).to_string())
                        .collect()
                } else {
                    vec![context.text(value.span).to_string()]
                };
                Some(Ok(AnnotationKeyOption::AppliesTo {
                    targets,
                    span: node.span,
                }))
            } else {
                None
            }
        })
        .collect()
}

fn identifier_to_snake(name: &str) -> String {
    let characters = name.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(name.len());
    for (index, character) in characters.iter().copied().enumerate() {
        let previous = index.checked_sub(1).and_then(|index| characters.get(index));
        let next = characters.get(index + 1);
        let boundary = character.is_ascii_uppercase()
            && (!output.is_empty())
            && (previous.is_some_and(|previous| {
                previous.is_ascii_lowercase() || previous.is_ascii_digit()
            }) || next.is_some_and(|next| next.is_ascii_lowercase()));
        let digit_boundary = character.is_ascii_digit()
            && previous.is_some_and(|previous| previous.is_ascii_alphabetic());
        if (boundary || digit_boundary) && !output.ends_with('_') {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

pub(super) fn lower_raw(
    context: &LalrpopLoweringContext<'_>,
    node: &LalrpopSyntaxNode,
    docs: Vec<String>,
) -> LalrpopLoweringResult<Decl> {
    let text = context.text(node.span).to_string();
    let kind = if node.kind == Kind::ConfigDeclaration {
        text.split_whitespace()
            .next()
            .unwrap_or("config")
            .to_string()
    } else {
        node.text.clone().unwrap_or_else(|| "raw".to_string())
    };
    Ok(Decl::Raw(UnsupportedDecl {
        kind,
        text,
        docs,
        span: node.span,
    }))
}

fn text_nodes(context: &LalrpopLoweringContext<'_>, nodes: &[LalrpopSyntaxNode]) -> Vec<String> {
    nodes.iter().map(|node| context.type_text(node)).collect()
}

fn struct_field_docs(
    context: &LalrpopLoweringContext<'_>,
    owner: &LalrpopSyntaxNode,
    fields: &[LalrpopSyntaxNode],
) -> Vec<Vec<String>> {
    let tokens = lex(context.text(owner.span)).unwrap_or_default();
    let mut previous_end = 0usize;
    fields
        .iter()
        .map(|field| {
            let local_start = field.span.start.saturating_sub(owner.span.start);
            let docs = tokens
                .iter()
                .filter(|token| {
                    token.start >= previous_end
                        && token.end <= local_start
                        && matches!(
                            token.kind,
                            TokenKind::DocComment | TokenKind::DocBlockComment
                        )
                })
                .map(|token| token.text.clone())
                .collect();
            previous_end = field.span.end.saturating_sub(owner.span.start);
            docs
        })
        .collect()
}

fn spaced_source(context: &LalrpopLoweringContext<'_>, node: &LalrpopSyntaxNode) -> String {
    lex(context.text(node.span))
        .unwrap_or_default()
        .into_iter()
        .filter(|token| {
            token.kind != TokenKind::EOF
                && !matches!(
                    token.kind,
                    TokenKind::Comment
                        | TokenKind::DocComment
                        | TokenKind::DocBlockComment
                        | TokenKind::ModuleDocComment
                )
        })
        .map(|token| token.text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_negative_trait_target(trait_ref: &TypeExpr) -> (TypeExpr, Option<TypeExpr>) {
    let text = trait_ref.text.trim();
    let Some(open) = text.find('[') else {
        return (trait_ref.clone(), None);
    };
    let Some(inner) = text.strip_suffix(']').map(|text| &text[open + 1..]) else {
        return (trait_ref.clone(), None);
    };
    (
        TypeExpr {
            text: text[..open].to_string(),
            span: trait_ref.span,
        },
        Some(TypeExpr {
            text: inner.to_string(),
            span: trait_ref.span,
        }),
    )
}
