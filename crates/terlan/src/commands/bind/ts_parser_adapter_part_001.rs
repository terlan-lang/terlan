use oxc_ast::{
    ast::{
        BindingPattern, Declaration, FormalParameter, PropertyKey, Statement, TSLiteral,
        TSModuleDeclarationBody, TSSignature, TSType, TSTypeAnnotation, TSTypeName,
        TSTypeParameterDeclaration,
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
    TypeAlias(TsTypeAliasDeclaration),
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

/// Neutral TypeScript type-alias declaration.
///
/// Inputs:
/// - Extracted from a top-level or namespace-local Oxc type alias.
///
/// Output:
/// - Alias namespace, source name, type parameters, and target type reference.
///
/// Transformation:
/// - Keeps namespace aliases, such as Angular's `ng.*` declarations, in the
///   same generator-owned model as interface bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TsTypeAliasDeclaration {
    pub(super) namespace: String,
    pub(super) name: String,
    pub(super) doc: Option<String>,
    pub(super) type_params: Vec<String>,
    pub(super) ty: TsTypeRef,
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
        declarations.extend(parse_top_level_statement(
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
) -> Result<Vec<TsDeclaration>, TsParseError> {
    match statement {
        Statement::TSInterfaceDeclaration(interface) => {
            parse_interface(source, comments, "", interface)
                .map(|declaration| vec![TsDeclaration::Interface(declaration)])
        }
        Statement::VariableDeclaration(variable) => Ok(vec![unsupported_declaration(
            top_level_variable_source(variable),
            "ts_bindgen.unsupported_top_level_variable",
            "top-level variables and constructors are not emitted yet",
        )]),
        Statement::TSTypeAliasDeclaration(alias) => Ok(vec![parse_type_alias_declaration(
            source, comments, "", alias,
        )]),
        Statement::TSEnumDeclaration(enumeration) => Ok(vec![unsupported_declaration(
            enumeration.id.name.as_str(),
            "ts_bindgen.unsupported_top_level_enum",
            "TypeScript enums are not emitted yet",
        )]),
        Statement::TSModuleDeclaration(module) => {
            parse_module_declaration(source, comments, "", module)
        }
        Statement::TSGlobalDeclaration(global) => {
            parse_module_block(source, comments, "", &global.body)
        }
        Statement::TSImportEqualsDeclaration(_) => Ok(vec![unsupported_declaration(
            "import_equals",
            "ts_bindgen.unsupported_top_level_import_equals",
            "TypeScript import-equals declarations are not emitted yet",
        )]),
        Statement::ImportDeclaration(import) => Ok(vec![unsupported_declaration(
            &top_level_import_source(import),
            "ts_bindgen.unsupported_top_level_import",
            "TypeScript imports are not emitted yet",
        )]),
        Statement::ExportAllDeclaration(_)
        | Statement::ExportDefaultDeclaration(_)
        | Statement::ExportNamedDeclaration(_)
        | Statement::TSExportAssignment(_)
        | Statement::TSNamespaceExportDeclaration(_) => Ok(unsupported_declaration(
            "export",
            "ts_bindgen.unsupported_top_level_export",
            "TypeScript exports are not emitted yet",
        ))
        .map(|declaration| vec![declaration]),
        Statement::FunctionDeclaration(function) => Ok(vec![unsupported_declaration(
            function
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("function"),
            "ts_bindgen.unsupported_top_level_function",
            "top-level functions are not emitted yet",
        )]),
        Statement::ClassDeclaration(class) => Ok(vec![unsupported_declaration(
            class
                .id
                .as_ref()
                .map(|id| id.name.as_str())
                .unwrap_or("class"),
            "ts_bindgen.unsupported_top_level_class",
            "classes are not emitted yet",
        )]),
        other => Ok(vec![unsupported_declaration(
            top_level_statement_label(other),
            "ts_bindgen.unsupported_top_level_statement",
            "statement is outside the TypeScript declaration binding surface",
        )]),
    }
}

/// Converts one Oxc declaration inside an exported namespace into neutral declarations.
///
/// Inputs:
/// - `declaration`: Oxc declaration nested under a TypeScript module block.
/// - `namespace`: dot-separated namespace accumulated from parent modules.
///
/// Output:
/// - Supported interfaces and aliases, nested module declarations, or skip rows.
///
/// Transformation:
/// - Preserves real declaration namespaces so `declare global { export
///   namespace ng { type X = Y } }` can generate `*.ng.X` bindings.
fn parse_nested_declaration(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    declaration: &Declaration<'_>,
) -> Result<Vec<TsDeclaration>, TsParseError> {
    match declaration {
        Declaration::TSInterfaceDeclaration(interface) => {
            parse_interface(source, comments, namespace, interface)
                .map(|declaration| vec![TsDeclaration::Interface(declaration)])
        }
        Declaration::TSTypeAliasDeclaration(alias) => Ok(vec![parse_type_alias_declaration(
            source, comments, namespace, alias,
        )]),
        Declaration::TSModuleDeclaration(module) => {
            parse_module_declaration(source, comments, namespace, module)
        }
        Declaration::VariableDeclaration(variable) => Ok(vec![unsupported_declaration(
            &qualified_source(namespace, top_level_variable_source(variable)),
            "ts_bindgen.unsupported_top_level_variable",
            "top-level variables and constructors are not emitted yet",
        )]),
        Declaration::FunctionDeclaration(function) => Ok(vec![unsupported_declaration(
            &qualified_source(
                namespace,
                function
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("function"),
            ),
            "ts_bindgen.unsupported_top_level_function",
            "top-level functions are not emitted yet",
        )]),
        Declaration::ClassDeclaration(class) => Ok(vec![unsupported_declaration(
            &qualified_source(
                namespace,
                class
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("class"),
            ),
            "ts_bindgen.unsupported_top_level_class",
            "classes are not emitted yet",
        )]),
        Declaration::TSEnumDeclaration(enumeration) => Ok(vec![unsupported_declaration(
            &qualified_source(namespace, enumeration.id.name.as_str()),
            "ts_bindgen.unsupported_top_level_enum",
            "TypeScript enums are not emitted yet",
        )]),
        Declaration::TSGlobalDeclaration(global) => {
            parse_module_block(source, comments, namespace, &global.body)
        }
        Declaration::TSImportEqualsDeclaration(_) => Ok(vec![unsupported_declaration(
            &qualified_source(namespace, "import_equals"),
            "ts_bindgen.unsupported_top_level_import_equals",
            "TypeScript import-equals declarations are not emitted yet",
        )]),
    }
}

/// Converts one TypeScript module or namespace declaration.
///
/// Inputs:
/// - `module`: Oxc TypeScript module declaration.
/// - `namespace`: parent namespace accumulated from outer modules.
///
/// Output:
/// - Nested declarations under the combined namespace, or a skip row for empty
///   unsupported ambient modules.
///
/// Transformation:
/// - Recurses through Oxc module blocks and dotted namespace declarations.
fn parse_module_declaration(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    module: &oxc_ast::ast::TSModuleDeclaration<'_>,
) -> Result<Vec<TsDeclaration>, TsParseError> {
    let module_name = top_level_module_source(module);
    let namespace = qualified_source(namespace, module_name);
    let Some(body) = &module.body else {
        return Ok(vec![unsupported_declaration(
            &namespace,
            "ts_bindgen.unsupported_top_level_module",
            "ambient TypeScript modules without bodies are not emitted yet",
        )]);
    };
    match body {
        TSModuleDeclarationBody::TSModuleDeclaration(module) => {
            parse_module_declaration(source, comments, &namespace, module)
        }
        TSModuleDeclarationBody::TSModuleBlock(block) if block.body.is_empty() => {
            Ok(vec![unsupported_declaration(
                &namespace,
                "ts_bindgen.unsupported_top_level_module",
                "empty TypeScript namespaces are not emitted yet",
            )])
        }
        TSModuleDeclarationBody::TSModuleBlock(block) => {
            parse_module_block(source, comments, &namespace, block)
        }
    }
}

/// Converts statements inside one TypeScript namespace block.
///
/// Inputs:
/// - `block`: Oxc TypeScript module block.
/// - `namespace`: dot-separated namespace for declarations inside the block.
///
/// Output:
/// - Flattened neutral declarations under `namespace`.
///
/// Transformation:
/// - Handles exported namespace members by unwrapping their nested declaration
///   while keeping unsupported re-export lists explicit.
fn parse_module_block(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    block: &oxc_ast::ast::TSModuleBlock<'_>,
) -> Result<Vec<TsDeclaration>, TsParseError> {
    let mut declarations = Vec::new();
    for statement in &block.body {
        match statement {
            Statement::ExportNamedDeclaration(export) => {
                if let Some(declaration) = &export.declaration {
                    declarations.extend(parse_nested_declaration(
                        source,
                        comments,
                        namespace,
                        declaration,
                    )?);
                } else {
                    declarations.push(unsupported_declaration(
                        &qualified_source(namespace, "export"),
                        "ts_bindgen.unsupported_top_level_export",
                        "TypeScript export lists are not emitted yet",
                    ));
                }
            }
            Statement::TSInterfaceDeclaration(interface) => {
                declarations.push(TsDeclaration::Interface(parse_interface(
                    source, comments, namespace, interface,
                )?));
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                declarations.push(parse_type_alias_declaration(
                    source, comments, namespace, alias,
                ));
            }
            Statement::TSModuleDeclaration(module) => {
                declarations.extend(parse_module_declaration(
                    source, comments, namespace, module,
                )?);
            }
            other => declarations.extend(parse_top_level_statement(source, comments, other)?),
        }
    }
    Ok(declarations)
}

/// Returns `name` under `namespace` when a namespace is present.
fn qualified_source(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}.{name}")
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

/// Returns a stable source label for an unsupported TypeScript import.
fn top_level_import_source(import: &oxc_ast::ast::ImportDeclaration<'_>) -> String {
    format!("import:{}", import.source.value.as_str())
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
    namespace: &str,
    interface: &oxc_ast::ast::TSInterfaceDeclaration<'_>,
) -> Result<TsInterfaceDeclaration, TsParseError> {
    let members = interface
        .body
        .body
        .iter()
        .map(|member| parse_interface_member(source, comments, member))
        .collect::<Vec<_>>();

    Ok(TsInterfaceDeclaration {
        namespace: namespace.to_string(),
        name: interface.id.name.to_string(),
        doc: leading_jsdoc(source, comments, interface.span),
        type_params: parse_type_parameter_names(interface.type_parameters.as_deref()),
        members,
    })
}

/// Converts one Oxc type alias declaration into the neutral model.
///
/// Inputs:
/// - `alias`: parsed Oxc TypeScript type alias.
/// - `namespace`: namespace accumulated from surrounding module declarations.
///
/// Output:
/// - Neutral alias declaration.
///
/// Transformation:
/// - Preserves alias docs, generic parameter names, and target type shape for
///   generated Terlan type aliases.
fn parse_type_alias(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    alias: &oxc_ast::ast::TSTypeAliasDeclaration<'_>,
) -> Result<TsTypeAliasDeclaration, TsParseError> {
    Ok(TsTypeAliasDeclaration {
        namespace: namespace.to_string(),
        name: alias.id.name.to_string(),
        doc: leading_jsdoc(source, comments, alias.span),
        type_params: parse_type_parameter_names(alias.type_parameters.as_deref()),
        ty: parse_type(&alias.type_annotation)?,
    })
}

/// Converts one type alias into a supported declaration or a skip row.
///
/// Inputs:
/// - `alias`: parsed TypeScript type alias.
/// - `namespace`: namespace accumulated from surrounding declarations.
///
/// Output:
/// - Supported alias declaration when its target type parses.
/// - Unsupported declaration row when the target type is outside the current
///   generator slice.
///
/// Transformation:
/// - Keeps broad `.d.ts` files generating around complex aliases instead of
///   failing the whole binding run.
fn parse_type_alias_declaration(
    source: &str,
    comments: &[Comment],
    namespace: &str,
    alias: &oxc_ast::ast::TSTypeAliasDeclaration<'_>,
) -> TsDeclaration {
    match parse_type_alias(source, comments, namespace, alias) {
        Ok(alias) => TsDeclaration::TypeAlias(alias),
        Err(err) => TsDeclaration::Unsupported(TsUnsupportedDeclaration {
            source: qualified_source(namespace, alias.id.name.as_str()),
            reason: err.reason,
            detail: err.message,
        }),
    }
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
