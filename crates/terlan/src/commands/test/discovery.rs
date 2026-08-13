use crate::terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxModuleOutput,
    SyntaxTypeOutput,
};

/// Validated test function metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredTest {
    pub(super) name: String,
    pub(super) kind: TestKind,
    pub(super) span_start: usize,
    pub(super) span_end: usize,
    pub(super) literal_bool_result: Option<bool>,
}

/// Source-level executable case category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestKind {
    Test,
    Benchmark,
}

impl TestKind {
    pub(super) fn annotation(self) -> &'static str {
        match self {
            Self::Test => "@test",
            Self::Benchmark => "@benchmark",
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Benchmark => "benchmark",
        }
    }
}

/// Discovers valid `@test` function declarations.
///
/// Inputs:
/// - `module`: syntax output produced by formal parsing.
///
/// Output:
/// - `Ok(Vec<DiscoveredTest>)` when all annotated declarations are valid.
/// - `Err(Vec<String>)` when any annotated declaration violates the test
///   contract.
///
/// Transformation:
/// - Filters declarations with `@test` annotations and validates that they are
///   zero-argument functions returning `Bool` or assertion-compatible types.
pub(super) fn discover_tests(
    module: &SyntaxModuleOutput,
) -> Result<Vec<DiscoveredTest>, Vec<String>> {
    let mut tests = Vec::new();
    let mut errors = Vec::new();

    for declaration in &module.declarations {
        let has_test = has_annotation(declaration, "test");
        let has_benchmark = has_annotation(declaration, "benchmark");
        if !has_test && !has_benchmark {
            continue;
        }
        if has_test && has_benchmark {
            errors.push("a function cannot be both @test and @benchmark".to_string());
            continue;
        }
        let kind = if has_benchmark {
            TestKind::Benchmark
        } else {
            TestKind::Test
        };
        let annotation = kind.annotation();
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                clauses,
                ..
            } => {
                if !params.is_empty() {
                    errors.push(format!(
                        "{annotation} function {name} must have zero parameters"
                    ));
                }
                if !is_supported_test_return_type(return_type) {
                    errors.push(format!(
                        "{annotation} function {name} must return Bool or std.test.Test.Assertion, got {}",
                        return_type.text
                    ));
                }
                if params.is_empty() && is_supported_test_return_type(return_type) {
                    tests.push(DiscoveredTest {
                        name: name.clone(),
                        kind,
                        span_start: declaration.span.start,
                        span_end: declaration.span.end,
                        literal_bool_result: literal_bool_test_result(clauses),
                    });
                }
            }
            _ => errors.push(format!(
                "{annotation} can only annotate function declarations"
            )),
        }
    }

    if errors.is_empty() {
        Ok(tests)
    } else {
        Err(errors)
    }
}

/// Selects all discovered tests or one named test.
///
/// Inputs:
/// - `tests`: discovered and validated source-level test functions.
/// - `test_name`: optional exact function-name selector from `terlc test
///   --name`.
/// - `path`: source path used only for diagnostics.
///
/// Output:
/// - `Ok(Vec<DiscoveredTest>)` containing all tests or the exact selected test.
/// - `Err(message)` when a selector is present but no matching test exists.
///
/// Transformation:
/// - Applies exact function-name filtering after test discovery so compiler
///   diagnostics still validate every `@test` declaration in the file.
pub(super) fn select_tests(
    tests: Vec<DiscoveredTest>,
    test_name: Option<&str>,
    path: &str,
    kind: TestKind,
) -> Result<Vec<DiscoveredTest>, String> {
    let selected = tests
        .into_iter()
        .filter(|test| test.kind == kind)
        .filter(|test| test_name.is_none_or(|name| test.name == name))
        .collect::<Vec<_>>();
    if let (true, Some(test_name)) = (selected.is_empty(), test_name) {
        Err(format!(
            "no {} declaration named `{test_name}` found in {path}",
            kind.annotation()
        ))
    } else {
        Ok(selected)
    }
}

/// Extracts a literal boolean test result when the function body is trivial.
///
/// Inputs:
/// - `clauses`: syntax-output clauses for one zero-argument `@test` function.
///
/// Output:
/// - `Some(true)` or `Some(false)` for a single unguarded literal boolean body.
/// - `None` for any non-trivial test body.
///
/// Transformation:
/// - Recognizes the syntax-output atom form used for source booleans without
///   inspecting source text.
fn literal_bool_test_result(
    clauses: &[crate::terlan_syntax::SyntaxFunctionClauseOutput],
) -> Option<bool> {
    let [clause] = clauses else {
        return None;
    };
    if clause.has_guard || clause.guard.is_some() || !clause.patterns.is_empty() {
        return None;
    }
    if !matches!(clause.body.kind, SyntaxExprKind::Atom | SyntaxExprKind::Var) {
        return None;
    }
    match clause.body.text.as_deref() {
        Some("true") | Some("True") => Some(true),
        Some("false") | Some("False") => Some(false),
        _ => None,
    }
}

/// Returns whether a declaration carries the source-level `@test` annotation.
///
/// Inputs:
/// - `declaration`: syntax declaration to inspect.
///
/// Output:
/// - `true` when any annotation path is exactly `test`.
///
/// Transformation:
/// - Compares serialized annotation path segments without reading source text.
fn has_annotation(declaration: &SyntaxDeclarationOutput, name: &str) -> bool {
    declaration
        .annotations
        .iter()
        .any(|annotation| annotation.path.len() == 1 && annotation.path[0] == name)
}

/// Returns whether a test return type is supported by the first runner.
///
/// Inputs:
/// - `return_type`: syntax-level return type text and span.
///
/// Output:
/// - `true` for `Bool`, imported `Assertion`, and canonical
///   `std.test.Test.Assertion`.
///
/// Transformation:
/// - Trims syntax-output type text and checks the stable 0.0.1 assertion
///   spellings accepted by test discovery, without accepting backend-shaped or
///   AST module spellings.
pub(super) fn is_supported_test_return_type(return_type: &SyntaxTypeOutput) -> bool {
    matches!(
        return_type.text.trim(),
        "Bool" | "Assertion" | "std.test.Test.Assertion"
    )
}
