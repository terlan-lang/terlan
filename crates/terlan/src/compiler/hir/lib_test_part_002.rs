
/// Verifies interface rendering preserves same-name same-arity overloads.
///
/// Inputs:
/// - A provider interface with two public `pick/1` signatures distinguished by
///   parameter and return type.
///
/// Output:
/// - Test passes when rendered `.typi` text contains both overloads and the
///   reparsed interface stores both candidates in `function_overloads`.
///
/// Transformation:
/// - Converts interface syntax to `ModuleInterface`, renders it to summary
///   text, reparses the summary, and inspects overload metadata without relying
///   on the compatibility single-signature map.
#[test]
fn interface_rendering_preserves_function_overloads() {
    let source = "\
module overload.Provider.\n\
\n\
pub pick(value: Int): Int.\n\
pub pick(value: String): String.\n\
";
    let parsed = parse_interface_module_as_syntax_output(source).expect("parse overload interface");
    let interface = syntax_module_output_to_interface(&parsed);
    let rendered = interface.to_terlan_interface_text();

    assert!(
        rendered.contains("pub pick(value: Int): Int."),
        "rendered interface should contain Int overload:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub pick(value: String): String."),
        "rendered interface should contain String overload:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered overload interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let overloads = reparsed_interface
        .function_overloads
        .get(&("pick".to_string(), 1))
        .expect("pick/1 overloads");

    assert_eq!(overloads.len(), 2);
    assert!(overloads
        .iter()
        .any(|signature| signature.return_type == "Int"));
    assert!(overloads
        .iter()
        .any(|signature| signature.return_type == "String"));
}

/// Verifies resolved source modules preserve implemented overloads in summaries.
///
/// Inputs:
/// - A source module with two implemented public `pick/1` overloads
///   distinguished by parameter type.
///
/// Output:
/// - Test passes when the resolved interface summary text contains both
///   overload signatures.
///
/// Transformation:
/// - Parses ordinary source syntax, resolves it through HIR, then renders the
///   module interface exactly as std summary generation does.
#[test]
fn resolved_interface_rendering_preserves_source_function_overloads() {
    let source = "\
module overload.SourceProvider.\n\
\n\
pub pick(value: Int): String ->\n\
    value.to_string().\n\
\n\
pub pick(value: String): String ->\n\
    value.\n\
";
    let parsed = parse_module_as_syntax_output(source).expect("parse overload source");
    let resolved = resolve_syntax_module_output(&parsed);
    let rendered = resolved.module.interface.to_terlan_interface_type_text();

    assert!(
        rendered.contains("pub pick(value: Int): String."),
        "rendered source interface should contain Int overload:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub pick(value: String): String."),
        "rendered source interface should contain String overload:\n{}",
        rendered
    );
}

/// Verifies interface rendering preserves public pure function metadata.
///
/// Inputs:
/// - A source module with a public `@pure` function and public `@pure`
///   receiver method.
///
/// Output:
/// - Test passes when direct HIR interface metadata and rendered/reparsed
///   summary metadata both mark those signatures pure.
///
/// Transformation:
/// - Converts source syntax to a resolved interface, renders summary text,
///   reparses that summary as an interface module, and compares the preserved
///   compiler-owned purity flag without relying on implementation bodies.
#[test]
fn interface_rendering_preserves_pure_function_metadata() {
    let source = "\
module purity.InterfaceProvider.\n\
\n\
pub struct Box {\n\
    value: Int\n\
}.\n\
\n\
pub trait Read[T] {\n\
    @pure\n\
    read_trait(value: T): Int.\n\
}.\n\
\n\
@pure\n\
pub value(input: Int): Int ->\n\
    input.\n\
\n\
@pure\n\
pub (box: Box) read(): Int ->\n\
    0.\n\
";
    let parsed = parse_module_as_syntax_output(source).expect("parse pure interface source");
    let resolved = resolve_syntax_module_output(&parsed);
    let interface = &resolved.module.interface;

    let value_signature = interface
        .functions
        .get(&("value".to_string(), 1))
        .expect("direct pure value signature");
    assert!(value_signature.pure);

    let read_signature = interface
        .functions
        .get(&("read".to_string(), 1))
        .expect("direct pure receiver signature");
    assert!(read_signature.pure);
    assert!(read_signature.receiver_method);
    assert!(interface.traits["Read"].methods["read_trait"].pure);

    let rendered = interface.to_terlan_interface_type_text();
    assert!(
        rendered.contains("@pure\npub value(input: Int): Int."),
        "rendered interface should preserve pure function metadata:\n{}",
        rendered
    );
    assert!(
        rendered.contains("@pure\npub (box: Box) read(): Int."),
        "rendered interface should preserve pure method metadata:\n{}",
        rendered
    );
    assert!(
        rendered.contains("    @pure\n    read_trait(value: T): Int."),
        "rendered interface should preserve pure trait method metadata:\n{}",
        rendered
    );

    let reparsed =
        parse_interface_module_as_syntax_output(&rendered).expect("parse rendered pure interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert!(
        reparsed_interface
            .functions
            .get(&("value".to_string(), 1))
            .expect("reparsed pure value signature")
            .pure
    );
    assert!(
        reparsed_interface
            .functions
            .get(&("read".to_string(), 1))
            .expect("reparsed pure receiver signature")
            .pure
    );
    assert!(reparsed_interface.traits["Read"].methods["read_trait"].pure);
}

/// Verifies public struct fields survive `.typi` rendering and parsing.
///
/// Inputs:
/// - A syntax-output module containing one public struct with two fields.
///
/// Output:
/// - Test passes when direct and reparsed interfaces both expose the public
///   struct field signatures.
///
/// Transformation:
/// - Converts source to interface metadata, renders that metadata as
///   Terlan interface text, reparses it, and compares the resulting
///   span-free field signatures.
#[test]
fn interface_rendering_preserves_public_struct_fields() {
    let module = parse_module_as_syntax_output(
        "\
module interface_struct_fields.\n\
\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String,\n\
    #internal_id: String\n\
}.\n",
    )
    .expect("parse struct field source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let fields = interface
        .struct_fields
        .get("Error")
        .expect("direct struct field metadata");
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].name, "code");
    assert_eq!(fields[0].annotation, "Atom");
    assert!(!fields[0].is_private);
    assert_eq!(fields[1].name, "message");
    assert_eq!(fields[1].annotation, "String");
    assert!(!fields[1].is_private);
    assert_eq!(fields[2].name, "internal_id");
    assert_eq!(fields[2].annotation, "String");
    assert!(fields[2].is_private);

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub struct Error"),
        "rendered interface should preserve struct declaration:\n{}",
        rendered
    );
    assert!(
        rendered.contains("#internal_id: String"),
        "rendered interface should preserve private field syntax:\n{}",
        rendered
    );
    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered struct interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let reparsed_fields = reparsed_interface
        .struct_fields
        .get("Error")
        .expect("reparsed struct field metadata");
    assert_eq!(reparsed_fields, fields);
}

/// Verifies public generic struct implications survive interface round trips.
#[test]
fn interface_rendering_preserves_generic_struct_implications() {
    let module = parse_module_as_syntax_output(
        "\
module interface_generic_struct.\n\
\n\
pub struct Page[T => {title: String}] {\n\
    model: T\n\
}.\n",
    )
    .expect("parse generic struct source fixture");

    let interface = syntax_module_output_to_interface(&module);
    assert_eq!(
        interface.type_params.get("Page"),
        Some(&vec!["T => {title: String}".to_string()])
    );

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub struct Page[T => {title: String}]"),
        "rendered interface should preserve generic implication:\n{rendered}"
    );
    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered generic struct interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert_eq!(reparsed_interface.type_params, interface.type_params);
    assert_eq!(reparsed_interface.struct_fields, interface.struct_fields);
}

/// Verifies public generic implementation implications survive interface
/// rendering and reparsing without changing the semantic trait argument.
#[test]
fn interface_rendering_preserves_generic_trait_impl_implications() {
    let module = parse_module_as_syntax_output(
        r#"
module interface_generic_impl.

pub trait Render[T] {
    render(value: T): String.
}.

pub impl Render[T => {title: String}] for T {
    render(value: T): String -> value.title.
}.
"#,
    )
    .expect("parse generic trait impl source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let conformance = interface
        .trait_conformances
        .iter()
        .find(|conformance| conformance.trait_ref == "Render[T]")
        .expect("generic trait implementation conformance");
    assert_eq!(
        conformance.generic_params,
        vec!["T => {title: String}".to_string()]
    );

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub impl Render[T => {title: String}] for T"),
        "rendered interface should preserve impl implication:\n{rendered}"
    );
    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered generic trait impl interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert_eq!(
        reparsed_interface.trait_conformances,
        interface.trait_conformances
    );
}

/// Verifies public raw shape declarations survive interface generation.
///
/// Inputs:
/// - A syntax-output module with one public shape and one local shape.
///
/// Output:
/// - Test passes when generated interface text preserves only the public shape
///   and reparsing the interface rebuilds the same shape metadata.
///
/// Transformation:
/// - Keeps shape declarations in generated summaries without enabling shape
///   expansion or runtime matching semantics.
#[test]
fn interface_rendering_preserves_public_shape_declarations() {
    let module = parse_module_as_syntax_output(
        "\
module interface_shapes.\n\
\n\
/**\n\
 * Matches a user asset path.\n\
 */\n\
pub shape UserAsset(id, file) =\n\
    \"users/${id: Int}/assets/${file}\".\n\
\n\
/**\n\
 * Internal response shape.\n\
 */\n\
shape InternalResponse(body) =\n\
    {status, body} where status in 200..299.\n",
    )
    .expect("parse shape source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let shape = interface
        .shapes
        .get("UserAsset")
        .expect("direct shape metadata");
    assert_eq!(shape.name, "UserAsset");
    assert!(shape.signature.contains("pub shape UserAsset"));
    assert_eq!(shape.docs, vec!["Matches a user asset path.".to_string()]);
    assert!(!interface.shapes.contains_key("InternalResponse"));

    let rendered = interface.to_terlan_interface_text();
    assert!(
        rendered.contains("pub shape UserAsset"),
        "rendered interface should preserve public shape declaration:\n{}",
        rendered
    );
    assert!(
        !rendered.contains("InternalResponse"),
        "rendered interface should omit local shape declarations:\n{}",
        rendered
    );
    let reparsed =
        parse_interface_module_as_syntax_output(&rendered).expect("parse rendered shape interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert_eq!(reparsed_interface.shapes.get("UserAsset"), Some(shape));
    assert!(!reparsed_interface.shapes.contains_key("InternalResponse"));
}

/// Verifies callable default parameters survive interface generation.
///
/// Inputs:
/// - A source module containing a public function, receiver method, trait
///   method, and constructor with defaulted parameters.
///
/// Output:
/// - Test passes when generated interface text renders the defaults and the
///   reparsed interface keeps the same default metadata.
///
/// Transformation:
/// - Converts source syntax output to HIR interface metadata, renders a `.typi`
///   compatible summary, reparses it as interface source, and checks that
///   default parameter text remains available to downstream phases.
#[test]
fn interface_rendering_preserves_callable_parameter_defaults() {
    let module = parse_module_as_syntax_output(
        "\
module interface_callable_defaults.\n\
\n\
pub type User = {name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (name: String, active: Bool = True): User ->\n\
        User(name, active)\n\
}.\n\
\n\
pub trait Label[T] {\n\
    label(value: T, separator: String = \":\"): String.\n\
}.\n\
\n\
pub greet(name: String, excited: Bool = False): String ->\n\
    name.\n\
\n\
pub (name: String) pad(width: Int = 2): String ->\n\
    name.\n",
    )
    .expect("parse callable defaults source fixture");

    let interface = syntax_module_output_to_interface(&module);
    let rendered = interface.to_terlan_interface_type_text();
    assert!(
        rendered.contains("pub greet(name: String, excited: Bool = False): String."),
        "rendered interface should preserve function defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("pub (name: String) pad(width: Int = 2): String."),
        "rendered interface should preserve receiver-method defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("label(value: T, separator: String = \":\"): String."),
        "rendered interface should preserve trait-method defaults:\n{}",
        rendered
    );
    assert!(
        rendered.contains("(name: String, active: Bool = True): User"),
        "rendered interface should preserve constructor defaults:\n{}",
        rendered
    );

    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("parse rendered callable default interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    let greet = reparsed_interface
        .functions
        .get(&("greet".to_string(), 2))
        .expect("reparsed greet signature");
    assert_eq!(greet.params[1].default_text.as_deref(), Some("False"));

    let pad = reparsed_interface
        .functions
        .get(&("pad".to_string(), 2))
        .expect("reparsed receiver method signature");
    assert_eq!(pad.params[1].default_text.as_deref(), Some("2"));

    let label = reparsed_interface
        .traits
        .get("Label")
        .and_then(|signature| signature.methods.get("label"))
        .expect("reparsed label trait method");
    assert_eq!(label.params[1].default_text.as_deref(), Some("\":\""));

    let constructor = reparsed_interface
        .constructors
        .get("User")
        .and_then(|signatures| signatures.first())
        .expect("reparsed User constructor");
    assert_eq!(constructor.params[1].default_text.as_deref(), Some("True"));
}

/// Verifies generated provider summaries with constructors and empty impls parse.
///
/// Inputs:
/// - Interface text matching a cached provider `.typi` summary for a module
///   with a public struct, constructor, trait, explicit impl, and function.
///
/// Output:
/// - Test passes when interface parsing and HIR extraction preserve the
///   provider module interface.
///
/// Transformation:
/// - Parses generated interface text and converts it back into a
///   `ModuleInterface`, catching cache summary shapes that would otherwise
///   be silently skipped by interface loading.
#[test]
fn generated_provider_interface_with_empty_impl_parses() {
    let source = "\
module people.Provider.\n\
\n\
pub type ExternalUser.\n\
\n\
pub new_user(name: String): ExternalUser.\n\
\n\
pub trait Named[T] {\n\
    name(value: T): String.\n\
}.\n\
\n\
pub impl Named[ExternalUser] for ExternalUser {\n\
}.\n\
\n\
pub constructor ExternalUser {\n\
    (name: String): ExternalUser ->\n\
        terlan_interface_constructor\n\
}.\n";

    let parsed = parse_interface_module_as_syntax_output(source)
        .expect("parse generated provider interface summary");
    let interface = syntax_module_output_to_interface(&parsed);

    assert_eq!(interface.module, "people.Provider");
    assert!(interface.public_types.contains("ExternalUser"));
    assert!(interface.traits.contains_key("Named"));
    assert_eq!(interface.trait_conformances.len(), 1);
    assert!(interface
        .functions
        .contains_key(&("new_user".to_string(), 1)));
}

/// Asserts one trait conformance fact exists in an interface snapshot.
///
/// Inputs:
/// - `interface`: module interface to inspect.
/// - `trait_ref`: expected normalized trait reference.
/// - `for_type`: expected normalized implementation type.
/// - `source`: expected conformance source category.
/// - `is_negative`: expected conformance polarity.
///
/// Output:
/// - Panics when the conformance fact is missing.
///
/// Transformation:
/// - Performs an exact metadata lookup without inspecting source text.
fn assert_trait_conformance(
    interface: &ModuleInterface,
    trait_ref: &str,
    for_type: &str,
    source: TraitConformanceSource,
    is_negative: bool,
) {
    assert!(
        interface.trait_conformances.iter().any(|conformance| {
            conformance.trait_ref == trait_ref
                && conformance.for_type == for_type
                && conformance.is_negative == is_negative
                && conformance.source == source
                && conformance.public
        }),
        "missing conformance {trait_ref} for {for_type} via {:?}: {:?}",
        source,
        interface.trait_conformances
    );
}

/// Verifies exact duplicate function shapes remain invalid.
///
/// Inputs:
/// - A source module declaring two `pick/1` functions with the same parameter
///   and return annotations.
///
/// Output:
/// - Test passes when HIR reports a duplicate function definition diagnostic.
///
/// Transformation:
/// - Parses canonical source, resolves syntax output to HIR, and checks that
///   overload relaxation only applies to distinct type shapes.
#[test]
fn hir_rejects_duplicate_function_shape() {
    let module = parse_module_as_syntax_output(
        "\
module duplicate_function_shape.\n\
\n\
pub pick(value: Int): Int ->\n\
    value.\n\
\n\
pub pick(other: Int): Int ->\n\
    other.\n\
",
    )
    .expect("parse duplicate function shape fixture");
    let resolved = resolve_syntax_module_output(&module).module;

    assert!(
        resolved.diagnostics.iter().any(|diag| diag
            .message
            .contains("duplicate function definition: pick / 1")),
        "expected duplicate diagnostic, got {:?}",
        resolved.diagnostics
    );
}

#[test]
fn hir_accepts_canonical_syntax_contract() {
    let contract =
        cached_canonical_terlan_syntax_contract().expect("cached canonical syntax contract");

    let diagnostics = validate_syntax_contract(contract);
    assert!(
        diagnostics.is_empty(),
        "unexpected syntax contract diagnostics: {diagnostics:?}"
    );
}

#[test]
fn hir_rejects_broken_syntax_contract() {
    let mut contract =
        canonical_terlan_syntax_contract().expect("compile canonical syntax contract");
    contract.entry_rule = Some("Program".to_string());
    let expr_rule = contract.rule("Expr").expect("Expr rule").clone();
    let expr_rule_index = contract
        .rules
        .iter()
        .position(|rule| rule.name == expr_rule.name)
        .expect("Expr rule index");
    contract.rules[expr_rule_index].expr.kind = EbnfGrammarExprKind::Terminal {
        value: "broken".to_string(),
    };

    let diagnostics = validate_syntax_contract(&contract);
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("entry rule")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message == "syntax rule Expr must reference AssignExpr"));
}

#[test]
fn resolve_syntax_output_records_function_symbols() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module syntax_resolve.

pub add(Value: Int): Int ->
    Value + 1.
"#,
    )
    .expect("parse syntax output");

    let resolved = resolve_syntax_module_output(&syntax_module);
    let symbol = resolved
        .module
        .function_symbols
        .get(&("add".to_string(), 1))
        .expect("add symbol");
    assert_eq!(symbol.return_type, "Int");
    assert!(symbol.exported);
    assert!(resolved.module.diagnostics.is_empty());
}

#[test]
fn resolve_syntax_output_rejects_source_export_payloads() {
    let mut syntax_module = crate::terlan_syntax::parse_interface_module_as_syntax_output(
        r#"
module syntax_resolve_source_export_payload.

export add/1.
"#,
    )
    .expect("parse interface syntax output");
    syntax_module.source_kind = SyntaxSourceKind::Module;

    let resolved = resolve_syntax_module_output(&syntax_module);
    assert!(resolved.module.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("source export declarations are not part of canonical Terlan")
    }));
    assert!(!resolved
        .module
        .function_symbols
        .contains_key(&("add".to_string(), 1)));
}

/// Verifies source-tree resolution rejects source-mode export
/// payloads.
///
/// Inputs:
/// - An interface-parsed AST module containing an `Export` payload.
///
/// Output:
/// - Test passes when AST resolution reports the canonical source-export
///   diagnostic and does not create a function symbol from the interface
///   export summary.
///
/// Transformation:
/// - Feeds an interface export-summary AST payload through the source-oriented
///   compatibility resolver to prove it no longer treats `export` as a
///   normal source visibility mechanism.
#[test]
fn formal_hir_syntax_output_resolves_interface_surface() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
//! Public docs.
module formal_syntax_iface.

/// Item collection.
pub type Items[T] =
    List[T].

/// Builds item collections.
pub constructor Items[T] {
    (Values: List[T]): Items[T] ->
        Values
}.

/// Shows values.
	pub trait Show[A] {
	  /// Converts to text.
	  show(Value: A): Text.
	}.

	/// Adds one.
	pub add(Value: Int): Int ->
	    Value + 1.
"#,
    )
    .expect("parse syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    assert_eq!(interface.module, "formal_syntax_iface");
    assert_eq!(interface.docs, vec!["Public docs."]);
    assert!(interface.public_types.contains("Items"));
    assert_eq!(
        interface.type_params.get("Items"),
        Some(&vec!["T".to_string()])
    );
    assert_eq!(interface.constructors.get("Items").map(Vec::len), Some(1));
    assert_eq!(
        interface.traits["Show"].methods["show"].docs,
        vec!["Converts to text."]
    );
    assert_eq!(
        interface.functions[&("add".to_string(), 1)].return_type,
        "Int"
    );

    let resolved = resolve_syntax_module_output(&syntax_module);
    let symbol = resolved
        .module
        .function_symbols
        .get(&("add".to_string(), 1))
        .expect("add symbol");
    assert!(symbol.exported);
    assert!(resolved.module.diagnostics.is_empty());
}

/// Verifies higher-kinded type parameters survive HIR interface extraction.
///
/// Inputs:
/// - Public trait syntax with a unary higher-kinded parameter.
///
/// Output:
/// - Interface metadata and rendered interface text preserving `F[_]`.
///
/// Transformation:
/// - Parses source through syntax output, lowers to HIR interface metadata, and
///   renders the importable `.typi` text without erasing kind arity.
#[test]
fn formal_hir_preserves_higher_kinded_trait_params() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module formal_hkt_iface.

pub trait Functor[F[_]] {
    map(value: F[T], fn: (T) -> U): F[U].
}.
"#,
    )
    .expect("parse hkt syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    assert_eq!(
        interface.traits["Functor"].type_params,
        vec!["F[_]".to_string()]
    );
    assert!(interface
        .to_terlan_interface_text()
        .contains("pub trait Functor[F[_]]"));
}

/// Verifies higher-kinded callable params survive HIR interface extraction.
///
/// Inputs:
/// - Public generic function syntax with a unary HKT parameter and a generic
///   bound.
///
/// Output:
/// - Interface metadata and rendered interface text preserving `F[_]`, `A`,
///   and the generic bound text.
///
/// Transformation:
/// - Parses source through syntax output, lowers to HIR interface metadata, and
///   renders the importable `.typi` text without erasing callable generic
///   parameters.
#[test]
fn formal_hir_preserves_higher_kinded_function_params() {
    let syntax_module = parse_module_as_syntax_output(
        r#"
module formal_hkt_function_iface.

pub trait Show[T] {
    show(value: T): String.
}.

pub identity[F[_], A]<A: Show>(value: F[A]): F[A] ->
    value.

pub trait Functor[F[_]] {
    map[A, B](value: F[A], f: (A) -> B): F[B].
}.
"#,
    )
    .expect("parse hkt function syntax output");

    let interface = syntax_module_output_to_interface(&syntax_module);
    let signature = interface
        .functions
        .get(&("identity".to_string(), 1))
        .expect("identity signature");

    assert_eq!(signature.generic_params, vec!["F[_]", "A"]);
    assert_eq!(signature.generic_bounds, vec!["A: Show"]);
    assert!(interface
        .to_terlan_interface_text()
        .contains("pub identity[F[_], A]<A: Show>(value: F[A]): F[A]."));
    assert!(interface
        .to_terlan_interface_text()
        .contains("map[A, B](value: F[A], f: (A) -> B): F[B]."));
}
