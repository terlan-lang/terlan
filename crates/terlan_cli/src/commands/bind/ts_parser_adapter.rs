use oxc_ast::{
    ast::{
        BindingPattern, FormalParameter, PropertyKey, Statement, TSLiteral, TSSignature, TSType,
        TSTypeAnnotation, TSTypeName, TSTypeParameterDeclaration,
    },
    Comment,
};
use oxc_span::Span;

use super::ts_type_mapping::{TsPrimitiveType, TsRecordField, TsTypeRef};

/// Neutral TypeScript declaration file model owned by the binding generator.
///
/// Inputs:
/// - Produced from Oxc's TypeScript parser for committed `.d.ts` files.
///
/// Output:
/// - A stable list of declarations supported by the current generator slice.
///
/// Transformation:
/// - Removes Oxc lifetimes and AST node details so later binding generation
///   stages consume a crate-local contract instead of parser internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsDeclarationFile {
    pub(super) declarations: Vec<TsDeclaration>,
}

/// Neutral TypeScript declaration accepted by the generator.
///
/// Inputs:
/// - Extracted from top-level TypeScript declarations.
///
/// Output:
/// - Currently supports interface declarations required by the DOM fixture.
///
/// Transformation:
/// - Gives unsupported declaration kinds a clear adapter boundary until later
///   roadmap slices add them deliberately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TsDeclaration {
    Interface(TsInterfaceDeclaration),
    Unsupported(TsUnsupportedDeclaration),
}

/// Neutral TypeScript declaration skipped during parsing.
///
/// Inputs:
/// - Produced when a top-level TypeScript declaration is outside the current
///   generated binding surface.
///
/// Output:
/// - Source label, stable reason code, and detail text consumed by generated
///   skip manifests.
///
/// Transformation:
/// - Makes broad standard-library generation auditable: top-level declarations
///   that are not emitted must be justified rather than silently ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsUnsupportedDeclaration {
    pub(super) source: String,
    pub(super) reason: &'static str,
    pub(super) detail: String,
}

/// Neutral TypeScript interface declaration.
///
/// Inputs:
/// - Extracted from an Oxc `TSInterfaceDeclaration`.
///
/// Output:
/// - Interface name and supported members.
///
/// Transformation:
/// - Preserves source-level member order while dropping parser-only span and
///   scope metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsInterfaceDeclaration {
    pub(super) namespace: String,
    pub(super) name: String,
    pub(super) doc: Option<String>,
    pub(super) type_params: Vec<String>,
    pub(super) members: Vec<TsInterfaceMember>,
}

/// Neutral TypeScript interface member.
///
/// Inputs:
/// - Extracted from Oxc property and method signatures.
///
/// Output:
/// - Either a property or method contract for later Terlan wrapper generation.
///
/// Transformation:
/// - Separates field-like DOM properties from callable DOM methods before type
///   mapping or name conversion occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TsInterfaceMember {
    Property(TsPropertyDeclaration),
    Method(TsMethodDeclaration),
    Unsupported(TsUnsupportedMember),
}

/// Neutral TypeScript interface member skipped during parsing.
///
/// Inputs:
/// - Produced when Oxc parses a member shape that the current generator cannot
///   preserve safely.
///
/// Output:
/// - Source label, stable reason code, and detail text consumed by the DOM
///   mapping stage.
///
/// Transformation:
/// - Keeps broad TypeScript library generation non-fatal while still recording
///   every unsupported member in generated skip manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsUnsupportedMember {
    pub(super) source: String,
    pub(super) reason: &'static str,
    pub(super) detail: String,
}

/// Neutral TypeScript interface property.
///
/// Inputs:
/// - Extracted from a named Oxc `TSPropertySignature`.
///
/// Output:
/// - Name, readonly flag, optional flag, and neutral type reference.
///
/// Transformation:
/// - Converts optional syntax into metadata while preserving nullability inside
///   the type reference itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsPropertyDeclaration {
    pub(super) name: String,
    pub(super) doc: Option<String>,
    pub(super) readonly: bool,
    pub(super) optional: bool,
    pub(super) ty: TsTypeRef,
}

/// Neutral TypeScript interface method.
///
/// Inputs:
/// - Extracted from a named Oxc `TSMethodSignature`.
///
/// Output:
/// - Name, optional flag, parameters, and return type.
///
/// Transformation:
/// - Keeps method signatures independent from future receiver-function wrapper
///   generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsMethodDeclaration {
    pub(super) name: String,
    pub(super) doc: Option<String>,
    pub(super) optional: bool,
    pub(super) params: Vec<TsParameterDeclaration>,
    pub(super) return_type: TsTypeRef,
}

/// Neutral TypeScript method parameter.
///
/// Inputs:
/// - Extracted from Oxc formal parameters in TypeScript signatures.
///
/// Output:
/// - Name, optional flag, and neutral type reference.
///
/// Transformation:
/// - Rejects destructured/rest parameter shapes until the generator can map
///   them without losing source semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsParameterDeclaration {
    pub(super) name: String,
    pub(super) optional: bool,
    pub(super) ty: TsTypeRef,
}

/// Stable parser adapter error.
///
/// Inputs:
/// - Produced by Oxc parse failures or unsupported generator model shapes.
///
/// Output:
/// - Reason code and human-readable message for focused tests and future
///   binding manifests.
///
/// Transformation:
/// - Converts parser diagnostics and unsupported AST branches into deterministic
///   generator-contract failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsParseError {
    pub(super) reason: &'static str,
    pub(super) message: String,
}

/// Parses TypeScript declarations into the generator-owned neutral model.
///
/// Inputs:
/// - `source`: `.d.ts` source text.
///
/// Output:
/// - `Ok(TsDeclarationFile)` when Oxc accepts the source and all encountered
///   declarations are supported by the current adapter.
/// - `Err(TsParseError)` when Oxc rejects the source or the adapter reaches an
///   unsupported declaration/member/type shape.
///
/// Transformation:
/// - Parses with Oxc using TypeScript-definition source mode, then walks only
///   the stable subset needed by the first `std.js.Dom` generator slice.
pub(super) fn parse_ts_declaration_file(source: &str) -> Result<TsDeclarationFile, TsParseError> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::d_ts();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(TsParseError {
            reason: "ts_bindgen.parse_failed",
            message: format!("{:?}", parsed.errors),
        });
    }

    let mut declarations = Vec::new();
    for statement in &parsed.program.body {
        declarations.push(parse_top_level_statement(
            source,
            &parsed.program.comments,
            statement,
        )?);
    }

    Ok(TsDeclarationFile { declarations })
}

/// Converts one top-level Oxc statement into a neutral declaration.
///
/// Inputs:
/// - `statement`: Oxc program statement from a `.d.ts` input.
///
/// Output:
/// - Supported interface declarations or explicit unsupported declaration rows.
///
/// Transformation:
/// - Admits TypeScript interfaces for generation and records all other
///   top-level declarations as skip-manifest entries.
fn parse_top_level_statement(
    source: &str,
    comments: &[Comment],
    statement: &Statement<'_>,
) -> Result<TsDeclaration, TsParseError> {
    match statement {
        Statement::TSInterfaceDeclaration(interface) => {
            parse_interface(source, comments, interface).map(TsDeclaration::Interface)
        }
        Statement::VariableDeclaration(variable) => Ok(unsupported_declaration(
            top_level_variable_source(variable),
            "ts_bindgen.unsupported_top_level_variable",
            "top-level variables and constructors are not emitted yet",
        )),
        Statement::TSTypeAliasDeclaration(alias) => Ok(unsupported_declaration(
            alias.id.name.as_str(),
            "ts_bindgen.unsupported_top_level_type_alias",
            "type aliases are not emitted yet",
        )),
        Statement::TSEnumDeclaration(enumeration) => Ok(unsupported_declaration(
            enumeration.id.name.as_str(),
            "ts_bindgen.unsupported_top_level_enum",
            "TypeScript enums are not emitted yet",
        )),
        Statement::TSModuleDeclaration(module) => Ok(unsupported_declaration(
            top_level_module_source(module),
            "ts_bindgen.unsupported_top_level_module",
            "ambient TypeScript modules are not emitted yet",
        )),
        Statement::TSGlobalDeclaration(_) => Ok(unsupported_declaration(
            "global",
            "ts_bindgen.unsupported_top_level_module",
            "ambient TypeScript modules are not emitted yet",
        )),
        Statement::TSImportEqualsDeclaration(_) => Ok(unsupported_declaration(
            "import_equals",
            "ts_bindgen.unsupported_top_level_import_equals",
            "TypeScript import-equals declarations are not emitted yet",
        )),
        Statement::ImportDeclaration(_) => Ok(unsupported_declaration(
            "import",
            "ts_bindgen.unsupported_top_level_import",
            "TypeScript imports are not emitted yet",
        )),
        Statement::ExportAllDeclaration(_)
        | Statement::ExportDefaultDeclaration(_)
        | Statement::ExportNamedDeclaration(_)
        | Statement::TSExportAssignment(_)
        | Statement::TSNamespaceExportDeclaration(_) => Ok(unsupported_declaration(
            "export",
            "ts_bindgen.unsupported_top_level_export",
            "TypeScript exports are not emitted yet",
        )),
        Statement::FunctionDeclaration(function) => Ok(unsupported_declaration(
            function
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("function"),
            "ts_bindgen.unsupported_top_level_function",
            "top-level functions are not emitted yet",
        )),
        Statement::ClassDeclaration(class) => Ok(unsupported_declaration(
            class
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("class"),
            "ts_bindgen.unsupported_top_level_class",
            "classes are not emitted yet",
        )),
        other => Ok(unsupported_declaration(
            top_level_statement_label(other),
            "ts_bindgen.unsupported_top_level_statement",
            "statement is outside the TypeScript declaration binding surface",
        )),
    }
}

/// Returns a stable source label for an unsupported top-level variable.
///
/// Inputs:
/// - `variable`: parsed Oxc variable declaration.
///
/// Output:
/// - Borrowed simple binding name when the declaration has exactly one named
///   binding, otherwise the coarse `variable` label.
///
/// Transformation:
/// - Makes generated skip manifests reviewable for normal declaration files
///   while keeping destructuring and multi-bind declarations conservative.
fn top_level_variable_source<'a>(variable: &'a oxc_ast::ast::VariableDeclaration<'a>) -> &'a str {
    if variable.declarations.len() != 1 {
        return "variable";
    }
    match &variable.declarations[0].id {
        BindingPattern::BindingIdentifier(id) => id.name.as_str(),
        BindingPattern::ObjectPattern(_)
        | BindingPattern::ArrayPattern(_)
        | BindingPattern::AssignmentPattern(_) => "variable",
    }
}

/// Returns a stable source label for an unsupported ambient module.
///
/// Inputs:
/// - `module`: parsed Oxc TypeScript module declaration.
///
/// Output:
/// - Module identifier, quoted module string, or coarse `module` label.
///
/// Transformation:
/// - Preserves the declared module name in skip manifests without exposing Oxc
///   debug formatting.
fn top_level_module_source<'a>(module: &'a oxc_ast::ast::TSModuleDeclaration<'a>) -> &'a str {
    match &module.id {
        oxc_ast::ast::TSModuleDeclarationName::Identifier(id) => id.name.as_str(),
        oxc_ast::ast::TSModuleDeclarationName::StringLiteral(literal) => literal.value.as_str(),
    }
}

/// Builds a neutral unsupported declaration.
///
/// Inputs:
/// - `source`: source declaration label.
/// - `reason`: stable skip reason code.
/// - `detail`: human-readable skip detail.
///
/// Output:
/// - Unsupported declaration row.
///
/// Transformation:
/// - Wraps top-level parser skips in the same neutral declaration stream as
///   supported interfaces.
fn unsupported_declaration(source: &str, reason: &'static str, detail: &str) -> TsDeclaration {
    TsDeclaration::Unsupported(TsUnsupportedDeclaration {
        source: source.to_string(),
        reason,
        detail: detail.to_string(),
    })
}

/// Returns a stable label for an unsupported top-level statement.
///
/// Inputs:
/// - `statement`: Oxc statement.
///
/// Output:
/// - Stable coarse statement label.
///
/// Transformation:
/// - Avoids debug-formatting parser internals in generated skip manifests.
fn top_level_statement_label(statement: &Statement<'_>) -> &'static str {
    match statement {
        Statement::BlockStatement(_) => "block",
        Statement::BreakStatement(_) => "break",
        Statement::ContinueStatement(_) => "continue",
        Statement::DebuggerStatement(_) => "debugger",
        Statement::DoWhileStatement(_) => "do_while",
        Statement::EmptyStatement(_) => "empty",
        Statement::ExpressionStatement(_) => "expression",
        Statement::ForInStatement(_) => "for_in",
        Statement::ForOfStatement(_) => "for_of",
        Statement::ForStatement(_) => "for",
        Statement::IfStatement(_) => "if",
        Statement::LabeledStatement(_) => "labeled",
        Statement::ReturnStatement(_) => "return",
        Statement::SwitchStatement(_) => "switch",
        Statement::ThrowStatement(_) => "throw",
        Statement::TryStatement(_) => "try",
        Statement::WhileStatement(_) => "while",
        Statement::WithStatement(_) => "with",
        Statement::VariableDeclaration(_) => "variable",
        Statement::FunctionDeclaration(_) => "function",
        Statement::ClassDeclaration(_) => "class",
        Statement::TSTypeAliasDeclaration(_) => "type_alias",
        Statement::TSInterfaceDeclaration(_) => "interface",
        Statement::TSEnumDeclaration(_) => "enum",
        Statement::TSModuleDeclaration(_) | Statement::TSGlobalDeclaration(_) => "module",
        Statement::TSImportEqualsDeclaration(_) => "import_equals",
        Statement::ImportDeclaration(_) => "import",
        Statement::ExportAllDeclaration(_) => "export_all",
        Statement::ExportDefaultDeclaration(_) => "export_default",
        Statement::ExportNamedDeclaration(_) => "export_named",
        Statement::TSExportAssignment(_) => "export_assignment",
        Statement::TSNamespaceExportDeclaration(_) => "namespace_export",
    }
}

/// Converts one Oxc interface declaration into the neutral model.
///
/// Inputs:
/// - `interface`: parsed Oxc TypeScript interface declaration.
///
/// Output:
/// - `Ok(TsInterfaceDeclaration)` for supported named members.
/// - `Err(TsParseError)` when a member uses an unsupported signature shape.
///
/// Transformation:
/// - Copies the interface name and delegates member conversion while preserving
///   source member order.
fn parse_interface(
    source: &str,
    comments: &[Comment],
    interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
) -> Result<TsInterfaceDeclaration, TsParseError> {
    let members = interface
        .body
        .body
        .iter()
        .map(|member| parse_interface_member(source, comments, member))
        .collect::<Vec<_>>();

    Ok(TsInterfaceDeclaration {
        namespace: String::new(),
        name: interface.id.name.to_string(),
        doc: leading_jsdoc(source, comments, interface.span),
        type_params: parse_type_parameter_names(interface.type_parameters.as_deref()),
        members,
    })
}

/// Extracts TypeScript interface type parameter names.
///
/// Inputs:
/// - `type_parameters`: optional Oxc type-parameter declaration.
///
/// Output:
/// - Type parameter names in source order.
///
/// Transformation:
/// - Preserves only names for Terlan generic declarations. Constraints and
///   defaults are intentionally ignored until Terlan has a matching type-level
///   constraint model.
fn parse_type_parameter_names(
    type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
) -> Vec<String> {
    type_parameters
        .map(|type_parameters| {
            type_parameters
                .params
                .iter()
                .map(|parameter| parameter.name.name.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Converts one Oxc interface signature into a neutral member.
///
/// Inputs:
/// - `member`: Oxc TypeScript interface signature.
///
/// Output:
/// - Property or method members for supported signatures.
/// - Unsupported members carrying stable skip metadata for unsupported shapes.
///
/// Transformation:
/// - Locks the first DOM binding slice to named members before broader
///   TypeScript declarations are admitted while allowing large standard libs to
///   keep generating around unsupported members.
fn parse_interface_member(
    source: &str,
    comments: &[Comment],
    member: &TSSignature<'_>,
) -> TsInterfaceMember {
    let parsed = (|| match member {
        TSSignature::TSPropertySignature(property) => {
            Ok(TsInterfaceMember::Property(TsPropertyDeclaration {
                name: property_key_name(&property.key)?,
                doc: leading_jsdoc(source, comments, property.span),
                readonly: property.readonly,
                optional: property.optional,
                ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
            }))
        }
        TSSignature::TSMethodSignature(method) => {
            let params = method
                .params
                .items
                .iter()
                .map(parse_parameter)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsInterfaceMember::Method(TsMethodDeclaration {
                name: property_key_name(&method.key)?,
                doc: leading_jsdoc(source, comments, method.span),
                optional: method.optional,
                params,
                return_type: parse_optional_type_annotation(method.return_type.as_deref())?,
            }))
        }
        TSSignature::TSIndexSignature(_)
        | TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_) => Err(unsupported(
            "ts_bindgen.unsupported_interface_signature",
            "only named properties and methods are supported",
        )),
    })();

    parsed.unwrap_or_else(|error| {
        TsInterfaceMember::Unsupported(TsUnsupportedMember {
            source: interface_member_source(member),
            reason: error.reason,
            detail: error.message,
        })
    })
}

/// Extracts the leading JSDoc block attached to one TypeScript AST node.
///
/// Inputs:
/// - `source`: original `.d.ts` source text.
/// - `comments`: Oxc comments collected while parsing the source.
/// - `span`: AST span for the declaration or member whose docs should be read.
///
/// Output:
/// - Normalized doc body when a leading JSDoc comment is attached to `span`.
/// - `None` when the node has no leading JSDoc.
///
/// Transformation:
/// - Uses Oxc's comment attachment metadata instead of scanning arbitrary
///   source text, then normalizes the comment body for Terlan block docs.
fn leading_jsdoc(source: &str, comments: &[Comment], span: Span) -> Option<String> {
    comments
        .iter()
        .rev()
        .find(|comment| comment.attached_to == span.start && comment.is_jsdoc())
        .and_then(|comment| normalize_jsdoc(source, *comment))
}

/// Normalizes one raw TypeScript JSDoc comment body.
///
/// Inputs:
/// - `source`: original `.d.ts` source text.
/// - `comment`: Oxc comment known to be a JSDoc comment.
///
/// Output:
/// - Cleaned doc text without `/**`, leading `*`, or closing `*/`.
/// - `None` when normalization removes all content.
///
/// Transformation:
/// - Slices the comment through Oxc spans and converts TypeScript JSDoc into a
///   language-neutral body that the Terlan generator can re-wrap.
fn normalize_jsdoc(source: &str, comment: Comment) -> Option<String> {
    let span = comment.content_span();
    let body = source.get(span.start as usize..span.end as usize)?;
    let normalized = body
        .lines()
        .map(|line| line.trim().strip_prefix('*').unwrap_or(line.trim()).trim())
        .collect::<Vec<_>>();
    let first = normalized
        .iter()
        .position(|line| !line.is_empty())
        .unwrap_or(normalized.len());
    let last = normalized
        .iter()
        .rposition(|line| !line.is_empty())
        .map(|index| index + 1)
        .unwrap_or(first);
    if first >= last {
        None
    } else {
        Some(normalized[first..last].join("\n"))
    }
}

/// Builds a source label for an interface member.
///
/// Inputs:
/// - `member`: Oxc interface member signature.
///
/// Output:
/// - Best-effort source member label.
///
/// Transformation:
/// - Reads static property or method keys when available and otherwise falls
///   back to `member` so unsupported skip rows remain deterministic.
fn interface_member_source(member: &TSSignature<'_>) -> String {
    match member {
        TSSignature::TSPropertySignature(property) => {
            property_key_name(&property.key).unwrap_or_else(|_| "property".to_string())
        }
        TSSignature::TSMethodSignature(method) => {
            property_key_name(&method.key).unwrap_or_else(|_| "method".to_string())
        }
        TSSignature::TSIndexSignature(_) => "index_signature".to_string(),
        TSSignature::TSCallSignatureDeclaration(_) => "call_signature".to_string(),
        TSSignature::TSConstructSignatureDeclaration(_) => "construct_signature".to_string(),
    }
}

/// Converts one Oxc TypeScript signature parameter.
///
/// Inputs:
/// - `parameter`: Oxc formal parameter from a method signature.
///
/// Output:
/// - `Ok(TsParameterDeclaration)` for simple identifier parameters.
/// - `Err(TsParseError)` for destructuring or missing type annotations.
///
/// Transformation:
/// - Preserves parameter names and optionality for later wrapper generation.
fn parse_parameter(
    parameter: &FormalParameter<'_>,
) -> Result<TsParameterDeclaration, TsParseError> {
    let name = match &parameter.pattern {
        BindingPattern::BindingIdentifier(binding) => binding.name.to_string(),
        BindingPattern::ObjectPattern(_)
        | BindingPattern::ArrayPattern(_)
        | BindingPattern::AssignmentPattern(_) => {
            return Err(unsupported(
                "ts_bindgen.unsupported_parameter_pattern",
                "only identifier parameters are supported",
            ));
        }
    };

    Ok(TsParameterDeclaration {
        name,
        optional: parameter.optional,
        ty: parse_optional_type_annotation(parameter.type_annotation.as_deref())?,
    })
}

/// Converts an optional Oxc type annotation.
///
/// Inputs:
/// - `annotation`: optional Oxc TypeScript annotation.
///
/// Output:
/// - `Ok(TsTypeRef)` for supported annotations.
/// - `Err(TsParseError)` when the annotation is missing.
///
/// Transformation:
/// - Treats missing declarations as unsupported instead of broadening them to
///   `Any`, keeping generated bindings explicit.
fn parse_optional_type_annotation(
    annotation: Option<&TSTypeAnnotation<'_>>,
) -> Result<TsTypeRef, TsParseError> {
    annotation
        .map(|annotation| parse_type(&annotation.type_annotation))
        .unwrap_or_else(|| {
            Err(unsupported(
                "ts_bindgen.missing_type_annotation",
                "generated bindings require explicit TypeScript annotations",
            ))
        })
}

/// Converts an Oxc TypeScript type into the neutral type mapper model.
///
/// Inputs:
/// - `ty`: Oxc TypeScript type node.
///
/// Output:
/// - `Ok(TsTypeRef)` for the supported first DOM slice.
/// - `Err(TsParseError)` for unsupported shapes.
///
/// Transformation:
/// - Reuses the existing neutral `TsTypeRef` vocabulary so parsing and Terlan
///   type mapping share one generator-owned contract.
fn parse_type(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    match ty {
        TSType::TSStringKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::String)),
        TSType::TSNumberKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Number)),
        TSType::TSBooleanKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Boolean)),
        TSType::TSVoidKeyword(_) => Ok(TsTypeRef::Primitive(TsPrimitiveType::Void)),
        TSType::TSNullKeyword(_) => Ok(TsTypeRef::Null),
        TSType::TSUndefinedKeyword(_) => Ok(TsTypeRef::Undefined),
        TSType::TSAnyKeyword(_) => Ok(TsTypeRef::Any),
        TSType::TSUnknownKeyword(_) => Ok(TsTypeRef::Unknown),
        TSType::TSObjectKeyword(_) => Ok(TsTypeRef::Object),
        TSType::TSFunctionType(function) => {
            let params = function
                .params
                .items
                .iter()
                .map(parse_parameter)
                .map(|param| param.map(|param| param.ty))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Callback {
                params,
                return_type: Box::new(parse_type(&function.return_type.type_annotation)?),
            })
        }
        TSType::TSArrayType(array) => {
            Ok(TsTypeRef::Array(Box::new(parse_type(&array.element_type)?)))
        }
        TSType::TSUnionType(union) => {
            let items = union
                .types
                .iter()
                .map(parse_type)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(TsTypeRef::Union(items))
        }
        TSType::TSLiteralType(literal) => parse_literal_type(&literal.literal),
        TSType::TSTypeReference(reference) => parse_type_reference(reference),
        TSType::TSParenthesizedType(parenthesized) => parse_type(&parenthesized.type_annotation),
        TSType::TSTypeLiteral(_) => parse_type_literal(ty),
        TSType::TSBigIntKeyword(_)
        | TSType::TSIntrinsicKeyword(_)
        | TSType::TSNeverKeyword(_)
        | TSType::TSSymbolKeyword(_)
        | TSType::TSConditionalType(_)
        | TSType::TSConstructorType(_)
        | TSType::TSImportType(_)
        | TSType::TSIndexedAccessType(_)
        | TSType::TSInferType(_)
        | TSType::TSIntersectionType(_)
        | TSType::TSMappedType(_)
        | TSType::TSNamedTupleMember(_)
        | TSType::TSTemplateLiteralType(_)
        | TSType::TSThisType(_)
        | TSType::TSTupleType(_)
        | TSType::TSTypeOperatorType(_)
        | TSType::TSTypePredicate(_)
        | TSType::TSTypeQuery(_)
        | TSType::JSDocNullableType(_)
        | TSType::JSDocNonNullableType(_)
        | TSType::JSDocUnknownType(_) => Err(unsupported(
            "ts_bindgen.unsupported_type",
            "TypeScript type shape is outside the first DOM binding slice",
        )),
    }
}

/// Converts an Oxc TypeScript type reference into named or generic form.
///
/// Inputs:
/// - `reference`: Oxc TypeScript type reference.
///
/// Output:
/// - `Ok(TsTypeRef::Named)` when no type arguments exist.
/// - `Ok(TsTypeRef::Generic)` when type arguments are present.
/// - `Err(TsParseError)` when any argument is unsupported.
///
/// Transformation:
/// - Preserves the source constructor name while lowering each type argument
///   through the same neutral type mapper vocabulary.
fn parse_type_reference(
    reference: &oxc_ast::ast::TSTypeReference<'_>,
) -> Result<TsTypeRef, TsParseError> {
    let name = type_name(&reference.type_name)?;
    let Some(type_arguments) = reference.type_arguments.as_deref() else {
        return Ok(TsTypeRef::Named(name));
    };
    let args = type_arguments
        .params
        .iter()
        .map(parse_type)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TsTypeRef::Generic { name, args })
}

/// Converts an Oxc TypeScript object type literal into a neutral record type.
///
/// Inputs:
/// - `ty`: Oxc TypeScript type node expected to be `TSTypeLiteral`.
///
/// Output:
/// - `Ok(TsTypeRef::Record)` for named property signatures.
/// - `Err(TsParseError)` for method, index, call, or construct signatures.
///
/// Transformation:
/// - Keeps anonymous object fields available to the type mapper without
///   treating them as broad dynamic `object`.
fn parse_type_literal(ty: &TSType<'_>) -> Result<TsTypeRef, TsParseError> {
    let TSType::TSTypeLiteral(type_literal) = ty else {
        return Err(unsupported(
            "ts_bindgen.internal_type_literal_mismatch",
            "expected a TypeScript type literal",
        ));
    };

    let fields = type_literal
        .members
        .iter()
        .map(parse_type_literal_field)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TsTypeRef::Record(fields))
}

/// Converts one TypeScript object-literal member into a neutral record field.
///
/// Inputs:
/// - `member`: Oxc TypeScript type-literal signature.
///
/// Output:
/// - `Ok(TsRecordField)` for named property signatures.
/// - `Err(TsParseError)` for callable or indexed object members.
///
/// Transformation:
/// - Preserves field names, optionality, and field type references for record
///   mapping.
fn parse_type_literal_field(member: &TSSignature<'_>) -> Result<TsRecordField, TsParseError> {
    match member {
        TSSignature::TSPropertySignature(property) => Ok(TsRecordField {
            name: property_key_name(&property.key)?,
            optional: property.optional,
            ty: parse_optional_type_annotation(property.type_annotation.as_deref())?,
        }),
        TSSignature::TSIndexSignature(_)
        | TSSignature::TSCallSignatureDeclaration(_)
        | TSSignature::TSConstructSignatureDeclaration(_)
        | TSSignature::TSMethodSignature(_) => Err(unsupported(
            "ts_bindgen.unsupported_record_member",
            "only property members are supported in record type literals",
        )),
    }
}

/// Converts a TypeScript literal type into a neutral type reference.
///
/// Inputs:
/// - `literal`: Oxc TypeScript literal type node.
///
/// Output:
/// - `Ok(TsTypeRef)` for string, number, and boolean literals.
/// - `Err(TsParseError)` for bigint, template, or unary literals.
///
/// Transformation:
/// - Keeps literal unions representable without admitting computed literal
///   expressions.
fn parse_literal_type(literal: &TSLiteral<'_>) -> Result<TsTypeRef, TsParseError> {
    match literal {
        TSLiteral::BooleanLiteral(value) => Ok(TsTypeRef::BooleanLiteral(value.value)),
        TSLiteral::NumericLiteral(value) => Ok(TsTypeRef::NumberLiteral(
            value
                .raw
                .map_or_else(|| value.value.to_string(), |raw| raw.to_string()),
        )),
        TSLiteral::StringLiteral(value) => Ok(TsTypeRef::StringLiteral(value.value.to_string())),
        TSLiteral::BigIntLiteral(_)
        | TSLiteral::TemplateLiteral(_)
        | TSLiteral::UnaryExpression(_) => Err(unsupported(
            "ts_bindgen.unsupported_literal_type",
            "literal type is outside the first DOM binding slice",
        )),
    }
}

/// Returns the source name for a supported property key.
///
/// Inputs:
/// - `key`: Oxc property key from an interface member.
///
/// Output:
/// - `Ok(String)` for static identifier and string-literal keys.
/// - `Err(TsParseError)` for computed or private keys.
///
/// Transformation:
/// - Normalizes property keys before later snake/camel name conversion.
fn property_key_name(key: &PropertyKey<'_>) -> Result<String, TsParseError> {
    match key {
        PropertyKey::StaticIdentifier(identifier) => Ok(identifier.name.to_string()),
        PropertyKey::StringLiteral(literal) => Ok(literal.value.to_string()),
        _ => Err(unsupported(
            "ts_bindgen.unsupported_property_key",
            "only static identifier and string-literal property keys are supported",
        )),
    }
}

/// Returns the source name for a supported TypeScript type name.
///
/// Inputs:
/// - `name`: Oxc TypeScript type name.
///
/// Output:
/// - `Ok(String)` for identifier and qualified names.
/// - `Err(TsParseError)` for `this` type names.
///
/// Transformation:
/// - Preserves namespace qualification as dot-separated text for later wrapper
///   mapping.
fn type_name(name: &TSTypeName<'_>) -> Result<String, TsParseError> {
    match name {
        TSTypeName::IdentifierReference(identifier) => Ok(identifier.name.to_string()),
        TSTypeName::QualifiedName(qualified) => Ok(format!(
            "{}.{}",
            type_name(&qualified.left)?,
            qualified.right.name
        )),
        TSTypeName::ThisExpression(_) => Err(unsupported(
            "ts_bindgen.unsupported_this_type_name",
            "`this` type names are outside the first DOM binding slice",
        )),
    }
}

/// Builds a stable unsupported-shape parser error.
///
/// Inputs:
/// - `reason`: stable reason code.
/// - `message`: human-readable explanation.
///
/// Output:
/// - `TsParseError` carrying both fields.
///
/// Transformation:
/// - Centralizes generator-contract refusal messages so future manifests can
///   reuse the same reason codes.
fn unsupported(reason: &'static str, message: &str) -> TsParseError {
    TsParseError {
        reason,
        message: message.to_string(),
    }
}
