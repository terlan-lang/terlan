use serde::{Deserialize, Serialize};

use super::{
    SyntaxConfigEntryOutput, SyntaxExportItem, SyntaxExprOutput, SyntaxImportItem,
    SyntaxImportKind, SyntaxParamOutput, SyntaxPatternOutput, SyntaxTypeOutput,
};
use crate::ebnf::EbnfSourceSpan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// One declaration in syntax-output form.
///
/// Inputs: parsed declaration plus docs/annotations. Output: stable
/// declaration record. Transformation: attaches index, class, span, docs,
/// annotations, and normalized payload for downstream compiler phases.
pub struct SyntaxDeclarationOutput {
    pub index: usize,
    pub class: String,
    pub span: EbnfSourceSpan,
    pub docs: Vec<String>,
    pub annotations: Vec<SyntaxAnnotationOutput>,
    pub payload: SyntaxDeclarationPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Annotation instance in syntax-output form.
///
/// Inputs: parsed annotation path, args, entries, values, and span. Output:
/// serializable annotation record. Transformation: separates positional values
/// from keyed entries while preserving source span.
pub struct SyntaxAnnotationOutput {
    pub path: Vec<String>,
    pub args: Option<String>,
    pub entries: Vec<SyntaxAnnotationEntryOutput>,
    pub values: Vec<SyntaxAnnotationValueOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Keyed annotation entry.
///
/// Inputs: annotation key path, value, and span. Output: serializable key/value
/// pair. Transformation: keeps dotted keys as segment vectors.
pub struct SyntaxAnnotationEntryOutput {
    pub key: Vec<String>,
    pub value: SyntaxAnnotationValueOutput,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Annotation value represented by the syntax-output contract.
///
/// Inputs: parsed annotation literal or compound value. Output: tagged value
/// payload. Transformation: normalizes names, scalars, lists, and objects into
/// JSON-serializable variants.
pub enum SyntaxAnnotationValueOutput {
    Name {
        segments: Vec<String>,
    },
    Bool {
        value: bool,
    },
    Int {
        text: String,
    },
    Float {
        text: String,
    },
    String {
        text: String,
    },
    List {
        values: Vec<SyntaxAnnotationValueOutput>,
    },
    Object {
        entries: Vec<SyntaxAnnotationEntryOutput>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Declaration-specific syntax-output payload.
///
/// Inputs: parsed declaration variants. Output: tagged declaration payload.
/// Transformation: preserves declaration semantics while removing parser-only
/// enum shapes from the compiler handoff contract.
pub enum SyntaxDeclarationPayload {
    Import {
        import_kind: SyntaxImportKind,
        module_name: String,
        items: Vec<SyntaxImportItem>,
        is_type: bool,
        is_selected: bool,
        source_path: Option<String>,
    },
    Export {
        items: Vec<SyntaxExportItem>,
    },
    Type {
        name: String,
        params: Vec<String>,
        is_public: bool,
        is_opaque: bool,
        implements: Vec<SyntaxTypeOutput>,
        variants: Vec<SyntaxTypeOutput>,
    },
    Struct {
        name: String,
        includes: Vec<String>,
        implements: Vec<SyntaxTypeOutput>,
        is_public: bool,
        fields: Vec<SyntaxStructFieldOutput>,
    },
    Constructor {
        name: String,
        params: Vec<String>,
        is_public: bool,
        clauses: Vec<SyntaxConstructorClauseOutput>,
    },
    Function {
        name: String,
        generic_params: Vec<String>,
        params: Vec<SyntaxParamOutput>,
        return_type: SyntaxTypeOutput,
        is_public: bool,
        is_macro: bool,
        generic_bounds: Vec<String>,
        clauses: Vec<SyntaxFunctionClauseOutput>,
    },
    Method {
        receiver: SyntaxParamOutput,
        name: String,
        generic_params: Vec<String>,
        params: Vec<SyntaxParamOutput>,
        return_type: SyntaxTypeOutput,
        is_public: bool,
        generic_bounds: Vec<String>,
        clauses: Vec<SyntaxFunctionClauseOutput>,
    },
    Trait {
        name: String,
        params: Vec<String>,
        super_traits: Vec<String>,
        is_public: bool,
        methods: Vec<SyntaxTraitMethodOutput>,
    },
    TraitImpl {
        trait_ref: SyntaxTypeOutput,
        for_type: SyntaxTypeOutput,
        is_public: bool,
        methods: Vec<SyntaxImplMethodOutput>,
    },
    AnnotationSchema {
        path: Vec<String>,
        is_public: bool,
        entries: Vec<SyntaxAnnotationSchemaEntryOutput>,
    },
    Template {
        name: String,
        source_path: String,
        props: Vec<SyntaxTemplatePropOutput>,
    },
    Config {
        name: String,
        target: String,
        text: String,
        entries: Vec<SyntaxConfigEntryOutput>,
    },
    Raw {
        raw_kind: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Annotation schema entry in syntax-output form.
///
/// Inputs: parsed annotation schema declaration entry. Output: applies-to or
/// key definition payload. Transformation: keeps schema metadata structured for
/// compile-time annotation validation.
pub enum SyntaxAnnotationSchemaEntryOutput {
    AppliesTo {
        targets: Vec<String>,
        span: EbnfSourceSpan,
    },
    Key {
        key: Vec<String>,
        value_type: String,
        options: Vec<SyntaxAnnotationKeyOptionOutput>,
        span: EbnfSourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Annotation key option in syntax-output form.
///
/// Inputs: parsed option attached to an annotation schema key. Output:
/// required/repeatable/default/applies-to payload. Transformation: carries
/// option spans so schema diagnostics can point at the exact option.
pub enum SyntaxAnnotationKeyOptionOutput {
    Required {
        value: bool,
        span: EbnfSourceSpan,
    },
    Repeatable {
        value: bool,
        span: EbnfSourceSpan,
    },
    Default {
        value: SyntaxAnnotationValueOutput,
        span: EbnfSourceSpan,
    },
    AppliesTo {
        targets: Vec<String>,
        span: EbnfSourceSpan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Struct field represented by syntax output.
///
/// Inputs: parsed field declaration, docs, default expression, and span.
/// Output: field record. Transformation: preserves type annotation and optional
/// default as syntax-output data.
pub struct SyntaxStructFieldOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub docs: Vec<String>,
    pub has_default: bool,
    #[serde(default)]
    pub default: Option<SyntaxExprOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Constructor parameter represented by syntax output.
///
/// Inputs: parsed constructor parameter. Output: parameter record with optional
/// default and varargs flag. Transformation: keeps source type annotation and
/// default expression in normalized syntax-output form plus source-like default
/// text for generated summaries.
pub struct SyntaxConstructorParamOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    pub has_default: bool,
    #[serde(default)]
    pub default: Option<SyntaxExprOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    pub is_varargs: bool,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Constructor clause represented by syntax output.
///
/// Inputs: parsed constructor clause. Output: params, return type, body, body
/// text, and span. Transformation: stores both structured body and source-like
/// text for diagnostics/contracts.
pub struct SyntaxConstructorClauseOutput {
    pub params: Vec<SyntaxConstructorParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub body: SyntaxExprOutput,
    pub body_text: String,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Function clause represented by syntax output.
///
/// Inputs: parsed function clause. Output: patterns, optional guard, body, and
/// span. Transformation: normalizes guard presence and expression/pattern
/// payloads for typechecking.
pub struct SyntaxFunctionClauseOutput {
    pub patterns: Vec<SyntaxPatternOutput>,
    pub guard: Option<SyntaxExprOutput>,
    pub body: SyntaxExprOutput,
    pub has_guard: bool,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Trait method declaration represented in syntax output.
///
/// Inputs: parsed trait method. Output: signature, bounds, optional default
/// body, visibility, docs, and span. Transformation: normalizes default methods
/// into expression output while preserving signature text.
pub struct SyntaxTraitMethodOutput {
    pub name: String,
    #[serde(default)]
    pub generic_params: Vec<String>,
    pub params: Vec<SyntaxParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub generic_bounds: Vec<String>,
    #[serde(default)]
    pub default_body: Option<SyntaxExprOutput>,
    pub is_public: bool,
    pub docs: Vec<String>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Explicit trait implementation method represented in syntax output.
///
/// Inputs: parsed impl method. Output: signature and clauses. Transformation:
/// normalizes implementation clauses for typechecking and trait conformance.
pub struct SyntaxImplMethodOutput {
    pub name: String,
    pub params: Vec<SyntaxParamOutput>,
    pub return_type: SyntaxTypeOutput,
    pub generic_bounds: Vec<String>,
    pub clauses: Vec<SyntaxFunctionClauseOutput>,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Template property represented in syntax output.
///
/// Inputs: parsed template property. Output: name, annotation, optional
/// default, and span. Transformation: keeps template property type/default
/// metadata structured for template typechecking and lowering.
pub struct SyntaxTemplatePropOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    #[serde(default, skip_serializing_if = "template_prop_bool_is_false")]
    pub has_default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<SyntaxExprOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<String>,
    pub span: EbnfSourceSpan,
}

/// Returns whether a template-property boolean should be omitted from JSON.
///
/// Inputs: `value` is a syntax-output boolean flag. Output: `true` for
/// compact false flags. Transformation: mirrors the parameter-output
/// serialization helper while keeping this root module self-contained.
fn template_prop_bool_is_false(value: &bool) -> bool {
    !*value
}
