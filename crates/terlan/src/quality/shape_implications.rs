use std::fs;
use std::path::Path;

use super::support::find_active_roadmap;
use crate::terlan_quality::{render_failure, QualityResult};

const SECTION_HEADER: &str = "### Shape Implications";
const COMPACTED_CONTRACT_ARCHIVE: &str =
    "archive/ROADMAP_0_0_7_ACTIVE_PRE_COMPACTION_2026_07_31.md";
const PLACEHOLDER_CONTRACT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

/// Summary produced by the shape implications roadmap contract gate.
///
/// Inputs:
/// - Required terminology and implementation constraints from the active
///   0.0.7 roadmap.
/// - Acceptance and adversarial coverage requirements for the future parser
///   and typechecker implementation.
///
/// Output:
/// - Stable counts for the quality CLI.
///
/// Transformation:
/// - Keeps the `=>` implication design executable before syntax support lands,
///   so the implementation cannot quietly skip EBNF, Lean, or std adoption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeImplicationsSummary {
    pub required_term_count: usize,
    pub acceptance_term_count: usize,
}

/// Runs the shape implications roadmap contract gate.
///
/// Inputs:
/// - `root`: compiler repository root.
///
/// Output:
/// - Success when the active 0.0.7 roadmap fully specifies the implication
///   arrow contract.
/// - Stable diagnostics when required language, proof, tooling, or acceptance
///   clauses are missing.
///
/// Transformation:
/// - Treats shape implications as a formal language feature, not an ad hoc
///   parser shortcut.
pub fn run_shape_implications(root: &Path) -> QualityResult<ShapeImplicationsSummary> {
    let roadmap_path = find_active_roadmap(root)?;
    let roadmap = fs::read_to_string(&roadmap_path)
        .map_err(|err| format!("{}: failed to read roadmap: {err}", roadmap_path.display()))?;
    let archived;
    let (contract_path, section) = if let Some(section) = extract_section(&roadmap, SECTION_HEADER)
    {
        (roadmap_path.clone(), section)
    } else {
        let archive_path = roadmap_path
            .parent()
            .expect("roadmap path has a parent")
            .join(COMPACTED_CONTRACT_ARCHIVE);
        archived = fs::read_to_string(&archive_path).map_err(|error| {
            format!(
                "{}: active roadmap is compacted and the implication contract archive could not be read: {error}",
                archive_path.display()
            )
        })?;
        let section = extract_section(&archived, SECTION_HEADER).ok_or_else(|| {
            format!(
                "{}: missing `{SECTION_HEADER}` section",
                archive_path.display()
            )
        })?;
        (archive_path, section)
    };
    let diagnostics = validate_shape_implications_section(section);
    if !diagnostics.is_empty() {
        return Err(format!(
            "{}\ncontract: {}",
            render_failure("shape-implications", &diagnostics),
            contract_path.display()
        ));
    }
    Ok(ShapeImplicationsSummary {
        required_term_count: REQUIRED_TERMS.len(),
        acceptance_term_count: ACCEPTANCE_TERMS.len(),
    })
}

/// Finds the active roadmap from the compiler root.
/// Extracts one Markdown subsection.
fn extract_section<'a>(document: &'a str, header: &str) -> Option<&'a str> {
    let start = document.find(header)?;
    let after_header = start + header.len();
    let rest = &document[after_header..];
    let mut end = rest.len();
    for marker in ["\n### ", "\n## "] {
        if let Some(position) = rest.find(marker) {
            end = end.min(position);
        }
    }
    Some(rest[..end].trim())
}

/// Validates the shape implications section content.
fn validate_shape_implications_section(section: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_no_placeholder_contract_terms());
    validate_required_terms(section, REQUIRED_TERMS, "requirement", &mut diagnostics);
    validate_required_terms(section, ACCEPTANCE_TERMS, "acceptance", &mut diagnostics);
    diagnostics
}

/// Checks a term matrix against the section.
fn validate_required_terms(
    section: &str,
    terms: &[RequiredTerm],
    category: &str,
    diagnostics: &mut Vec<String>,
) {
    for term in terms {
        if !term
            .fragments
            .iter()
            .all(|fragment| section.contains(fragment))
        {
            diagnostics.push(format!(
                "`{SECTION_HEADER}` is missing {category} `{}`",
                term.label
            ));
        }
    }
}

/// One required roadmap term represented by stable fragments.
struct RequiredTerm {
    label: &'static str,
    fragments: &'static [&'static str],
}

fn validate_no_placeholder_contract_terms() -> Vec<String> {
    REQUIRED_TERMS
        .iter()
        .chain(ACCEPTANCE_TERMS)
        .flat_map(validate_required_term_has_no_placeholder_fragments)
        .collect()
}

fn validate_required_term_has_no_placeholder_fragments(term: &RequiredTerm) -> Vec<String> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_text_has_no_placeholder_term(
        "shape implication term label",
        term.label,
    ));
    for fragment in term.fragments {
        diagnostics.extend(validate_text_has_no_placeholder_term(
            "shape implication term fragment",
            fragment,
        ));
    }
    diagnostics
}

fn validate_text_has_no_placeholder_term(label: &str, text: &str) -> Vec<String> {
    let normalized = text.to_ascii_lowercase();
    PLACEHOLDER_CONTRACT_TERMS
        .iter()
        .filter(|term| normalized.contains(**term))
        .map(|term| {
            format!("`{SECTION_HEADER}` {label} `{text}` contains placeholder term `{term}`")
        })
        .collect()
}

const REQUIRED_TERMS: &[RequiredTerm] = &[
    RequiredTerm {
        label: "implication arrow terminology",
        fragments: &["`=>` is called the implication arrow"],
    },
    RequiredTerm {
        label: "compile-time structural evidence",
        fragments: &["compile-time structural evidence"],
    },
    RequiredTerm {
        label: "proof-only non-conversion semantics",
        fragments: &[
            "does not allocate",
            "construct a wrapper",
            "call user code",
            "convert the value",
        ],
    },
    RequiredTerm {
        label: "not runtime syntax",
        fragments: &[
            "not a runtime",
            "not a conversion operator",
            "not a generator hook",
            "not a\n    macro system",
        ],
    },
    RequiredTerm {
        label: "where reserved for guards",
        fragments: &[
            "`where` is reserved",
            "runtime/value guards",
            "must not use declaration `where` clauses",
        ],
    },
    RequiredTerm {
        label: "generic parameter implication surface",
        fragments: &[
            "positive structural implication",
            "generic parameter constraints only",
            "generic parameter lists use implication shorthand",
            "pub display_name[T => {name: String}](value: T): String",
            "pub struct Page[T => {title: String}]",
            "pub impl Render[T => {title: String}] for T",
            "pub type Named[T => {name: String}]",
            "This shorthand is the canonical implication syntax",
            "must not desugar to",
            "any declaration `where` implication form",
        ],
    },
    RequiredTerm {
        label: "field access evidence",
        fragments: &[
            "field access",
            "`value.name` is legal and typed as\n    `String`",
        ],
    },
    RequiredTerm {
        label: "fail-closed implication checking",
        fragments: &[
            "implication checking is fail-closed",
            "program is rejected",
            "no\n    runtime fallback",
        ],
    },
    RequiredTerm {
        label: "typed evidence provenance",
        fragments: &[
            "typed compiler\n    evidence with provenance",
            "built-in core\n    rules",
            "explicit user declarations",
            "generated binding manifests",
            "shape definitions with guards",
            "trait/type facts",
            "Ad hoc name matching is not evidence",
        ],
    },
    RequiredTerm {
        label: "scoped implication evidence",
        fragments: &[
            "implication evidence is scoped",
            "local constraint environment",
            "owning generic parameter list",
            "must not leak it outside",
            "lexical/typechecking scope",
        ],
    },
    RequiredTerm {
        label: "stable implication diagnostics",
        fragments: &[
            "unproven_implication",
            "ambiguous_implication",
            "implication_violation",
            "implication_scope_error",
        ],
    },
    RequiredTerm {
        label: "closed-shape target restriction",
        fragments: &[
            "closed structural field shapes",
            "reject open/dynamic maps",
            "`Dynamic`",
        ],
    },
    RequiredTerm {
        label: "field decorators rejected",
        fragments: &[
            "Field-level implication decorators",
            "declaration `where` implication",
            "clauses are not supported",
        ],
    },
    RequiredTerm {
        label: "negative implication deferred",
        fragments: &[
            "negative capability implications",
            "compiler-known contract",
            "negative structural implication is future work only",
        ],
    },
    RequiredTerm {
        label: "canonical EBNF update",
        fragments: &[
            "update the canonical EBNF before implementation",
            "generic-parameter shorthand",
            "type/evidence positions",
            "not as an expression-level binary operator",
            "not as a declaration `where` clause",
            "declaration `where` clauses",
            "parameter",
            "type annotations",
            "`value: T => {title: String}`",
        ],
    },
    RequiredTerm {
        label: "single grammar source",
        fragments: &[
            "parser fixtures",
            "tree-sitter grammar",
            "no duplicate implication grammar",
        ],
    },
    RequiredTerm {
        label: "Lean proof track",
        fragments: &[
            "formal type specification and Lean proof track",
            "implication well-formedness",
            "evidence\n    soundness for field access",
            "fail-closed unproven\n    implication rejection",
            "evidence provenance preservation",
        ],
    },
    RequiredTerm {
        label: "compiler phase agreement",
        fragments: &[
            "parser, formatter, typechecker, CoreIR, VM diagnostics",
            "LSP hover/completion",
            "coverage inventories",
        ],
    },
    RequiredTerm {
        label: "std adoption requirement",
        fragments: &[
            "seek std-library adoption",
            "at least one std-library API uses shape implication",
        ],
    },
    RequiredTerm {
        label: "gate wiring",
        fragments: &[
            "make shape-implications-check",
            "run `shape-implications-check` from `make check`",
        ],
    },
];

const ACCEPTANCE_TERMS: &[RequiredTerm] = &[
    RequiredTerm {
        label: "positive executable coverage",
        fragments: &[
            "executable `.terl` tests prove implication-constrained",
            "functions, receiver methods, structs, impls",
        ],
    },
    RequiredTerm {
        label: "adversarial diagnostics",
        fragments: &[
            "adversarial tests prove missing fields",
            "implication outside generic parameter",
            "constraints",
            "unproven implication",
            "scoped-evidence leakage",
            "attempted negative structural implication",
        ],
    },
    RequiredTerm {
        label: "real std usage before completion",
        fragments: &[
            "at least one std-library API uses shape implication evidence",
            "not only synthetic fixtures",
        ],
    },
];

#[cfg(test)]
#[path = "shape_implications_test.rs"]
#[cfg(test)]
mod shape_implications_test;
