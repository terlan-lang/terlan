use std::collections::HashSet;

use crate::{
    ast::{Decl, Module},
    ebnf::{
        EbnfCompileError, EbnfCompileResult, EbnfGrammarContract, EbnfGrammarExpr,
        EbnfGrammarExprKind, EbnfGrammarRule,
    },
    parser::{parse_interface_module, parse_module},
    span::Span,
};

#[derive(Debug, Clone, Copy)]
pub enum ContractMode {
    /// Parse and contract-check canonical `.tl` source modules.
    Module,
    /// Parse and contract-check `.tli` interface summaries.
    Interface,
}

/// Convert parser output into a first-pass compiler contract tree.
///
/// Inputs:
/// - `input`: canonical `.tl` source text.
///
/// Output:
/// - `EbnfGrammarContract` when the source parses as a normal module.
/// - `EbnfCompileError::Parse` when source-only grammar is malformed or removed.
///
/// Transformation:
/// - Parses through the normal source parser, then projects declaration classes
///   into the lossy EBNF grammar contract shape for parser migration workstreams.
pub fn parse_module_as_contract(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_module_as_contract_mode(input, ContractMode::Module)
}

/// Parse interface modules into the shared contract tree.
///
/// Inputs:
/// - `input`: `.tli` interface text.
///
/// Output:
/// - `EbnfGrammarContract` when the interface parses.
/// - `EbnfCompileError::Parse` when interface syntax is malformed.
///
/// Transformation:
/// - Parses through the interface parser, preserving interface-only summaries
///   such as `ExportDecl`, then projects declarations into the shared contract
///   shape used by formal parser migration checks.
pub fn parse_interface_module_as_contract(input: &str) -> EbnfCompileResult<EbnfGrammarContract> {
    parse_module_as_contract_mode(input, ContractMode::Interface)
}

/// Parses a module-like source according to the selected contract mode.
///
/// Inputs:
/// - `input`: raw source text.
/// - `mode`: whether to parse as canonical `.tl` source or `.tli` interface text.
///
/// Output:
/// - Contract tree on success, or parse/serialization error on failure.
///
/// Transformation:
/// - Selects the appropriate parser entrypoint, maps parser diagnostics into
///   EBNF compile diagnostics, and delegates AST-to-contract projection.
fn parse_module_as_contract_mode(
    input: &str,
    mode: ContractMode,
) -> EbnfCompileResult<EbnfGrammarContract> {
    let module = match mode {
        ContractMode::Module => {
            parse_module(input).map_err(|err| EbnfCompileError::Parse(err.message, err.span))?
        }
        ContractMode::Interface => parse_interface_module(input)
            .map_err(|err| EbnfCompileError::Parse(err.message, err.span))?,
    };

    module_as_contract(&module)
}

/// Converts a parsed module AST into a lossy EBNF grammar contract.
///
/// Inputs:
/// - `module`: parsed module or interface AST.
///
/// Output:
/// - `EbnfGrammarContract` containing module metadata and declaration classes.
///
/// Transformation:
/// - Emits one terminal rule per observed declaration class and a `Program`
///   sequence that references the module header, module name, and declarations.
pub(crate) fn module_as_contract(module: &Module) -> EbnfCompileResult<EbnfGrammarContract> {
    let mut rules = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut program_items = Vec::new();

    push_terminal_rule(
        "ModuleDecl",
        "ModuleDecl",
        module.span,
        &mut rules,
        &mut seen,
    );
    program_items.push(nonterminal_expr("Program", 0, "ModuleDecl", module.span));
    program_items.push(nonterminal_expr("Program", 1, "ModuleName", module.span));
    push_terminal_rule(
        "ModuleName",
        module.name.clone(),
        module.span,
        &mut rules,
        &mut seen,
    );

    for (index, declaration) in module.declarations.iter().enumerate() {
        let class = contract_decl_class(declaration);
        let span = decl_span(declaration);
        program_items.push(nonterminal_expr("Program", index + 2, class, span));
        push_terminal_rule(class, class, span, &mut rules, &mut seen);
    }

    rules.push(EbnfGrammarRule {
        id: "rule:Program".to_string(),
        name: "Program".to_string(),
        span: module.span.into(),
        name_span: module.span.into(),
        expr: EbnfGrammarExpr {
            id: "rule:Program/expr".to_string(),
            span: module.span.into(),
            kind: EbnfGrammarExprKind::Sequence {
                items: program_items,
            },
        },
    });

    Ok(EbnfGrammarContract {
        format_version: 1,
        entry_rule: Some("Program".to_string()),
        rules,
    })
}

/// Builds a nonterminal expression for one generated contract rule reference.
///
/// Inputs:
/// - `rule_name`: owner rule name used in generated expression identifiers.
/// - `index`: sequence index for stable generated IDs.
/// - `name`: referenced nonterminal name.
/// - `span`: source span attached to the generated expression.
///
/// Output:
/// - `EbnfGrammarExpr` referencing `name`.
///
/// Transformation:
/// - Encodes contract structure metadata without inspecting source text.
fn nonterminal_expr(rule_name: &str, index: usize, name: &str, span: Span) -> EbnfGrammarExpr {
    EbnfGrammarExpr {
        id: format!("rule:{rule_name}/expr/seq:{index}"),
        span: span.into(),
        kind: EbnfGrammarExprKind::Nonterminal {
            name: name.to_string(),
        },
    }
}

/// Adds a terminal rule if that declaration class has not already been emitted.
///
/// Inputs:
/// - `name`: generated rule name.
/// - `value`: terminal text to store in the rule expression.
/// - `span`: source span attached to the rule.
/// - `rules`: output rule collection.
/// - `seen`: set of rule names already emitted.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - De-duplicates declaration-class rules while preserving first-observed span
///   information for the contract output.
fn push_terminal_rule(
    name: &str,
    value: impl Into<String>,
    span: Span,
    rules: &mut Vec<EbnfGrammarRule>,
    seen: &mut HashSet<String>,
) {
    if seen.insert(name.to_string()) {
        rules.push(EbnfGrammarRule {
            id: format!("rule:{name}"),
            name: name.to_string(),
            span: span.into(),
            name_span: span.into(),
            expr: EbnfGrammarExpr {
                id: format!("rule:{name}/expr"),
                span: span.into(),
                kind: EbnfGrammarExprKind::Terminal {
                    value: value.into(),
                },
            },
        });
    }
}

/// Returns the source span for a parsed declaration.
///
/// Inputs:
/// - `declaration`: AST declaration from a parsed module or interface.
///
/// Output:
/// - The declaration's source span.
///
/// Transformation:
/// - Dispatches across declaration variants without changing the AST.
pub(crate) fn decl_span(declaration: &Decl) -> Span {
    match declaration {
        Decl::Import(decl) => decl.span,
        Decl::Export(decl) => decl.span,
        Decl::Type(decl) => decl.span,
        Decl::Struct(decl) => decl.span,
        Decl::Constructor(decl) => decl.span,
        Decl::Function(decl) => decl.span,
        Decl::Method(decl) => decl.span,
        Decl::Trait(decl) => decl.span,
        Decl::TraitImpl(decl) => decl.span,
        Decl::Template(decl) => decl.span,
        Decl::Raw(decl) => decl.span,
    }
}

/// Maps a parsed declaration into the formal contract declaration class name.
///
/// Inputs:
/// - `declaration`: AST declaration from a parsed module or interface.
///
/// Output:
/// - Stable EBNF declaration class name.
///
/// Transformation:
/// - Collapses parser-specific variants into contract-facing rule names. Export
///   declarations are interface summaries only in current canonical Terlan; the
///   normal source parser rejects them before this mapping is reached.
pub(crate) fn contract_decl_class(declaration: &Decl) -> &'static str {
    match declaration {
        Decl::Import(_) => "ImportDecl",
        Decl::Export(_) => "ExportDecl",
        Decl::Type(ty) if ty.is_opaque => "OpaqueTypeDecl",
        Decl::Type(_) => "TypeDecl",
        Decl::Struct(_) => "StructDecl",
        Decl::Constructor(_) => "ConstructorDecl",
        Decl::Function(_) => "FunctionDecl",
        Decl::Method(_) => "MethodDecl",
        Decl::Trait(_) => "TraitDecl",
        Decl::TraitImpl(_) => "TraitImplDecl",
        Decl::Template(_) => "TemplateDecl",
        Decl::Raw(raw) if is_config_decl_kind(&raw.kind) => "ConfigDecl",
        Decl::Raw(_) => "RawDecl",
    }
}

/// Returns whether a raw placeholder kind is a formal config declaration.
///
/// Inputs:
/// - `kind`: raw declaration kind preserved by the hand-written parser.
///
/// Output:
/// - `true` for the source-level config declaration heads.
///
/// Transformation:
/// - Classifies transitional raw placeholders without changing the main AST,
///   allowing parser-contract output to follow the canonical `ConfigDecl` rule.
fn is_config_decl_kind(kind: &str) -> bool {
    matches!(kind, "target" | "native" | "machine" | "static")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_contract_includes_program_entry_and_declarations() {
        let output = parse_module_as_contract(
            r#"
            module demo.

            import lib.Mod.
            type Item = Int.
            pub add(X: Int): Int -> X + 1.
            "#,
        )
        .expect("parse to contract");

        assert_eq!(output.entry_rule.as_deref(), Some("Program"));
        assert!(output
            .rules
            .iter()
            .any(|rule| rule.name == "Program" || rule.name == "ModuleDecl"));
        let module_name_rule = output
            .rules
            .iter()
            .find(|rule| rule.name == "ModuleName")
            .expect("module name rule");
        assert!(matches!(
            module_name_rule.expr.kind,
            EbnfGrammarExprKind::Terminal { .. }
        ));
        let EbnfGrammarExprKind::Terminal { value } = &module_name_rule.expr.kind else {
            panic!("expected terminal module name")
        };
        assert_eq!(value, "demo");
        assert_eq!(module_name_rule.id, "rule:ModuleName");
        assert_eq!(module_name_rule.expr.id, "rule:ModuleName/expr");
    }

    #[test]
    fn interface_contract_follows_same_rules() {
        let output = parse_interface_module_as_contract(
            r#"
            module demo.

            export demo/1.
            "#,
        )
        .expect("parse interface contract");

        assert_eq!(output.entry_rule.as_deref(), Some("Program"));
        assert!(output.rules.iter().any(|rule| rule.name == "ExportDecl"));
    }

    /// Verifies the normal source contract path cannot reintroduce export-list
    /// declarations.
    ///
    /// Inputs:
    /// - `.tl` module source containing removed source-mode `export` syntax.
    ///
    /// Output:
    /// - Parse diagnostic from the normal source parser.
    ///
    /// Transformation:
    /// - Routes the source through `parse_module_as_contract`, proving contract
    ///   projection starts after canonical source validation.
    #[test]
    fn module_contract_rejects_source_export_declarations() {
        let error = parse_module_as_contract(
            r#"
            module demo.

            export demo/1.
            "#,
        )
        .expect_err("normal source contract must reject export lists");

        match error {
            EbnfCompileError::Parse(message, _) => {
                assert!(
                    message.contains("source export declarations are not part of canonical Terlan")
                );
            }
            other => panic!("unexpected contract error: {other:?}"),
        }
    }

    #[test]
    fn contract_output_is_serializable_via_grammar_contract_path() {
        let output = parse_module_as_contract(
            r#"
            module demo.

            type X = Int.
            "#,
        )
        .expect("parse contract");

        let raw = serde_json::to_string(&output).expect("to json");
        let decoded = serde_json::from_str::<EbnfGrammarContract>(&raw).expect("from json");
        assert_eq!(decoded.entry_rule, Some("Program".to_string()));
        assert_eq!(decoded.rules.len(), output.rules.len());
    }

    #[test]
    fn module_decl_class_mapping_is_stable() {
        use crate::ast::Decl;
        let class = contract_decl_class(&Decl::Raw(crate::ast::UnsupportedDecl {
            kind: "target".into(),
            text: "{}".into(),
            docs: vec![],
            span: crate::span::Span::new(0, 0),
        }));
        assert_eq!(class, "ConfigDecl");
    }
}
