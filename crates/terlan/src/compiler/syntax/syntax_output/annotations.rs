use super::*;

/// Validates compiler-known annotation schemas before syntax output is returned.
///
/// Inputs:
/// - `declarations`: structured syntax declarations with parsed annotations.
///
/// Output:
/// - `Ok(())` when all compiler-known annotations satisfy their built-in schema.
/// - `EbnfCompileError::Parse` at the offending annotation or entry span.
///
/// Transformation:
/// - Applies early metadata validation for built-in annotation paths so invalid
///   compiler metadata stops before semantic lowering, CoreIR, or backends.
pub(super) fn validate_builtin_annotation_schemas(
    declarations: &[SyntaxDeclarationOutput],
) -> EbnfCompileResult<()> {
    let user_schemas = collect_user_annotation_schemas(declarations);
    for declaration in declarations {
        for annotation in &declaration.annotations {
            if annotation_path_matches(annotation, &["test"]) {
                validate_marker_annotation(annotation, declaration, &["FunctionDecl"], "@test")?;
            } else if annotation_path_matches(annotation, &["compiler", "inline"]) {
                validate_marker_annotation(
                    annotation,
                    declaration,
                    &[
                        "TypeDecl",
                        "OpaqueTypeDecl",
                        "StructDecl",
                        "ConstructorDecl",
                        "FunctionDecl",
                        "MethodDecl",
                        "TraitDecl",
                        "TraitImplDecl",
                        "TemplateDecl",
                    ],
                    "@compiler.inline",
                )?;
            } else if annotation_path_matches(annotation, &["compiler", "intrinsic"]) {
                validate_compiler_key_annotation(annotation, declaration, "@compiler.intrinsic")?;
            } else if annotation_path_matches(annotation, &["compiler", "native"]) {
                validate_compiler_key_annotation(annotation, declaration, "@compiler.native")?;
            } else if annotation_path_matches(annotation, &["target", "erlang"]) {
                validate_target_erlang_annotation(annotation, declaration)?;
            } else if annotation_path_matches(annotation, &["target", "js"]) {
                validate_target_js_annotation(annotation, declaration)?;
            } else if annotation_path_matches(annotation, &["native"]) {
                validate_native_annotation(annotation, declaration)?;
            } else if let Some(schema) = user_schemas.get(&annotation.path.join(".")) {
                validate_user_annotation_schema(annotation, declaration, schema)?;
            }
        }
    }

    Ok(())
}

/// Collects user-declared annotation schemas from syntax declarations.
///
/// Inputs:
/// - `declarations`: module declarations after syntax-output payload routing.
///
/// Output:
/// - Map from dotted annotation path to schema payload.
///
/// Transformation:
/// - Indexes `AnnotationSchemaDecl` payloads so declaration-leading annotations
///   can be validated without reparsing source text.
fn collect_user_annotation_schemas<'a>(
    declarations: &'a [SyntaxDeclarationOutput],
) -> std::collections::BTreeMap<String, &'a SyntaxDeclarationPayload> {
    let mut schemas = std::collections::BTreeMap::new();
    for declaration in declarations {
        if let SyntaxDeclarationPayload::AnnotationSchema { path, .. } = &declaration.payload {
            schemas.insert(path.join("."), &declaration.payload);
        }
    }
    schemas
}

/// Validates one annotation against a user-declared schema.
///
/// Inputs:
/// - `annotation`: declaration-leading annotation metadata.
/// - `declaration`: annotated declaration.
/// - `schema`: matching `AnnotationSchemaDecl` payload.
///
/// Output:
/// - `Ok(())` when target, keys, required options, repeatability, and value
///   types satisfy the schema.
///
/// Transformation:
/// - Applies user-authored schema metadata as an early compile-time contract
///   before semantic lowering, CoreIR, or backend emission.
fn validate_user_annotation_schema(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
    schema: &SyntaxDeclarationPayload,
) -> EbnfCompileResult<()> {
    let SyntaxDeclarationPayload::AnnotationSchema { path, entries, .. } = schema else {
        return Ok(());
    };
    let label = format!("@{}", path.join("."));
    validate_user_annotation_target(annotation, declaration, entries, &label)?;

    if !annotation.values.is_empty() {
        return annotation_error(
            annotation,
            format!("{label} does not accept positional metadata"),
        );
    }

    let key_schemas = entries
        .iter()
        .filter_map(|entry| match entry {
            SyntaxAnnotationSchemaEntryOutput::Key {
                key,
                value_type,
                options,
                ..
            } => Some((key.join("."), value_type.as_str(), options.as_slice())),
            _ => None,
        })
        .collect::<Vec<_>>();

    if key_schemas.is_empty() {
        if has_annotation_metadata(annotation) {
            return annotation_error(annotation, format!("{label} does not accept metadata"));
        }
        return Ok(());
    }

    for (key, _, options) in &key_schemas {
        if schema_key_is_required(options)
            && !annotation
                .entries
                .iter()
                .any(|entry| entry.key_text() == *key)
        {
            return annotation_error(annotation, format!("{label} missing required key `{key}`"));
        }
    }

    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for entry in &annotation.entries {
        let key = entry.key_text();
        let Some((_, value_type, options)) = key_schemas
            .iter()
            .find(|(schema_key, _, _)| schema_key == &key)
        else {
            return annotation_entry_error(entry, format!("{label} has unknown key `{key}`"));
        };

        validate_user_annotation_key_target(entry, declaration, options, &label)?;
        if !annotation_value_matches_type(&entry.value, value_type) {
            return annotation_entry_error(
                entry,
                format!("annotation key `{key}` expects {value_type}"),
            );
        }

        let count = seen.entry(key.clone()).or_insert(0);
        *count += 1;
        if *count > 1 && !schema_key_is_repeatable(options) {
            return annotation_entry_error(entry, format!("{label} key `{key}` is not repeatable"));
        }
    }

    Ok(())
}

/// Validates declaration target restrictions for a user schema.
///
/// Inputs:
/// - `annotation`: annotation being validated.
/// - `declaration`: annotated declaration.
/// - `entries`: schema entries.
/// - `label`: user-facing annotation label.
///
/// Output:
/// - `Ok(())` when the declaration target is allowed.
///
/// Transformation:
/// - Converts schema target names such as `function` into declaration-class
///   checks such as `FunctionDecl`.
fn validate_user_annotation_target(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
    entries: &[SyntaxAnnotationSchemaEntryOutput],
    label: &str,
) -> EbnfCompileResult<()> {
    let Some(targets) = entries.iter().find_map(|entry| match entry {
        SyntaxAnnotationSchemaEntryOutput::AppliesTo { targets, .. } => Some(targets),
        _ => None,
    }) else {
        return Ok(());
    };
    if targets
        .iter()
        .any(|target| annotation_target_matches_declaration(target, declaration))
    {
        return Ok(());
    }
    annotation_error(
        annotation,
        format!("{label} cannot annotate {}", declaration.class),
    )
}

/// Validates key-level target restrictions for a user schema.
///
/// Inputs:
/// - `entry`: annotation key entry being validated.
/// - `declaration`: annotated declaration.
/// - `options`: schema options for the key.
/// - `label`: user-facing annotation label.
///
/// Output:
/// - `Ok(())` when the key is valid for the declaration target.
///
/// Transformation:
/// - Applies key-specific `applies_to` restrictions after top-level schema
///   target validation.
fn validate_user_annotation_key_target(
    entry: &SyntaxAnnotationEntryOutput,
    declaration: &SyntaxDeclarationOutput,
    options: &[SyntaxAnnotationKeyOptionOutput],
    label: &str,
) -> EbnfCompileResult<()> {
    let Some(targets) = options.iter().find_map(|option| match option {
        SyntaxAnnotationKeyOptionOutput::AppliesTo { targets, .. } => Some(targets),
        _ => None,
    }) else {
        return Ok(());
    };
    if targets
        .iter()
        .any(|target| annotation_target_matches_declaration(target, declaration))
    {
        return Ok(());
    }
    annotation_entry_error(
        entry,
        format!(
            "{label} key `{}` cannot annotate {}",
            entry.key_text(),
            declaration.class
        ),
    )
}

/// Returns whether a schema target name matches a declaration class.
///
/// Inputs:
/// - `target`: schema target spelling.
/// - `declaration`: annotated declaration.
///
/// Output:
/// - `true` when the target permits the declaration class.
///
/// Transformation:
/// - Maps the public schema target vocabulary to formal parser-contract class
///   names.
fn annotation_target_matches_declaration(
    target: &str,
    declaration: &SyntaxDeclarationOutput,
) -> bool {
    matches!(
        (target, declaration.class.as_str()),
        ("module", "ModuleDecl")
            | ("import", "ImportDecl")
            | ("type", "TypeDecl")
            | ("opaque_type", "OpaqueTypeDecl")
            | ("struct", "StructDecl")
            | ("constructor", "ConstructorDecl")
            | ("trait", "TraitDecl")
            | ("impl", "TraitImplDecl")
            | ("function", "FunctionDecl")
            | ("method", "MethodDecl")
            | ("template", "TemplateDecl")
            | ("config", "ConfigDecl")
    )
}

/// Returns whether a schema key is required.
///
/// Inputs:
/// - `options`: key options from an annotation schema entry.
///
/// Output:
/// - `true` when `required: true` is present.
///
/// Transformation:
/// - Interprets omitted `required` as false.
fn schema_key_is_required(options: &[SyntaxAnnotationKeyOptionOutput]) -> bool {
    options.iter().any(|option| {
        matches!(
            option,
            SyntaxAnnotationKeyOptionOutput::Required { value: true, .. }
        )
    })
}

/// Returns whether a schema key is repeatable.
///
/// Inputs:
/// - `options`: key options from an annotation schema entry.
///
/// Output:
/// - `true` when `repeatable: true` is present.
///
/// Transformation:
/// - Interprets omitted `repeatable` as false.
fn schema_key_is_repeatable(options: &[SyntaxAnnotationKeyOptionOutput]) -> bool {
    options.iter().any(|option| {
        matches!(
            option,
            SyntaxAnnotationKeyOptionOutput::Repeatable { value: true, .. }
        )
    })
}

/// Returns whether an annotation value satisfies a schema value type.
///
/// Inputs:
/// - `value`: parsed annotation value.
/// - `value_type`: schema value-type text.
///
/// Output:
/// - `true` when the current schema type supports the value shape.
///
/// Transformation:
/// - Checks primitive value kinds, name/type references, list types, and object
///   types using syntax-output metadata rather than source text.
fn annotation_value_matches_type(value: &SyntaxAnnotationValueOutput, value_type: &str) -> bool {
    match value_type {
        "Bool" => matches!(value, SyntaxAnnotationValueOutput::Bool { .. }),
        "Int" => matches!(value, SyntaxAnnotationValueOutput::Int { .. }),
        "Float" => matches!(value, SyntaxAnnotationValueOutput::Float { .. }),
        "String" => matches!(value, SyntaxAnnotationValueOutput::String { .. }),
        "Name" | "Type" => matches!(value, SyntaxAnnotationValueOutput::Name { .. }),
        text if text.starts_with('[') && text.ends_with(']') => {
            let inner = text[1..text.len() - 1].trim();
            matches!(value, SyntaxAnnotationValueOutput::List { values }
                if values.iter().all(|item| annotation_value_matches_type(item, inner)))
        }
        text if text.starts_with('{') && text.ends_with('}') => {
            matches!(value, SyntaxAnnotationValueOutput::Object { .. })
        }
        _ => matches!(value, SyntaxAnnotationValueOutput::Name { .. }),
    }
}

/// Returns whether an annotation path matches a static schema path.
///
/// Inputs:
/// - `annotation`: parsed annotation with owned path segments.
/// - `path`: expected schema path segments.
///
/// Output:
/// - `true` when both paths contain the same segments in the same order.
///
/// Transformation:
/// - Compares owned compiler output strings against borrowed schema strings
///   without allocating a temporary path.
fn annotation_path_matches(annotation: &SyntaxAnnotationOutput, path: &[&str]) -> bool {
    annotation.path.len() == path.len()
        && annotation
            .path
            .iter()
            .zip(path.iter())
            .all(|(actual, expected)| actual == expected)
}

/// Validates a marker-only annotation.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `declaration`: annotated declaration.
/// - `targets`: allowed declaration classes.
/// - `label`: user-facing annotation spelling.
///
/// Output:
/// - `Ok(())` when the annotation has no metadata and targets a legal
///   declaration class.
///
/// Transformation:
/// - Rejects metadata and invalid targets for annotations whose presence alone
///   carries all required meaning.
fn validate_marker_annotation(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
    targets: &[&str],
    label: &str,
) -> EbnfCompileResult<()> {
    validate_annotation_target(annotation, declaration, targets, label)?;
    if has_annotation_metadata(annotation) {
        return annotation_error(annotation, format!("{label} does not accept metadata"));
    }
    Ok(())
}

/// Validates compiler annotations that may optionally carry one explicit key.
///
/// Inputs:
/// - `annotation`: parsed `@compiler.intrinsic` or `@compiler.native` metadata.
/// - `declaration`: annotated declaration.
/// - `label`: user-facing annotation spelling.
///
/// Output:
/// - `Ok(())` when the annotation is marker-only or carries one positional
///   qualified-name value.
///
/// Transformation:
/// - Keeps release source marker-based while preserving a narrow explicit-key
///   escape hatch for scratch/compiler tests.
fn validate_compiler_key_annotation(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
    label: &str,
) -> EbnfCompileResult<()> {
    validate_annotation_target(
        annotation,
        declaration,
        &["FunctionDecl", "MethodDecl"],
        label,
    )?;
    if !annotation.entries.is_empty() {
        return annotation_error(
            annotation,
            format!("{label} does not accept keyed metadata"),
        );
    }
    if annotation.values.len() > 1 {
        return annotation_error(
            annotation,
            format!("{label} accepts at most one metadata value"),
        );
    }
    if let Some(value) = annotation.values.first() {
        if !matches!(value, SyntaxAnnotationValueOutput::Name { .. }) {
            return annotation_error(
                annotation,
                format!("{label} metadata must be a qualified name"),
            );
        }
    }
    Ok(())
}

/// Validates `@target.erlang` annotation metadata.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `declaration`: annotated declaration.
///
/// Output:
/// - `Ok(())` when entries use known keys and valid value types.
///
/// Transformation:
/// - Rejects positional metadata, unknown keys, duplicate keys, and invalid
///   value types for the Erlang target-owned annotation surface.
fn validate_target_erlang_annotation(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
) -> EbnfCompileResult<()> {
    validate_annotation_target(
        annotation,
        declaration,
        &[
            "TypeDecl",
            "OpaqueTypeDecl",
            "StructDecl",
            "ConstructorDecl",
            "FunctionDecl",
            "MethodDecl",
            "TraitDecl",
            "TraitImplDecl",
            "TemplateDecl",
            "ConfigDecl",
        ],
        "@target.erlang",
    )?;
    if !annotation.values.is_empty() {
        return annotation_error(
            annotation,
            "@target.erlang does not accept positional metadata",
        );
    }
    validate_unique_annotation_keys(annotation, "@target.erlang")?;
    for entry in &annotation.entries {
        match entry.key_text().as_str() {
            "otp_application" => {
                validate_annotation_entry_value(entry, annotation_value_is_bool, "Bool")?
            }
            "process_mailbox" => validate_annotation_entry_value(
                entry,
                annotation_value_is_bool_name_or_string,
                "Bool, name, or String",
            )?,
            key => {
                return annotation_entry_error(
                    entry,
                    format!("@target.erlang has unknown key `{key}`"),
                );
            }
        }
    }
    Ok(())
}

/// Validates `@target.js` annotation metadata.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `declaration`: annotated declaration.
///
/// Output:
/// - `Ok(())` when entries use known JS target metadata keys and value types.
///
/// Transformation:
/// - Enforces the generated-binding metadata surface before CoreIR or JS
///   backend emission, keeping ordinary snake-case to camel-case mapping
///   convention-based and reserving annotations for explicit exceptions.
fn validate_target_js_annotation(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
) -> EbnfCompileResult<()> {
    validate_annotation_target(
        annotation,
        declaration,
        &[
            "TypeDecl",
            "OpaqueTypeDecl",
            "StructDecl",
            "ConstructorDecl",
            "FunctionDecl",
            "MethodDecl",
            "TraitDecl",
            "TraitImplDecl",
            "TemplateDecl",
            "ConfigDecl",
        ],
        "@target.js",
    )?;
    if !annotation.values.is_empty() {
        return annotation_error(annotation, "@target.js does not accept positional metadata");
    }
    validate_unique_annotation_keys(annotation, "@target.js")?;
    for entry in &annotation.entries {
        match entry.key_text().as_str() {
            "name" | "source_module" | "namespace" => validate_annotation_entry_value(
                entry,
                annotation_value_is_name_or_string,
                "name or String",
            )?,
            "profile" => validate_annotation_entry_value(
                entry,
                annotation_value_is_name_or_string,
                "name or String",
            )?,
            "global" => validate_annotation_entry_value(entry, annotation_value_is_bool, "Bool")?,
            key => {
                return annotation_entry_error(
                    entry,
                    format!("@target.js has unknown key `{key}`"),
                );
            }
        }
    }
    Ok(())
}

/// Validates `@native` annotation metadata.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `declaration`: annotated declaration.
///
/// Output:
/// - `Ok(())` when entries use known keys and valid value types.
///
/// Transformation:
/// - Models native metadata as explicit typed key/value configuration, not as
///   positional compiler-internal names.
fn validate_native_annotation(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
) -> EbnfCompileResult<()> {
    validate_annotation_target(
        annotation,
        declaration,
        &[
            "TypeDecl",
            "OpaqueTypeDecl",
            "StructDecl",
            "FunctionDecl",
            "MethodDecl",
        ],
        "@native",
    )?;
    if !annotation.values.is_empty() {
        return annotation_error(annotation, "@native does not accept positional metadata");
    }
    if annotation.entries.is_empty() {
        return annotation_error(annotation, "@native requires metadata entries");
    }
    validate_unique_annotation_keys(annotation, "@native")?;
    for entry in &annotation.entries {
        match entry.key_text().as_str() {
            "adapter" | "runtime" => {
                validate_annotation_entry_value(entry, annotation_value_is_string, "String")?
            }
            "worker" => validate_annotation_entry_value(entry, annotation_value_is_bool, "Bool")?,
            key => {
                return annotation_entry_error(entry, format!("@native has unknown key `{key}`"))
            }
        }
    }
    Ok(())
}

/// Validates that an annotation is applied to an allowed declaration class.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `declaration`: annotated declaration.
/// - `targets`: allowed declaration classes.
/// - `label`: user-facing annotation spelling.
///
/// Output:
/// - `Ok(())` when `declaration.class` is present in `targets`.
///
/// Transformation:
/// - Converts declaration class routing into a compile-time annotation target
///   schema check.
fn validate_annotation_target(
    annotation: &SyntaxAnnotationOutput,
    declaration: &SyntaxDeclarationOutput,
    targets: &[&str],
    label: &str,
) -> EbnfCompileResult<()> {
    if targets
        .iter()
        .any(|target| *target == declaration.class.as_str())
    {
        return Ok(());
    }
    annotation_error(
        annotation,
        format!("{label} cannot annotate {}", declaration.class),
    )
}

/// Returns whether an annotation carries any metadata.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
///
/// Output:
/// - `true` when raw args, keyed entries, or positional values are present.
///
/// Transformation:
/// - Collapses raw and typed metadata forms into one marker-vs-metadata check.
fn has_annotation_metadata(annotation: &SyntaxAnnotationOutput) -> bool {
    annotation.args.is_some() || !annotation.entries.is_empty() || !annotation.values.is_empty()
}

/// Rejects duplicate non-repeatable annotation keys.
///
/// Inputs:
/// - `annotation`: parsed annotation metadata.
/// - `label`: user-facing annotation spelling.
///
/// Output:
/// - `Ok(())` when every key appears at most once.
///
/// Transformation:
/// - Treats all current built-in annotation keys as non-repeatable.
fn validate_unique_annotation_keys(
    annotation: &SyntaxAnnotationOutput,
    label: &str,
) -> EbnfCompileResult<()> {
    let mut seen = std::collections::BTreeSet::new();
    for entry in &annotation.entries {
        let key = entry.key_text();
        if !seen.insert(key.clone()) {
            return annotation_entry_error(entry, format!("{label} key `{key}` is not repeatable"));
        }
    }
    Ok(())
}

/// Validates one annotation entry value with a predicate.
///
/// Inputs:
/// - `entry`: parsed annotation entry.
/// - `predicate`: accepted value-shape test.
/// - `expected`: user-facing expected type text.
///
/// Output:
/// - `Ok(())` when `predicate(entry.value)` is true.
///
/// Transformation:
/// - Centralizes typed metadata diagnostics for built-in annotation schemas.
fn validate_annotation_entry_value(
    entry: &SyntaxAnnotationEntryOutput,
    predicate: fn(&SyntaxAnnotationValueOutput) -> bool,
    expected: &str,
) -> EbnfCompileResult<()> {
    if predicate(&entry.value) {
        return Ok(());
    }
    annotation_entry_error(
        entry,
        format!("annotation key `{}` expects {expected}", entry.key_text()),
    )
}

/// Returns whether an annotation value is `Bool`.
///
/// Inputs:
/// - `value`: parsed annotation value.
///
/// Output:
/// - `true` for typed boolean values.
///
/// Transformation:
/// - Matches the annotation schema value kind without reading raw source text.
fn annotation_value_is_bool(value: &SyntaxAnnotationValueOutput) -> bool {
    matches!(value, SyntaxAnnotationValueOutput::Bool { .. })
}

/// Returns whether an annotation value is `String`.
///
/// Inputs:
/// - `value`: parsed annotation value.
///
/// Output:
/// - `true` for typed string values.
///
/// Transformation:
/// - Matches the annotation schema value kind without reading raw source text.
fn annotation_value_is_string(value: &SyntaxAnnotationValueOutput) -> bool {
    matches!(value, SyntaxAnnotationValueOutput::String { .. })
}

/// Returns whether an annotation value is a qualified name or `String`.
///
/// Inputs:
/// - `value`: parsed annotation value.
///
/// Output:
/// - `true` for typed qualified-name and string values.
///
/// Transformation:
/// - Supports target metadata that may refer to symbolic platform names or
///   string spellings without accepting unrelated scalar types.
fn annotation_value_is_name_or_string(value: &SyntaxAnnotationValueOutput) -> bool {
    matches!(
        value,
        SyntaxAnnotationValueOutput::Name { .. } | SyntaxAnnotationValueOutput::String { .. }
    )
}

/// Returns whether an annotation value is `Bool`, name, or `String`.
///
/// Inputs:
/// - `value`: parsed annotation value.
///
/// Output:
/// - `true` for typed boolean, qualified-name, or string values.
///
/// Transformation:
/// - Allows target-owned enum-like values while preserving type validation.
fn annotation_value_is_bool_name_or_string(value: &SyntaxAnnotationValueOutput) -> bool {
    matches!(
        value,
        SyntaxAnnotationValueOutput::Bool { .. }
            | SyntaxAnnotationValueOutput::Name { .. }
            | SyntaxAnnotationValueOutput::String { .. }
    )
}

/// Builds a syntax-output parse diagnostic for an annotation.
///
/// Inputs:
/// - `annotation`: offending annotation.
/// - `message`: user-facing diagnostic message.
///
/// Output:
/// - `EbnfCompileError::Parse` at the annotation span.
///
/// Transformation:
/// - Converts syntax-output validation failures into the existing parser error
///   channel used by command phases.
fn annotation_error<T>(
    annotation: &SyntaxAnnotationOutput,
    message: impl Into<String>,
) -> EbnfCompileResult<T> {
    Err(EbnfCompileError::Parse(
        message.into(),
        annotation.span.into(),
    ))
}

/// Builds a syntax-output parse diagnostic for an annotation entry.
///
/// Inputs:
/// - `entry`: offending annotation entry.
/// - `message`: user-facing diagnostic message.
///
/// Output:
/// - `EbnfCompileError::Parse` at the entry span.
///
/// Transformation:
/// - Reports key/value schema failures at the most specific typed metadata
///   span currently available.
fn annotation_entry_error<T>(
    entry: &SyntaxAnnotationEntryOutput,
    message: impl Into<String>,
) -> EbnfCompileResult<T> {
    Err(EbnfCompileError::Parse(message.into(), entry.span.into()))
}

impl SyntaxAnnotationEntryOutput {
    /// Returns a dotted annotation key string.
    ///
    /// Inputs:
    /// - `self`: typed annotation entry with one or more key segments.
    ///
    /// Output:
    /// - Dotted key path such as `otp_application`.
    ///
    /// Transformation:
    /// - Joins structured key segments for schema lookup and diagnostics.
    fn key_text(&self) -> String {
        self.key.join(".")
    }
}

/// Converts parser annotation metadata into serializable syntax output.
///
/// Inputs:
/// - `annotation`: parsed declaration-leading annotation metadata.
///
/// Output:
/// - Syntax-output annotation payload with path, optional raw args, typed
///   entries, and span.
///
/// Transformation:
/// - Clones parser-owned annotation fields into the formal output schema so
///   downstream phases can inspect annotations without reading source text.
pub(super) fn annotation_output(annotation: &Annotation) -> SyntaxAnnotationOutput {
    SyntaxAnnotationOutput {
        path: annotation.path.clone(),
        args: annotation.args.clone(),
        entries: annotation
            .entries
            .iter()
            .map(annotation_entry_output)
            .collect(),
        values: annotation
            .values
            .iter()
            .map(annotation_value_output)
            .collect(),
        span: annotation.span.into(),
    }
}

/// Converts one parser annotation entry into serializable syntax output.
///
/// Inputs:
/// - `entry`: parsed annotation key/value metadata.
///
/// Output:
/// - Serializable annotation entry with source span.
///
/// Transformation:
/// - Clones key segments and recursively converts the typed annotation value.
fn annotation_entry_output(entry: &AnnotationEntry) -> SyntaxAnnotationEntryOutput {
    SyntaxAnnotationEntryOutput {
        key: entry.key.clone(),
        value: annotation_value_output(&entry.value),
        span: entry.span.into(),
    }
}

/// Converts one parser annotation value into serializable syntax output.
///
/// Inputs:
/// - `value`: parsed annotation metadata value.
///
/// Output:
/// - Serializable annotation value payload.
///
/// Transformation:
/// - Preserves literal source text for numeric/string values and recursively
///   maps list/object values.
fn annotation_value_output(value: &AnnotationValue) -> SyntaxAnnotationValueOutput {
    match value {
        AnnotationValue::Name(segments) => SyntaxAnnotationValueOutput::Name {
            segments: segments.clone(),
        },
        AnnotationValue::Bool(value) => SyntaxAnnotationValueOutput::Bool { value: *value },
        AnnotationValue::Int(text) => SyntaxAnnotationValueOutput::Int { text: text.clone() },
        AnnotationValue::Float(text) => SyntaxAnnotationValueOutput::Float { text: text.clone() },
        AnnotationValue::String(text) => SyntaxAnnotationValueOutput::String { text: text.clone() },
        AnnotationValue::List(values) => SyntaxAnnotationValueOutput::List {
            values: values.iter().map(annotation_value_output).collect(),
        },
        AnnotationValue::Object(entries) => SyntaxAnnotationValueOutput::Object {
            entries: entries.iter().map(annotation_entry_output).collect(),
        },
    }
}

/// Converts one parser annotation schema entry into syntax output.
///
/// Inputs:
/// - `entry`: parsed schema body entry.
///
/// Output:
/// - Serializable schema entry preserving targets, key path, value type text,
///   options, and source span.
///
/// Transformation:
/// - Maps parse tree schema entries into the formal syntax-output representation used
///   by validation and downstream compiler phases.
pub(super) fn annotation_schema_entry_output(
    entry: &AnnotationSchemaEntry,
) -> SyntaxAnnotationSchemaEntryOutput {
    match entry {
        AnnotationSchemaEntry::AppliesTo { targets, span } => {
            SyntaxAnnotationSchemaEntryOutput::AppliesTo {
                targets: targets.clone(),
                span: (*span).into(),
            }
        }
        AnnotationSchemaEntry::Key {
            key,
            value_type,
            options,
            span,
        } => SyntaxAnnotationSchemaEntryOutput::Key {
            key: key.clone(),
            value_type: value_type.text.clone(),
            options: options.iter().map(annotation_key_option_output).collect(),
            span: (*span).into(),
        },
    }
}

/// Converts one parser annotation key option into syntax output.
///
/// Inputs:
/// - `option`: parsed schema key option.
///
/// Output:
/// - Serializable option preserving typed values and source span.
///
/// Transformation:
/// - Reuses annotation value conversion for default metadata and copies target
///   lists for applies-to restrictions.
fn annotation_key_option_output(option: &AnnotationKeyOption) -> SyntaxAnnotationKeyOptionOutput {
    match option {
        AnnotationKeyOption::Required { value, span } => {
            SyntaxAnnotationKeyOptionOutput::Required {
                value: *value,
                span: (*span).into(),
            }
        }
        AnnotationKeyOption::Repeatable { value, span } => {
            SyntaxAnnotationKeyOptionOutput::Repeatable {
                value: *value,
                span: (*span).into(),
            }
        }
        AnnotationKeyOption::Default { value, span } => SyntaxAnnotationKeyOptionOutput::Default {
            value: annotation_value_output(value),
            span: (*span).into(),
        },
        AnnotationKeyOption::AppliesTo { targets, span } => {
            SyntaxAnnotationKeyOptionOutput::AppliesTo {
                targets: targets.clone(),
                span: (*span).into(),
            }
        }
    }
}
