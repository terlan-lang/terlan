use crate::terlan_syntax::parse_module;
use crate::terlan_syntax::parse_tree::{Decl, Pattern};

/// Verifies local bindings may still be named `shape`.
///
/// Inputs:
/// - A valid function body with a local `let shape = ...` binding.
///
/// Output:
/// - Parsed module with no shape-synonym reservation diagnostic.
///
/// Transformation:
/// - Proves `shape` remains contextual declaration syntax only and does
///   not become a broad reserved word in ordinary expression bindings.
#[test]
fn parses_local_binding_named_shape() {
    let source = r#"
module shape_local_binding_name.

pub run(): Int ->
    let shape = 1;
    shape.
"#;

    parse_module(source).expect("local binding named shape should parse");
}

/// Verifies pattern bindings may still be named `shape`.
///
/// Inputs:
/// - A case expression whose variable pattern is named `shape`.
///
/// Output:
/// - Parsed module with no shape-synonym reservation diagnostic.
///
/// Transformation:
/// - Keeps future `shape Name(...) = ...` declarations from polluting
///   normal match-variable names.
#[test]
fn parses_pattern_binding_named_shape() {
    let source = r#"
module shape_pattern_binding_name.

pub run(value: Int): Int ->
    case value {
        shape -> shape
    }.
"#;

    parse_module(source).expect("pattern binding named shape should parse");
}

/// Verifies the previous `shape ... => ...` proposal no longer consumes
/// the implication arrow.
///
/// Inputs:
/// - A shape-like declaration using the old fat-arrow spelling.
///
/// Output:
/// - A parser error that is not the reserved shape-synonym diagnostic.
///
/// Transformation:
/// - Keeps `=>` available for future compile-time implication semantics
///   instead of reserving it as a shape-synonym body marker.
#[test]
fn old_shape_fat_arrow_spelling_is_not_shape_synonym_surface() {
    let source = r#"
module shape_fat_arrow_not_reserved.

shape OkResponse(body) =>
    body.
"#;

    let err = parse_module(source).expect_err("old shape fat-arrow spelling should fail");
    assert_ne!(err.message, "shape synonym names must be upper-case");
}

/// Verifies `shape ... -> ...` does not become shape-synonym syntax.
///
/// Inputs:
/// - A shape-like declaration using the ordinary runtime function arrow.
///
/// Output:
/// - A parser error that is not the reserved shape-synonym diagnostic.
///
/// Transformation:
/// - Keeps the shape-synonym reservation scoped to the agreed
///   `shape Name(...) = ...` spelling and prevents `->` from becoming a
///   second shape body marker.
#[test]
fn old_shape_runtime_arrow_spelling_is_not_shape_synonym_surface() {
    let source = r#"
module shape_runtime_arrow_not_reserved.

shape OkResponse(body) ->
    body.
"#;

    let err = parse_module(source).expect_err("shape runtime-arrow spelling should fail");
    assert_ne!(err.message, "shape synonym names must be upper-case");
}

/// Verifies traits may provide default method bodies.
///
/// Inputs:
/// - A trait with one signature-only method and one default method.
///
/// Output:
/// - Trait method metadata indicating which method owns a default body.
///
/// Transformation:
/// - Parses default trait behavior without introducing an external impl
///   declaration, matching the Java-style default-method model.

/// Verifies traits may provide default method bodies.
///
/// Inputs:
/// - A trait with one signature-only method and one default method.
///
/// Output:
/// - Trait method metadata indicating which method owns a default body.
///
/// Transformation:
/// - Parses default trait behavior without introducing an external impl
///   declaration, matching the Java-style default-method model.
#[test]
fn formal_trait_conformance_syntax_supports_trait_default_methods() {
    let module = parse_module(
        r#"
            module traits.conformance.defaults.

            pub trait Show[T] {
              to_string(value: T): String.
              debug(value: T): String -> to_string(value).
            }.
            "#,
    )
    .expect("parse default trait method");

    let Decl::Trait(show) = &module.declarations[0] else {
        panic!("expected Show trait");
    };
    assert_eq!(show.methods.len(), 2);
    assert!(show.methods[0].default_body.is_none());
    assert!(show.methods[1].default_body.is_some());
}

/// Verifies trait method parameters may require mutability.
///
/// Inputs:
/// - A trait method with `mut` on its first parameter.
///
/// Output:
/// - Trait method parameter metadata preserving `is_mutable`.
///
/// Transformation:
/// - Parses mutable parameter syntax in trait contracts so collection
///   mutation traits can express receiver-like mutation requirements.

/// Verifies trait method parameters may require mutability.
///
/// Inputs:
/// - A trait method with `mut` on its first parameter.
///
/// Output:
/// - Trait method parameter metadata preserving `is_mutable`.
///
/// Transformation:
/// - Parses mutable parameter syntax in trait contracts so collection
///   mutation traits can express receiver-like mutation requirements.
#[test]
fn formal_trait_methods_preserve_mutable_parameters() {
    let module = parse_module(
        r#"
            module traits.mutable.params.

            pub trait IndexSet[C, I, T] {
              set_at(mut collection: C, index: I, value: T): Unit.
            }.
            "#,
    )
    .expect("parse mutable trait parameter");

    let Decl::Trait(index_set) = &module.declarations[0] else {
        panic!("expected IndexSet trait");
    };
    let method = &index_set.methods[0];
    assert_eq!(method.params.len(), 3);
    assert!(method.params[0].is_mutable);
    assert!(!method.params[1].is_mutable);
    assert!(!method.params[2].is_mutable);
}

/// Verifies canonical callable constraint-list parsing.
///
/// Inputs:
/// - A module containing a generic function with `[Eq[A], Show[A]]` after
///   its parameter list.
///
/// Output:
/// - Parsed function declaration with preserved generic-bound strings.
///
/// Transformation:
/// - Exercises the canonical EBNF constraint-list position and confirms
///   constraints are kept for typechecker lowering.
#[test]
fn parses_function_declaration_with_constraint_list() {
    let source = r#"
module bounds_demo.

pub debug[A](X: A, Y: A)[Eq[A], Show[A]]: Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> "neq"
    }.
"#;

    let module = parse_module(source).expect("parse constraint-list function");
    let function = match &module.declarations[0] {
        Decl::Function(function) => function,
        _ => panic!("expected function declaration"),
    };
    assert_eq!(function.name, "debug");
    assert_eq!(function.params.len(), 2);
    assert_eq!(
        function.generic_bounds,
        vec!["Eq[A]".to_string(), "Show[A]".to_string()]
    );
}

/// Verifies function-like declarations accept trailing default parameters.
///
/// Inputs:
/// - A function and receiver method with trailing default values.
///
/// Output:
/// - Parsed callable declarations preserving the default expressions.
///
/// Transformation:
/// - Exercises the 0.0.5 callable default-parameter syntax without
///   requiring call-site omission semantics yet.
#[test]
fn parses_function_and_method_default_parameters() {
    let source = r#"
module defaults_demo.

pub add(X: Int, Step: Int = 1): Int ->
    X + Step.

pub (value: Int) clamp(Min: Int = 0, Max: Int = 10): Int ->
    value.
"#;

    let module = parse_module(source).expect("parse callable defaults");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert!(function.params[1].default.is_some());

    let Decl::Method(method) = &module.declarations[1] else {
        panic!("expected method declaration");
    };
    assert!(method.params[0].default.is_some());
    assert!(method.params[1].default.is_some());
}

/// Verifies ordinary named parameters still parse while typed pattern heads are
/// supported.
///
/// Inputs:
/// - One expression-bodied function and one clause-style function using
///   simple named parameters.
///
/// Output:
/// - Parsed function declarations preserving parameter names and types.
///
/// Transformation:
/// - Acts as a positive control for the pattern-parameter reservation
///   lookahead so it cannot accidentally reject canonical named parameter
///   forms.
#[test]
fn parses_named_parameters_while_function_head_patterns_are_reserved() {
    let source = r#"
module function_head_pattern_named_params.

pub full_name(user: User): String ->
    user.name.

pub initials(first: String, family: String): String.
initials(first, family) ->
    first + family.
"#;

    let module = parse_module(source).expect("named parameters should parse");
    assert_eq!(module.declarations.len(), 2);

    let Decl::Function(full_name) = &module.declarations[0] else {
        panic!("expected expression-bodied function");
    };
    assert_eq!(full_name.params[0].name, "user");
    assert_eq!(full_name.params[0].annotation.text, "User");

    let Decl::Function(initials) = &module.declarations[1] else {
        panic!("expected clause-style function");
    };
    assert_eq!(initials.params.len(), 2);
    assert_eq!(initials.params[0].name, "first");
    assert_eq!(initials.params[1].name, "family");
}

/// Identifiers that merely extend contextual keywords remain ordinary names.
#[test]
fn parses_plural_keyword_prefix_parameter_names() {
    let source = r#"
module keyword_prefix_parameters.

collect(modules: Int, functions: Int): Int ->
    modules + functions.
"#;

    let module = parse_module(source).expect("plural keyword prefixes should parse");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params[0].name, "modules");
    assert_eq!(function.params[1].name, "functions");
}

/// Verifies required callable parameters may not follow defaults.
///
/// Inputs:
/// - A function whose second parameter has a default and third parameter
///   does not.
///
/// Output:
/// - Parse error anchored by the shared trailing-default rule.
///
/// Transformation:
/// - Locks down deterministic callable arity before omitted-argument
///   call-site semantics are implemented.
#[test]
fn rejects_required_parameter_after_function_default_parameter() {
    let source = r#"
module defaults_bad.

pub add(X: Int = 1, Step: Int): Int ->
    X + Step.
"#;

    let err = parse_module(source).expect_err("required param after default");
    assert_eq!(err.message, "default parameters must be trailing");
}

/// Verifies typed destructuring parameters in function heads parse.
///
/// Inputs:
/// - A function attempting tuple-shaped destructuring in its parameter
///   list.
///
/// Output:
/// - Parsed callable signature with a synthetic ABI parameter and the
///   original tuple pattern preserved on the generated expression clause.
///
/// Transformation:
/// - Prevents pattern parameters from being erased into `_ArgN` clause
///   variables, so later typechecking and VM binding can reuse the normal
///   function-clause pattern path.
#[test]
fn parses_typed_function_head_pattern_parameter() {
    let source = r#"
module function_head_pattern_reserved.

pub full_name({name, family}: User): String ->
    name + family.
"#;

    let module = parse_module(source).expect("function head pattern parameter");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params.len(), 1);
    assert_eq!(function.params[0].annotation.text, "User");
    assert_eq!(function.params[0].name, "_Arg1");
    assert_eq!(function.clauses.len(), 1);
    let Pattern::Tuple(items) = &function.clauses[0].patterns[0] else {
        panic!("expected preserved tuple pattern");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(&items[0], Pattern::Var(name) if name == "name"));
    assert!(matches!(&items[1], Pattern::Var(name) if name == "family"));
}

/// Verifies typed aliasing destructuring in function-head parameters parses and
/// keeps the alias name available as the callable surface binding.
///
/// Inputs:
/// - A function attempting `{pattern} = alias: Type` in its parameter
///   list.
///
/// Output:
/// - Stable parser diagnostic describing the reserved 0.0.7 feature.
///
/// Transformation:
/// - Locks the future alias spelling without allowing it to be parsed as a
///   malformed current parameter.
#[test]
fn parses_pattern_first_alias_function_head_parameter() {
    let source = r#"
module function_head_pattern_alias_reserved.

pub full_name({name, family} = user: User): String ->
    user.id.to_string() + name + family.
"#;

    let module = parse_module(source).expect("pattern-first alias function parameter");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params[0].name, "user");
    assert_eq!(function.params[0].annotation.text, "User");
    let Pattern::Alias { alias, pattern } = &function.clauses[0].patterns[0] else {
        panic!("expected alias pattern");
    };
    assert_eq!(alias, "user");
    let Pattern::Tuple(items) = pattern.as_ref() else {
        panic!("expected preserved tuple pattern");
    };
    assert!(matches!(&items[0], Pattern::Var(name) if name == "name"));
    assert!(matches!(&items[1], Pattern::Var(name) if name == "family"));
}

/// Verifies constructor-shaped typed function-head parameters route through
/// the typed parameter parser rather than the untyped clause parser.
///
/// Inputs:
/// - A function attempting `Some(value): Option[Int]` in its parameter list.
///
/// Output:
/// - Parsed callable signature with the constructor pattern preserved on the
///   generated expression-bodied clause.
///
/// Transformation:
/// - Keeps identifier-led constructor pattern heads from being rejected at
///   the balanced lookahead boundary before the `: Type` annotation.
#[test]
fn parses_constructor_function_head_pattern_parameter() {
    let source = r#"
module function_head_constructor_pattern.

pub unwrap(Some(value): Option[Int]): Int ->
    value.
"#;

    let module = parse_module(source).expect("constructor function head pattern");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params.len(), 1);
    assert_eq!(function.params[0].annotation.text, "Option[Int]");
    let Pattern::Tuple(items) = &function.clauses[0].patterns[0] else {
        panic!("expected preserved constructor tuple pattern");
    };
    assert!(matches!(&items[0], Pattern::Atom(name) if name == "Some"));
    assert!(matches!(&items[1], Pattern::Var(name) if name == "value"));
}

/// Verifies default values remain rejected for function-head pattern
/// parameters until pattern binding semantics are stabilized.
///
/// Inputs:
/// - A function attempting a default value on a destructuring parameter.
///
/// Output:
/// - Stable parser diagnostic describing the reserved 0.0.7 feature.
///
/// Transformation:
/// - Makes the roadmap rule executable: pattern parameters with defaults
///   remain unsupported until the typechecker can prove unambiguous
///   binding behavior.
#[test]
fn rejects_defaulted_function_head_pattern_parameter() {
    let source = r#"
module function_head_pattern_default_reserved.

pub full_name({name, family}: User = default_user): String ->
    name + family.
"#;

    let err = parse_module(source).expect_err("pattern parameter default rejected");
    assert_eq!(
            err.message,
            "function-head pattern parameters do not support defaults in 0.0.7; use plain named parameters for defaults"
        );
}

/// Verifies reverse alias syntax remains rejected.
///
/// Inputs:
/// - A function attempting `name = pattern: Type` in its parameter list.
///
/// Output:
/// - Stable parser diagnostic pointing at the preferred pattern-first
///   shape.
///
/// Transformation:
/// - Locks the syntax decision that aliasing must read as pattern-first
///   matching, not assignment.
#[test]
fn rejects_reverse_alias_function_head_pattern_parameter() {
    let source = r#"
module function_head_pattern_reverse_alias.

pub full_name(user = {name, family}: User): String ->
    name + family.
"#;

    let err = parse_module(source).expect_err("reverse alias rejected");
    assert_eq!(
            err.message,
            "migration.function_head_pattern.invalid_alias_style: reverse alias function-head pattern syntax is rejected; use pattern-first aliasing `{pattern} = name: Type`; docs docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style"
        );
}

/// Verifies clause-style function signatures accept typed function-head patterns.
///
/// Inputs:
/// - A public clause-style function declaration whose signature attempts
///   tuple destructuring in the parameter list.
///
/// Output:
/// - Stable parser diagnostic describing the reserved 0.0.7 feature.
///
/// Transformation:
/// - Makes the roadmap rule executable: clause-style functions and
///   single-expression functions share the same future parameter-pattern
///   boundary.
#[test]
fn parses_typed_clause_style_function_head_pattern_parameter() {
    let source = r#"
module function_head_pattern_clause_reserved.

pub full_name({name, family}: User): String.
full_name({name, family}) ->
    name + family.
"#;

    let module = parse_module(source).expect("typed clause function head pattern");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params.len(), 1);
    assert_eq!(function.params[0].name, "_Arg1");
    assert_eq!(function.params[0].annotation.text, "User");
}

/// Verifies clause-style function signatures accept pattern-first aliases.
///
/// Inputs:
/// - A public clause-style function declaration whose signature attempts
///   `{pattern} = alias: Type`.
///
/// Output:
/// - Stable parser diagnostic describing the reserved 0.0.7 feature.
///
/// Transformation:
/// - Prevents clause-style aliases from falling through to an unrelated
///   parameter parse error while preserving the planned spelling.
#[test]
fn parses_clause_style_function_head_pattern_alias_parameter() {
    let source = r#"
module function_head_pattern_clause_alias_reserved.

pub full_name({name, family} = user: User): String.
full_name({name, family}) ->
    user.id.to_string() + name + family.
"#;

    let module = parse_module(source).expect("typed clause pattern alias parameter");
    let Decl::Function(function) = &module.declarations[0] else {
        panic!("expected function declaration");
    };
    assert_eq!(function.params.len(), 1);
    assert_eq!(function.params[0].name, "user");
    assert_eq!(function.params[0].annotation.text, "User");
}

/// Verifies clause-style reverse alias syntax remains rejected in favor of
/// the planned pattern-first alias spelling.
///
/// Inputs:
/// - A public clause-style function declaration whose signature attempts
///   `alias = {pattern}: Type`.
///
/// Output:
/// - Stable parser diagnostic pointing at the future pattern-first shape.
///
/// Transformation:
/// - Keeps rejected reverse aliasing consistent across expression-bodied
///   and clause-style function declarations.
#[test]
fn rejects_reverse_alias_clause_style_function_head_pattern_parameter() {
    let source = r#"
module function_head_pattern_clause_reverse_alias.

pub full_name(user = {name, family}: User): String.
full_name({name, family}) ->
    name + family.
"#;

    let err = parse_module(source).expect_err("clause reverse alias rejected");
    assert_eq!(
            err.message,
            "migration.function_head_pattern.invalid_alias_style: reverse alias function-head pattern syntax is rejected; use pattern-first aliasing `{pattern} = name: Type`; docs docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style"
        );
}

/// Verifies canonical constraint lists on non-function callable forms.
///
/// Inputs:
/// - A module containing a trait method, receiver method, and explicit impl
///   method with post-parameter constraint lists.
///
/// Output:
/// - Parsed declarations whose `generic_bounds` preserve each constraint
///   as type-reference text.
///
/// Transformation:
/// - Exercises all callable parser paths that share the canonical
///   `[TraitRef]` constraint-list syntax.

/// Verifies canonical constraint lists on non-function callable forms.
///
/// Inputs:
/// - A module containing a trait method, receiver method, and explicit impl
///   method with post-parameter constraint lists.
///
/// Output:
/// - Parsed declarations whose `generic_bounds` preserve each constraint
///   as type-reference text.
///
/// Transformation:
/// - Exercises all callable parser paths that share the canonical
///   `[TraitRef]` constraint-list syntax.
#[test]
fn parses_method_trait_method_and_impl_method_constraint_lists() {
    let source = r#"
module bounds_surfaces.

pub struct User {
    name: String
}.

pub trait Show[T] {
    show[A](value: A)[Eq[A]]: String.
}.

pub (user: User) label[A](value: A)[Show[A]]: String ->
    Show.show(value).

pub impl Show[User] for User {
    show[A](value: A)[Eq[A]]: String ->
        "user".
}.
"#;

    let module = parse_module(source).expect("parse constraint-list surfaces");

    let trait_decl = match &module.declarations[1] {
        Decl::Trait(trait_decl) => trait_decl,
        _ => panic!("expected trait declaration"),
    };
    assert_eq!(
        trait_decl.methods[0].generic_bounds,
        vec!["Eq[A]".to_string()]
    );

    let method_decl = match &module.declarations[2] {
        Decl::Method(method_decl) => method_decl,
        _ => panic!("expected method declaration"),
    };
    assert_eq!(method_decl.generic_bounds, vec!["Show[A]".to_string()]);

    let impl_decl = match &module.declarations[3] {
        Decl::TraitImpl(impl_decl) => impl_decl,
        _ => panic!("expected trait impl declaration"),
    };
    assert_eq!(
        impl_decl.methods[0].generic_bounds,
        vec!["Eq[A]".to_string()]
    );
}

#[test]
fn parses_module_and_item_doc_comments() {
    let source = r#"
//! Math helpers.
//! Second module line.

module mathx.

/// Adds one.
/// Second function line.
pub add(X: Int): Int ->
    X + 1.

/// Optional value.
pub type Option[T] =
      none
    | {some, T}.
"#;

    let module = parse_module(source).expect("parse docs");
    assert_eq!(module.docs, vec!["Math helpers.", "Second module line."]);
    match &module.declarations[0] {
        Decl::Function(function) => {
            assert_eq!(function.docs, vec!["Adds one.", "Second function line."]);
        }
        _ => panic!("expected documented function"),
    }
    match &module.declarations[1] {
        Decl::Type(type_decl) => {
            assert_eq!(type_decl.docs, vec!["Optional value."]);
        }
        _ => panic!("expected documented type"),
    }
}

#[test]
fn parses_module_and_item_doc_block_comments() {
    let source = r#"
/**
 * Math helpers.
 *
 * @module mathx
 */
module mathx.

/**
 * Adds one.
 *
 * @param x The value to increment.
 * @returns The incremented value.
 */
@test
pub add(x: Int): Int ->
    x + 1.

/**
 * Optional value.
 *
 * @type T The wrapped value type.
 */
pub type Option[T] =
      none
    | {some, T}.
"#;

    let module = parse_module(source).expect("parse block docs");
    assert_eq!(module.docs, vec!["Math helpers.\n\n@module mathx"]);
    assert_eq!(module.declaration_annotations[0][0].path, vec!["test"]);
    match &module.declarations[0] {
        Decl::Function(function) => {
            assert_eq!(
                    function.docs,
                    vec![
                        "Adds one.\n\n@param x The value to increment.\n@returns The incremented value."
                    ]
                );
        }
        _ => panic!("expected documented function"),
    }
    match &module.declarations[1] {
        Decl::Type(type_decl) => {
            assert_eq!(
                type_decl.docs,
                vec!["Optional value.\n\n@type T The wrapped value type."]
            );
        }
        _ => panic!("expected documented type"),
    }
}

#[test]
fn parses_public_constructor_with_varargs_and_defaults() {
    let source = r#"
module queue.

/// Builds queues.
pub constructor Queue[T] {
    (): Queue[T] ->
        empty();

    (Items: List[T]): Queue[T] ->
        from_list(Items);

    (...Items: T): Queue[T] ->
        from_list(Items)
}.

pub constructor Range {
    (Start: Int, End: Int, Step: Int = 1): Range ->
        make(Start, End, Step)
}.
"#;

    let module = parse_module(source).expect("parse constructors");
    match &module.declarations[0] {
        Decl::Constructor(constructor) => {
            assert!(constructor.is_public);
            assert_eq!(constructor.docs, vec!["Builds queues."]);
            assert_eq!(constructor.name, "Queue");
            assert_eq!(constructor.params, vec!["T"]);
            assert_eq!(constructor.clauses.len(), 3);
            assert!(constructor.clauses[2].params[0].is_varargs);
        }
        _ => panic!("expected queue constructor"),
    }
    match &module.declarations[1] {
        Decl::Constructor(constructor) => {
            let step = &constructor.clauses[0].params[2];
            assert_eq!(step.name, "Step");
            assert!(step.default.is_some());
        }
        _ => panic!("expected range constructor"),
    }
}

#[test]
fn rejects_constructor_varargs_before_other_params() {
    let source = r#"
module bad.

pub constructor Queue[T] {
    (...Items: T, Last: T): Queue[T] ->
        from_list(Items)
}.
"#;

    let err = parse_module(source).expect_err("invalid varargs");
    assert_eq!(err.message, "constructor varargs parameter must be last");
}
