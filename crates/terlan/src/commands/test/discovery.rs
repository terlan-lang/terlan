use crate::terlan_syntax::{
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxModuleOutput,
    SyntaxTypeOutput,
};

/// Validated test function metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredTest {
    pub(super) name: String,
    pub(super) span_start: usize,
    pub(super) span_end: usize,
    pub(super) literal_bool_result: Option<bool>,
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
        if !has_test_annotation(declaration) {
            continue;
        }
        match &declaration.payload {
            SyntaxDeclarationPayload::Function {
                name,
                params,
                return_type,
                clauses,
                ..
            } => {
                if !params.is_empty() {
                    errors.push(format!("@test function {name} must have zero parameters"));
                }
                if !is_supported_test_return_type(return_type) {
                    errors.push(format!(
                        "@test function {name} must return Bool or std.test.Test.Assertion, got {}",
                        return_type.text
                    ));
                }
                if params.is_empty() && is_supported_test_return_type(return_type) {
                    tests.push(DiscoveredTest {
                        name: name.clone(),
                        span_start: declaration.span.start,
                        span_end: declaration.span.end,
                        literal_bool_result: literal_bool_test_result(clauses),
                    });
                }
            }
            _ => errors.push("@test can only annotate function declarations".to_string()),
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
) -> Result<Vec<DiscoveredTest>, String> {
    let Some(test_name) = test_name else {
        return Ok(tests);
    };
    let selected = tests
        .into_iter()
        .filter(|test| test.name == test_name)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        Err(format!(
            "no @test declaration named `{test_name}` found in {path}"
        ))
    } else {
        Ok(selected)
    }
}

/// Returns whether tests need compiled std release support modules.
///
/// Inputs:
/// - `tests`: discovered test metadata after syntax validation.
///
/// Output:
/// - `true` when at least one test body is not a literal boolean.
///
/// Transformation:
/// - Treats literal-bool surface tests as self-contained and all other tests as
///   runtime tests that may call support modules.
pub(super) fn tests_require_release_support(tests: &[DiscoveredTest]) -> bool {
    tests.iter().any(|test| test.literal_bool_result.is_none())
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
fn has_test_annotation(declaration: &SyntaxDeclarationOutput) -> bool {
    declaration
        .annotations
        .iter()
        .any(|annotation| annotation.path.as_slice() == ["test"])
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
