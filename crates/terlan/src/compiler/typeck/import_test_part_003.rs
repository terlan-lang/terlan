
/// Verifies imported public struct type identity does not allow raw
/// construction outside the defining module.
///
/// Inputs:
/// - A provider interface declaring public struct `Point`.
/// - A consumer module importing that type and attempting `#Point { ... }`.
///
/// Output:
/// - Test passes when typechecking rejects the raw imported struct literal
///   before CoreIR/backend emission.
///
/// Transformation:
/// - Resolves a consumer against an explicit interface map and checks that
///   record construction visibility is enforced semantically, independent
///   of syntax acceptance.
#[test]
fn syntax_output_rejects_raw_imported_struct_construction_without_constructor() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct Point {\n\
    x: Int\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module raw_imported_struct_construction_boundary.\n\
\n\
import type provider.Point.\n\
\n\
pub make(): Dynamic ->\n\
    Point { x: 1 }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot raw-construct imported struct provider.Point")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported private struct fields cannot be read outside the provider.
///
/// Inputs:
/// - A provider interface declaring public struct `User` with private field
///   `#email`.
/// - A consumer importing `User` and attempting `user.#email`.
///
/// Output:
/// - Test passes when typechecking rejects cross-module private field access.
///
/// Transformation:
/// - Resolves the consumer against explicit interface metadata and checks that
///   preserved struct field visibility is enforced during expression
///   inference.
#[test]
fn syntax_output_rejects_imported_private_struct_field_access() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module private_import_boundary.\n\
\n\
import type provider.User.\n\
\n\
pub email(user: User): String ->\n\
    user.#email.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("private field email on imported struct provider.User cannot be accessed outside defining module")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported private struct fields cannot be updated outside provider.
///
/// Inputs:
/// - A provider interface declaring public struct `User` with private field
///   `#email`.
/// - A consumer importing `User` and attempting `user#User { #email = ... }`.
///
/// Output:
/// - Test passes when typechecking rejects cross-module private field update.
///
/// Transformation:
/// - Checks record-update visibility against imported interface metadata.
#[test]
fn syntax_output_rejects_imported_private_struct_field_update() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module private_import_update_boundary.\n\
\n\
import type provider.User.\n\
\n\
pub update(user: User): User ->\n\
    user#User { #email: \"next@example.com\" }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("private field email on imported struct provider.User cannot be accessed outside defining module")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported private struct fields cannot be pattern matched.
///
/// Inputs:
/// - A provider interface declaring public struct `User` with private field
///   `#email`.
/// - A consumer importing `User` and matching `User { #email = email }`.
///
/// Output:
/// - Test passes when typechecking rejects cross-module private field pattern.
///
/// Transformation:
/// - Checks record-pattern visibility against imported interface metadata.
#[test]
fn syntax_output_rejects_imported_private_struct_field_pattern() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct User {\n\
    #email: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module private_import_pattern_boundary.\n\
\n\
import type provider.User.\n\
\n\
pub read(user: User): String ->\n\
    case user {\n\
      User { #email: email } -> email\n\
    }.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("private field email on imported struct provider.User cannot be accessed outside defining module")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported public struct fields remain readable.
///
/// Inputs:
/// - A provider interface declaring public struct `User` with public field
///   `name`.
/// - A consumer importing `User` and reading `user.name`.
///
/// Output:
/// - Test passes when imported public field lookup typechecks without
///   diagnostics.
///
/// Transformation:
/// - Confirms imported struct field metadata is merged into expression
///   inference while preserving privacy checks for private fields.
#[test]
fn syntax_output_accepts_imported_public_struct_field_access() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub struct User {\n\
    name: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module public_import_boundary.\n\
\n\
import type provider.User.\n\
\n\
pub name(user: User): String ->\n\
    user.name.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies missing imported module members produce member diagnostics.
///
/// Inputs:
/// - A provider interface for `provider.Users` with one public function.
/// - A consumer importing `provider.Users` and passing `Users.missing` as a
///   function value.
///
/// Output:
/// - Test passes when typechecking reports that the imported module has no
///   exported `missing` function.
///
/// Transformation:
/// - Resolves the consumer against the provider interface and exercises the
///   module-member function value path before ordinary struct field access can
///   produce a misleading fallback diagnostic.
#[test]
fn syntax_output_rejects_missing_imported_module_member_function_value() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.Users.\n\
\n\
pub index(value: Int): Int ->\n\
    value.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import provider.Users.\n\
\n\
pub run(f: (Int) -> Int): Int ->\n\
    f(1).\n\
\n\
pub value(): Int ->\n\
    run(Users.missing).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("module `provider.Users` has no exported function `missing`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Builds a minimal imported module interface with public function signatures.
///
/// Inputs:
/// - `module`: fully qualified provider module name.
/// - `functions`: function names, parameter names/types, and return types.
///
/// Output:
/// - `ModuleInterface` suitable for import-resolution/typecheck unit tests.
///
/// Transformation:
/// - Populates both compatibility function lookup and overload lookup so tests
///   exercise the same interface shape produced by generated summaries.
fn public_function_interface(
    module: &str,
    functions: &[(&str, Vec<(&str, &str)>, &str)],
) -> ModuleInterface {
    let mut interface = ModuleInterface {
        module: module.to_string(),
        docs: Vec::new(),
        public_types: HashSet::new(),
        private_types: HashSet::new(),
        opaque_types: HashSet::new(),
        type_params: HashMap::new(),
        type_bodies: HashMap::new(),
        struct_fields: HashMap::new(),
        type_docs: HashMap::new(),
        shapes: HashMap::new(),
        traits: HashMap::new(),
        trait_conformances: Vec::new(),
        constructors: HashMap::new(),
        functions: HashMap::new(),
        function_overloads: HashMap::new(),
        constants: HashMap::new(),
        const_functions: HashMap::new(),
        expression_macros: HashMap::new(),
        valued_unions: HashMap::new(),
        associated_constants: HashMap::new(),
    };

    for (name, params, return_type) in functions {
        let signature = FunctionSignature {
            name: (*name).to_string(),
            generic_params: Vec::new(),
            params: params
                .iter()
                .map(|(param_name, annotation)| ParamSignature {
                    name: (*param_name).to_string(),
                    annotation: (*annotation).to_string(),
                    is_mutable: false,
                    default_text: None,
                })
                .collect::<Vec<_>>(),
            return_type: (*return_type).to_string(),
            generic_bounds: Vec::new(),
            receiver_method: false,
            receiver_mutable: false,
            public: true,
            pure: false,
            docs: Vec::new(),
        };
        let key = ((*name).to_string(), signature.params.len());
        interface.functions.insert(key.clone(), signature.clone());
        interface
            .function_overloads
            .entry(key)
            .or_default()
            .push(signature);
    }

    interface
}

/// Verifies ambiguous imported module-member function values resolve by call context.
///
/// Inputs:
/// - A provider interface for `provider.Users` with two public `index`
///   overloads.
/// - A consumer importing `provider.Users` and passing `Users.index` as a
///   function value.
///
/// Output:
/// - Test passes when typechecking uses `run`'s expected `(Int) -> Int`
///   parameter type to select the unary overload.
///
/// Transformation:
/// - Exercises the 0.0.5 contextual rule: module-member function values may be
///   overloaded when the surrounding call supplies an expected function type.
#[test]
fn syntax_output_resolves_ambiguous_imported_module_member_function_value_from_call_context() {
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Users".to_string(),
        public_function_interface(
            "provider.Users",
            &[
                ("index", vec![("value", "Int")], "Int"),
                ("index", vec![("value", "Int"), ("step", "Int")], "Int"),
            ],
        ),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import provider.Users.\n\
\n\
pub run(f: (Int) -> Int): Int ->\n\
    f(1).\n\
\n\
pub value(): Int ->\n\
    run(Users.index).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies ambiguous imported module-member function values resolve by return type.
///
/// Inputs:
/// - A provider interface for `provider.Users` with two public `index`
///   overloads.
/// - A consumer returning `Users.index` from a function with a function type
///   alias return.
///
/// Output:
/// - Test passes when typechecking uses the declared return type to select the
///   unary overload.
///
/// Transformation:
/// - Exercises the return-position contextual rule for module-member function
///   values without requiring a wrapper lambda.
#[test]
fn syntax_output_resolves_ambiguous_imported_module_member_function_value_from_return_context() {
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Users".to_string(),
        public_function_interface(
            "provider.Users",
            &[
                ("index", vec![("value", "Int")], "Int"),
                ("index", vec![("value", "Int"), ("step", "Int")], "Int"),
            ],
        ),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import provider.Users.\n\
\n\
pub type Indexer = (Int) -> Int.\n\
\n\
pub value(): Indexer ->\n\
    Users.index.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies ambiguous imported module-member function values still need context.
///
/// Inputs:
/// - A provider interface for `provider.Users` with two public `index`
///   overloads.
/// - A consumer returning `Users.index` from a function whose return type does
///   not provide a function shape.
///
/// Output:
/// - Test passes when typechecking reports ambiguity because no function-valued
///   expected type is available.
///
/// Transformation:
/// - Protects ordinary field-access inference from guessing an overload when
///   the surrounding context is not a concrete function type.
#[test]
fn syntax_output_rejects_ambiguous_imported_module_member_function_value_without_function_context()
{
    let mut interfaces = HashMap::new();
    interfaces.insert(
        "provider.Users".to_string(),
        public_function_interface(
            "provider.Users",
            &[
                ("index", vec![("value", "Int")], "Int"),
                ("index", vec![("value", "Int"), ("step", "Int")], "Int"),
            ],
        ),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import provider.Users.\n\
\n\
pub value(): Dynamic ->\n\
    Users.index.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("module-member function value `Users.index` is ambiguous")),
        "diagnostics: {:?}",
        diagnostics
    );
}

include!("import_test/module_member_context_test.rs");
include!("import_test/transparent_union_alias_test.rs");
