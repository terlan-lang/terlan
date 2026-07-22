use super::*;

/// Verifies the active section satisfies the shape implication contract.
///
/// Inputs:
/// - The active roadmap section copied from the 0.0.7 roadmap.
///
/// Output: no diagnostics.
///
/// Transformation: proves the gate accepts the current design contract.
#[test]
fn shape_implications_accepts_complete_contract_section() {
    let section = r#"
- [ ] Add the `=>` implication arrow for compile-time structural evidence.
  - Requirement: `=>` is called the implication arrow. It is not a runtime
    execution arrow, not a conversion operator, not a generator hook, and not a
    macro system.
  - Requirement: `where` is reserved for runtime/value guards only. Implication
    evidence must not use declaration `where` clauses.
  - Requirement: the initial supported form is positive structural implication
    in generic parameter constraints only. `where` is reserved for runtime
    guards and is not an implication surface.
  - Requirement: generic parameter lists use implication shorthand when the
    evidence is local to that generic parameter:
    ```terl
    pub display_name[T => {name: String}](value: T): String ->
        value.name.

    pub struct Page[T => {title: String}] {
        model: T
    }.

    pub impl Render[T => {title: String}] for T {
        render(value: T): Html ->
            h1(value.title).
    }

    pub type Named[T => {name: String}] =
        T.
    ```
    This shorthand is the canonical implication syntax and must not desugar to
    any declaration `where` implication form.
  - Requirement: `T => Shape` means the compiler can prove that values of `T`
    expose at least the required structural shape. It does not allocate,
    construct a wrapper, call user code, or convert the value.
  - Requirement: implication checking is fail-closed. If the compiler cannot
    prove the left side entails the right side from explicit evidence, the
    program is rejected. There is no best-effort implication inference and no
    runtime fallback.
  - Requirement: every accepted implication must produce typed compiler
    evidence with provenance. Accepted evidence sources are built-in core
    rules, explicit user declarations, generated binding manifests,
    shape definitions with guards, concrete closed types, and already-proven
    trait/type facts. Ad hoc name matching is not evidence.
  - Requirement: implication evidence is scoped. The typechecker must attach
    implication evidence to the local constraint environment introduced by the
    owning generic parameter list and must not leak it outside that
    lexical/typechecking scope.
  - Requirement: implication diagnostics must be stable and specific:
    `unproven_implication` when no evidence exists, `ambiguous_implication`
    when more than one incompatible evidence source matches,
    `implication_violation` when a later operation contradicts proven negative
    evidence, and `implication_scope_error` when evidence is used outside its
    valid scope.
  - Requirement: implications are allowed only in generic parameter lists on
    functions, receiver methods, structs, type aliases where legal, and impls.
    Field-level implication decorators and declaration `where` implication
    clauses are not supported.
  - Requirement: implication targets start with closed structural field shapes,
    including field names, field types, optional visibility rules, and nested
    shapes. The compiler must reject open/dynamic maps, `Dynamic`, unknown
    fields, private fields from outside the defining module, and ambiguous
    generated fields unless the source type is explicitly known closed.
  - Requirement: implication evidence must compose with field access. Inside a
    scope where `T => {name: String}` holds, `value.name` is legal and typed as
    `String`; outside that scope, generic field access remains rejected unless
    another rule proves it.
  - Requirement: negative capability implications such as
    `SecretKey => not Log` are allowed only after the capability/trait
    operation being denied has a compiler-known contract.
  - Requirement: negative structural implication is future work only.
  - Requirement: update the canonical EBNF before implementation. The grammar
    must define implication constraints as generic-parameter shorthand in
    type/evidence positions, not as an expression-level binary operator and
    not as a declaration `where` clause. EBNF must reject implication in runtime
    expressions, declaration `where` clauses, parameter type annotations such as
    `value: T => {title: String}`, field declarations, shape bodies, case
    branches, lambdas, and ordinary type aliases unless those forms explicitly
    own a generic-parameter constraint list.
  - Requirement: update parser fixtures, syntax output fixtures, formatter
    fixtures, and tree-sitter grammar from the same EBNF change. There must be
    no duplicate implication grammar in golden/scratch copies.
  - Requirement: update the formal type specification and Lean proof track.
    Proof obligations must include implication well-formedness, evidence
    soundness for field access, fail-closed unproven
    implication rejection, and evidence provenance preservation.
  - Requirement: parser, formatter, typechecker, CoreIR, VM diagnostics,
    JS/backend diagnostics, generated summaries, docs, LSP hover/completion,
    tree-sitter, and coverage inventories must all agree on implication
    syntax and supported positions.
  - Requirement: seek std-library adoption while implementing the feature.
  - Gate: add `make shape-implications-check`.
  - Make integration: run `shape-implications-check` from `make check`.
  - Acceptance: executable `.terl` tests prove implication-constrained
    functions, receiver methods, structs, impls.
  - Acceptance: adversarial tests prove missing fields, wrong field types,
    private field access, dynamic/open maps, unsupported target evidence,
    ambiguous generated fields, implication outside generic parameter
    constraints,
    field-decorator syntax, attempted runtime conversion, unproven implication,
    scoped-evidence leakage, and attempted negative structural implication all
    report stable diagnostics.
  - Acceptance: at least one std-library API uses shape implication evidence
    before the slice is marked complete, so the feature is proven against real
    library code and not only synthetic fixtures.
"#;

    let diagnostics = validate_shape_implications_section(section);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies missing proof and grammar obligations are rejected.
///
/// Inputs:
/// - A deliberately vague implication roadmap section.
///
/// Output: diagnostics naming missing requirements.
///
/// Transformation: prevents `=>` from being implemented without formal syntax
/// and proof ownership.
#[test]
fn shape_implications_rejects_vague_contract_section() {
    let section = r#"
- [ ] Add the `=>` implication arrow.
  - Requirement: make it work.
  - Gate: add `make shape-implications-check`.
"#;

    let diagnostics = validate_shape_implications_section(section);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("canonical EBNF update")),
        "expected EBNF diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Lean proof track")),
        "expected Lean diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("adversarial diagnostics")),
        "expected adversarial diagnostic: {diagnostics:?}"
    );
}

/// Verifies implication cannot be documented as a runtime conversion feature.
///
/// Inputs:
/// - A section that names the implication arrow but omits proof-only
///   non-conversion semantics.
///
/// Output: a diagnostic for missing non-conversion semantics.
///
/// Transformation: keeps `=>` from drifting into a conversion, wrapper, macro,
/// or generator mechanism.
#[test]
fn shape_implications_rejects_missing_non_conversion_semantics() {
    let section = r#"
- [ ] Add the `=>` implication arrow for compile-time structural evidence.
  - Requirement: `=>` is called the implication arrow. It is not a runtime
    execution arrow, not a conversion operator, not a generator hook, and not a
    macro system.
  - Requirement: the initial supported form is positive structural implication
    in generic parameter constraints only. `where` is reserved for runtime
    guards and is not an implication surface.
  - Requirement: generic parameter lists use implication shorthand when the
    evidence is local to that generic parameter.
  - Requirement: implication checking is fail-closed and the program is rejected
    with no runtime fallback.
  - Requirement: every accepted implication must produce typed compiler
    evidence with provenance from built-in core rules, explicit user
    declarations, generated binding manifests, shape definitions with guards,
    and trait/type facts. Ad hoc name matching is not evidence.
  - Requirement: implication evidence is scoped to the local constraint
    environment introduced by the owning generic parameter list and must not
    leak it outside.
  - Requirement: diagnostics include unproven_implication,
    ambiguous_implication, implication_violation, and implication_scope_error.
  - Requirement: implication targets start with closed structural field shapes
    and reject open/dynamic maps and `Dynamic`.
  - Requirement: Field-level implication decorators and declaration `where`
    implication clauses are not supported.
  - Requirement: negative capability implications require a compiler-known
    contract and negative structural implication is future work only.
  - Requirement: update the canonical EBNF before implementation using
    generic-parameter shorthand in type/evidence positions, not as an
    expression-level binary operator and not as a declaration `where` clause.
  - Requirement: update parser fixtures, formatter fixtures, and tree-sitter
    grammar with no duplicate implication grammar.
  - Requirement: update the formal type specification and Lean proof track with
    implication well-formedness, evidence soundness for field access,
    fail-closed unproven implication rejection, and evidence provenance
    preservation.
  - Requirement: parser, formatter, typechecker, CoreIR, VM diagnostics, LSP
    hover/completion, and coverage inventories agree.
  - Requirement: seek std-library adoption and at least one std-library API uses
    shape implication evidence.
  - Gate: make shape-implications-check and run `shape-implications-check` from
    `make check`.
  - Acceptance: executable `.terl` tests prove implication-constrained functions,
    receiver methods, structs, impls.
  - Acceptance: adversarial tests prove missing fields, implication outside
    generic parameter constraints, unproven implication, scoped-evidence
    leakage, and attempted negative structural implication.
  - Acceptance: at least one std-library API uses shape implication evidence and
    not only synthetic fixtures.
"#;

    let diagnostics = validate_shape_implications_section(section);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("proof-only non-conversion semantics")),
        "expected non-conversion diagnostic: {diagnostics:?}"
    );
}

/// Verifies the implication contract matrix cannot contain placeholder labels.
///
/// Inputs:
/// - Current required/acceptance term matrix plus an injected placeholder term.
///
/// Output: current matrix is clean and the injected term is rejected.
///
/// Transformation: prevents TODO/TBD contract wording from satisfying the
/// formal language-feature gate.
#[test]
fn shape_implications_rejects_placeholder_contract_terms() {
    let diagnostics = validate_no_placeholder_contract_terms();

    assert!(
        diagnostics.is_empty(),
        "shape implication contract terms must not contain placeholders: {diagnostics:?}"
    );

    let injected = RequiredTerm {
        label: "todo implication rule",
        fragments: &["compile-time evidence"],
    };
    let injected_diagnostics = validate_required_term_has_no_placeholder_fragments(&injected);

    assert!(
        injected_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected_diagnostics:?}"
    );
}

/// Verifies subsection extraction stops at the next Markdown header.
///
/// Inputs:
/// - A document with shape implications followed by another subsection.
///
/// Output: only shape implication text.
///
/// Transformation: keeps unrelated roadmap sections from satisfying this gate.
#[test]
fn shape_implications_extracts_only_named_section() {
    let document = r#"
### Shape Implications

body

### Other Feature

not body
"#;

    let section = extract_section(document, SECTION_HEADER).expect("section exists");

    assert_eq!(section, "body");
}
