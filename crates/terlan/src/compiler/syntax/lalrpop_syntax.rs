//! Versioned syntax nodes constructed directly by the generated parser.

use serde::{Deserialize, Serialize};

use super::span::Span;

/// Schema for standalone expression output owned by LALRPOP.
pub const LALRPOP_EXPRESSION_OUTPUT_SCHEMA: &str = "terlan.lalrpop-expression-output.v1";
/// Schema for standalone type output owned by LALRPOP.
pub const LALRPOP_TYPE_OUTPUT_SCHEMA: &str = "terlan.lalrpop-type-output.v1";
/// Schema for standalone pattern output owned by LALRPOP.
pub const LALRPOP_PATTERN_OUTPUT_SCHEMA: &str = "terlan.lalrpop-pattern-output.v1";
/// Schema for complete module syntax output owned by LALRPOP.
pub const LALRPOP_MODULE_OUTPUT_SCHEMA: &str = "terlan.lalrpop-module-output.v1";

/// O(1) conversion from canonical lexer scalar offsets to Rust byte ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LalrpopSourceIndex;

impl LalrpopSourceIndex {
    /// Indexes every Unicode scalar boundary, including the end of input.
    pub fn new(_source: &str) -> Self {
        Self
    }

    /// Returns the text covered by canonical scalar offsets.
    pub fn text<'source>(&self, source: &'source str, start: usize, end: usize) -> &'source str {
        &source[start..end]
    }
}

/// Stable generated syntax classifications used before validation and lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LalrpopSyntaxNodeKind {
    /// Integer literal.
    Int,
    /// Floating-point literal.
    Float,
    /// String literal.
    String,
    /// Canonical `Atom["..."]` literal.
    AtomLiteral,
    /// Binary literal.
    BinaryLiteral,
    /// Descriptor-backed `Binary[endian] { ... }` value or pattern.
    BinaryLayout,
    /// One descriptor-backed binary layout field.
    BinaryLayoutField,
    /// Raw HTML block retained for the HTML validation phase.
    RawMacro,
    /// Source binding or value reference.
    Binding,
    /// Parenthesized expression.
    Group,
    /// Tuple expression.
    Tuple,
    /// List expression.
    List,
    /// Fixed-array expression.
    FixedArray,
    /// Keyed-container expression.
    Map,
    /// One keyed-container field.
    MapField,
    /// Improper list head/tail pair.
    ListCons,
    /// List-comprehension expression and qualifiers.
    ListComprehension,
    /// One list-comprehension generator.
    Generator,
    /// Unary operation.
    Unary,
    /// Binary operation.
    Binary,
    /// Explicit type cast.
    Cast,
    /// Type syntax retained at the parser boundary.
    Type,
    /// Union type.
    TypeUnion,
    /// Function-arrow type.
    TypeArrow,
    /// Existential type.
    TypeExistential,
    /// Tuple type.
    TypeTuple,
    /// Keyed-container type.
    TypeMap,
    /// One keyed-container type field.
    TypeField,
    /// List type.
    TypeList,
    /// Source pattern.
    Pattern,
    /// Tuple pattern.
    PatternTuple,
    /// List pattern.
    PatternList,
    /// Improper-list pattern.
    PatternListCons,
    /// Keyed-container pattern.
    PatternMap,
    /// One keyed-container pattern field.
    PatternField,
    /// Constructor pattern.
    PatternConstructor,
    /// Function-value call.
    Call,
    /// Indexed value.
    Index,
    /// Indexed update.
    IndexAssign,
    /// Adjacent field access.
    FieldAccess,
    /// Named-record field access.
    RecordAccess,
    /// Named-record update.
    RecordUpdate,
    /// Semicolon-separated expression sequence.
    Sequence,
    /// Quoted source expression.
    Quote,
    /// Unquoted source expression.
    Unquote,
    /// Pattern-binding expression.
    Let,
    /// Case expression.
    Case,
    /// Try/catch/after expression.
    Try,
    /// If expression.
    If,
    /// Branch clause.
    Clause,
    /// Anonymous function expression.
    Lambda,
    /// Constructor value extended by one or more constructor layers.
    ConstructorChain,
    /// Compile-time macro call.
    MacroCall,
    /// Complete source module.
    Module,
    /// Module declaration.
    ModuleDeclaration,
    /// Import declaration.
    ImportDeclaration,
    /// Interface export declaration.
    ExportDeclaration,
    /// One exported callable name/arity pair.
    ExportItem,
    /// Constant declaration.
    ConstantDeclaration,
    /// Type declaration.
    TypeDeclaration,
    /// One name/value arm of a valued union declaration.
    ValuedUnionArm,
    /// Struct declaration.
    StructDeclaration,
    /// Constructor declaration.
    ConstructorDeclaration,
    /// One constructor clause.
    ConstructorClause,
    /// Trait declaration.
    TraitDeclaration,
    /// Trait implementation declaration.
    TraitImplementationDeclaration,
    /// Template declaration.
    TemplateDeclaration,
    /// Target or runtime configuration declaration.
    ConfigDeclaration,
    /// Binary shape declaration.
    ShapeDeclaration,
    /// Function declaration.
    FunctionDeclaration,
    /// Receiver method declaration.
    MethodDeclaration,
    /// Receiver binding on a method.
    Receiver,
    /// Typed callable parameter.
    Parameter,
    /// Struct field declaration.
    StructField,
    /// Source annotation attached to a declaration.
    Annotation,
    /// Structured annotation value or entry.
    AnnotationValue,
}

/// Span-preserving syntax node emitted by generated grammar productions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LalrpopSyntaxNode {
    /// Stable node classification.
    pub kind: LalrpopSyntaxNodeKind,
    /// Exact byte range owned by this node.
    pub span: Span,
    /// Literal, binding, field, or operator spelling when the kind has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Ordered child nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<LalrpopSyntaxNode>,
}

impl LalrpopSyntaxNode {
    /// Constructs a leaf node from an exact source range.
    pub fn leaf(
        kind: LalrpopSyntaxNodeKind,
        source: &str,
        source_index: &LalrpopSourceIndex,
        start: usize,
        end: usize,
    ) -> Self {
        Self {
            kind,
            span: Span::new(start, end),
            text: Some(source_index.text(source, start, end).to_string()),
            children: Vec::new(),
        }
    }

    /// Constructs a node whose spelling is structural rather than source text.
    pub fn branch(
        kind: LalrpopSyntaxNodeKind,
        start: usize,
        end: usize,
        text: Option<String>,
        children: Vec<Self>,
    ) -> Self {
        Self {
            kind,
            span: Span::new(start, end),
            text,
            children,
        }
    }

    /// Constructs a unary node while retaining the operator spelling.
    pub fn unary(
        source: &str,
        source_index: &LalrpopSourceIndex,
        start: usize,
        operator_end: usize,
        operand: Self,
    ) -> Self {
        let end = operand.span.end;
        Self::branch(
            LalrpopSyntaxNodeKind::Unary,
            start,
            end,
            Some(source_index.text(source, start, operator_end).to_string()),
            vec![operand],
        )
    }

    /// Left-folds one precedence level into explicit binary nodes.
    pub fn fold_binary(mut left: Self, tails: Vec<(String, Self)>) -> Self {
        for (operator, right) in tails {
            let start = left.span.start;
            let end = right.span.end;
            left = Self::branch(
                LalrpopSyntaxNodeKind::Binary,
                start,
                end,
                Some(operator),
                vec![left, right],
            );
        }
        left
    }

    /// Applies postfix syntax in source order.
    pub fn apply_postfix(mut value: Self, suffixes: Vec<LalrpopPostfixSyntax>) -> Self {
        for suffix in suffixes {
            value = match suffix {
                LalrpopPostfixSyntax::Call { args, end } => {
                    let start = value.span.start;
                    let mut children = Vec::with_capacity(args.len() + 1);
                    children.push(value);
                    children.extend(args);
                    Self::branch(LalrpopSyntaxNodeKind::Call, start, end, None, children)
                }
                LalrpopPostfixSyntax::GenericCall {
                    type_args,
                    args,
                    end,
                } => {
                    let start = value.span.start;
                    let type_arg_count = type_args.len();
                    let mut children = Vec::with_capacity(type_args.len() + args.len() + 1);
                    children.push(value);
                    children.extend(type_args);
                    children.extend(args);
                    Self::branch(
                        LalrpopSyntaxNodeKind::Call,
                        start,
                        end,
                        Some(format!("generic:{type_arg_count}")),
                        children,
                    )
                }
                LalrpopPostfixSyntax::Index { index, end } => {
                    let start = value.span.start;
                    Self::branch(
                        LalrpopSyntaxNodeKind::Index,
                        start,
                        end,
                        None,
                        vec![value, index],
                    )
                }
                LalrpopPostfixSyntax::Field { field, end } => {
                    let start = value.span.start;
                    Self::branch(
                        LalrpopSyntaxNodeKind::FieldAccess,
                        start,
                        end,
                        Some(field),
                        vec![value],
                    )
                }
                LalrpopPostfixSyntax::RecordAccess { record, field, end } => {
                    let start = value.span.start;
                    Self::branch(
                        LalrpopSyntaxNodeKind::RecordAccess,
                        start,
                        end,
                        Some(format!("{record}.{field}")),
                        vec![value],
                    )
                }
                LalrpopPostfixSyntax::RecordUpdate {
                    record,
                    fields,
                    end,
                } => {
                    let start = value.span.start;
                    let mut children = Vec::with_capacity(fields.len() + 1);
                    children.push(value);
                    children.extend(fields);
                    Self::branch(
                        LalrpopSyntaxNodeKind::RecordUpdate,
                        start,
                        end,
                        Some(record),
                        children,
                    )
                }
            };
        }
        value
    }
}

/// Deferred postfix payload used to keep grammar actions structural.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LalrpopPostfixSyntax {
    /// Call arguments and closing-delimiter location.
    Call {
        /// Positional arguments in source order.
        args: Vec<LalrpopSyntaxNode>,
        /// End of the closing parenthesis.
        end: usize,
    },
    /// Explicit type arguments followed by ordinary call arguments.
    GenericCall {
        /// Type arguments in source order.
        type_args: Vec<LalrpopSyntaxNode>,
        /// Positional call arguments in source order.
        args: Vec<LalrpopSyntaxNode>,
        /// End of the closing parenthesis.
        end: usize,
    },
    /// Index operand and closing-delimiter location.
    Index {
        /// Index expression.
        index: LalrpopSyntaxNode,
        /// End of the closing bracket.
        end: usize,
    },
    /// Adjacent field name and its end location.
    Field {
        /// Field spelling.
        field: String,
        /// End of the field.
        end: usize,
    },
    /// Named-record field access.
    RecordAccess {
        /// Record type spelling.
        record: String,
        /// Field spelling.
        field: String,
        /// End of the field.
        end: usize,
    },
    /// Named-record update fields.
    RecordUpdate {
        /// Record type spelling.
        record: String,
        /// Updated fields.
        fields: Vec<LalrpopSyntaxNode>,
        /// End of the closing brace.
        end: usize,
    },
}

/// Versioned standalone expression syntax output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LalrpopExpressionOutput {
    /// Stable output schema.
    pub schema: &'static str,
    /// Fingerprint of the canonical EBNF used for the parse.
    pub grammar_identity: String,
    /// Generated expression root.
    pub root: LalrpopSyntaxNode,
}

/// Versioned generated syntax output for a standalone type or pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LalrpopFragmentOutput {
    /// Stable output schema.
    pub schema: &'static str,
    /// Fingerprint of the canonical EBNF used for the parse.
    pub grammar_identity: String,
    /// Generated fragment root.
    pub root: LalrpopSyntaxNode,
}

impl LalrpopFragmentOutput {
    /// Wraps a generated fragment root in its syntax-output envelope.
    pub fn new(schema: &'static str, grammar_identity: &str, root: LalrpopSyntaxNode) -> Self {
        Self {
            schema,
            grammar_identity: grammar_identity.to_string(),
            root,
        }
    }
}

/// Versioned generated syntax output for a complete source module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LalrpopModuleSyntaxOutput {
    /// Stable output schema.
    pub schema: &'static str,
    /// Fingerprint of the canonical EBNF used for the parse.
    pub grammar_identity: String,
    /// Canonical dotted module identity.
    pub module_name: String,
    /// Module declaration and ordered declarations.
    pub root: LalrpopSyntaxNode,
}

impl LalrpopModuleSyntaxOutput {
    /// Constructs the stable module syntax-output envelope.
    pub fn new(
        grammar_identity: &str,
        module_name: String,
        module_declaration: LalrpopSyntaxNode,
        declarations: Vec<LalrpopSyntaxNode>,
    ) -> Self {
        let start = module_declaration.span.start;
        let end = declarations
            .last()
            .map_or(module_declaration.span.end, |node| node.span.end);
        let mut children = Vec::with_capacity(declarations.len() + 1);
        children.push(module_declaration);
        children.extend(declarations);
        Self {
            schema: LALRPOP_MODULE_OUTPUT_SCHEMA,
            grammar_identity: grammar_identity.to_string(),
            module_name,
            root: LalrpopSyntaxNode::branch(
                LalrpopSyntaxNodeKind::Module,
                start,
                end,
                None,
                children,
            ),
        }
    }
}

impl LalrpopExpressionOutput {
    /// Wraps a generated root in the stable syntax-output envelope.
    pub fn new(grammar_identity: &str, root: LalrpopSyntaxNode) -> Self {
        Self {
            schema: LALRPOP_EXPRESSION_OUTPUT_SCHEMA,
            grammar_identity: grammar_identity.to_string(),
            root,
        }
    }
}

/// Validates syntax restrictions intentionally kept out of grammar actions.
pub fn validate_lalrpop_expression(root: &LalrpopSyntaxNode) -> Result<(), (String, Span)> {
    validate_lalrpop_expression_with_parent(root, None)
}

fn validate_lalrpop_expression_with_parent(
    root: &LalrpopSyntaxNode,
    parent: Option<LalrpopSyntaxNodeKind>,
) -> Result<(), (String, Span)> {
    if root.kind == LalrpopSyntaxNodeKind::Index
        && root.children.len() == 2
        && root.children[0].kind == LalrpopSyntaxNodeKind::Binding
        && root.children[0].text.as_deref() == Some("Atom")
    {
        return Err((
            "expected String literal inside `Atom[...]`".to_string(),
            root.children[1].span,
        ));
    }
    if root.kind == LalrpopSyntaxNodeKind::AtomLiteral
        && root.text.as_deref().is_some_and(|text| text == "\"\"")
    {
        return Err((
            "expected non-empty atom string literal".to_string(),
            root.span,
        ));
    }
    if root.kind == LalrpopSyntaxNodeKind::IndexAssign
        && root
            .children
            .first()
            .is_none_or(|left| left.kind != LalrpopSyntaxNodeKind::Index)
        && parent != Some(LalrpopSyntaxNodeKind::Call)
    {
        return Err((
            "plain `=` is not assignment or pattern matching in Terlan; use `let name = value` to bind, `==` to compare, `case` to match shapes, or `collection[index] = value` for indexed collection updates".to_string(),
            root.span,
        ));
    }
    for child in &root.children {
        validate_lalrpop_expression_with_parent(child, Some(root.kind))?;
    }
    Ok(())
}

/// Classifies context-sensitive shapes after generated parsing.
///
/// This is deliberately separate from grammar actions. `Atom["..."]` shares
/// its token prefix with ordinary indexing, so the structural result is
/// normalized only after LALRPOP has produced an unambiguous tree.
pub fn normalize_lalrpop_expression(mut root: LalrpopSyntaxNode) -> LalrpopSyntaxNode {
    root.children = root
        .children
        .into_iter()
        .map(normalize_lalrpop_expression)
        .collect();
    if root.kind == LalrpopSyntaxNodeKind::Index
        && root.children.len() == 2
        && root.children[0].kind == LalrpopSyntaxNodeKind::Binding
        && root.children[0].text.as_deref() == Some("Atom")
        && root.children[1].kind == LalrpopSyntaxNodeKind::String
    {
        root.kind = LalrpopSyntaxNodeKind::AtomLiteral;
        root.text = root.children[1].text.clone();
        root.children.clear();
    }
    if root.kind == LalrpopSyntaxNodeKind::Call
        && root
            .children
            .first()
            .is_some_and(|callee| callee.kind == LalrpopSyntaxNodeKind::MacroCall)
    {
        let mut children = root.children;
        let callee = children.remove(0);
        root.kind = LalrpopSyntaxNodeKind::MacroCall;
        root.text = callee.text;
        root.children = children;
    }
    if root.kind == LalrpopSyntaxNodeKind::Sequence
        && root
            .children
            .first()
            .is_some_and(|child| child.kind == LalrpopSyntaxNodeKind::Let)
    {
        let mut sequence_children = root.children;
        let mut leading_let = sequence_children.remove(0);
        if let Some(body) = leading_let.children.pop() {
            let mut body_children = Vec::with_capacity(sequence_children.len() + 1);
            body_children.push(body);
            body_children.extend(sequence_children);
            let body_start = body_children
                .first()
                .map_or(leading_let.span.start, |child| child.span.start);
            let body_end = body_children
                .last()
                .map_or(leading_let.span.end, |child| child.span.end);
            leading_let.children.push(LalrpopSyntaxNode::branch(
                LalrpopSyntaxNodeKind::Sequence,
                body_start,
                body_end,
                None,
                body_children,
            ));
        }
        leading_let.span.end = root.span.end;
        root = leading_let;
    }
    root
}
