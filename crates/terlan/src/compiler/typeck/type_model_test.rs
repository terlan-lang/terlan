use std::collections::{HashMap, HashSet};

use super::test_support::check_syntax_output;
use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies multi-segment module type references keep their full module path.
///
/// Inputs:
/// - A type expression with a lowercase package segment, uppercase module
///   segment, and uppercase type name.
///
/// Output:
/// - Parsed and rendered type text preserving `people.Provider` as the
///   module path and `ExternalUser` as the type name.
///
/// Transformation:
/// - Exercises qualified type-name splitting so imported interface
///   conformance metadata can match consumer-side imported value types.
#[test]
fn type_parser_preserves_multi_segment_module_type_references() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let ty = parse_type_expr(
        "people.Provider.ExternalUser",
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )
    .expect("parse qualified type");

    assert_eq!(pretty_type(&ty), "people.Provider.ExternalUser");
}

/// Verifies singleton atom types unquote escaped single-quoted atom payloads.
///
/// Inputs:
/// - A type expression using `Atom['it\\'s-ready']`.
///
/// Output:
/// - Internal literal atom type containing `it's-ready`.
///
/// Transformation:
/// - Exercises the shared single-quoted atom unquote helper from the type
///   parser path, matching the value parser's atom literal handling.
#[test]
fn type_parser_unquotes_escaped_single_quoted_atom_types() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let ty = parse_type_expr(
        "Atom['it\\'s-ready']",
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )
    .expect("parse escaped singleton atom type");

    assert_eq!(ty, Type::LiteralAtom("it's-ready".to_string()));
    assert_eq!(pretty_type(&ty), "it's-ready");
}

/// Verifies singleton atom types decode canonical string-literal escapes.
///
/// Inputs:
/// - A type expression using `Atom["..."]` with quote, backslash, newline,
///   carriage return, and tab escapes.
///
/// Output:
/// - Internal literal atom type containing the decoded payload.
///
/// Transformation:
/// - Exercises the canonical atom type parser path independently from CoreIR
///   lowering and expression parsing.
#[test]
fn type_parser_decodes_canonical_atom_string_literal_escapes() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let ty = parse_type_expr(
        r#"Atom["quote \" slash \\ newline \n carriage \r tab \t"]"#,
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )
    .expect("parse escaped canonical singleton atom type");

    assert_eq!(
        ty,
        Type::LiteralAtom("quote \" slash \\ newline \n carriage \r tab \t".to_string())
    );
}

/// Verifies `_` parses as a type placeholder in higher-kinded slots.
///
/// Inputs:
/// - A bare placeholder type expression.
///
/// Output:
/// - Internal `Type::Placeholder`.
///
/// Transformation:
/// - Exercises the formal HKT slot marker without allocating an inference
///   variable or resolving a named type.
#[test]
fn type_parser_recognizes_higher_kind_placeholder() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let ty = parse_type_expr("_", &HashSet::new(), &mut vars, &mut next_var)
        .expect("parse placeholder type");

    assert_eq!(ty, Type::Placeholder);
    assert_eq!(pretty_type(&ty), "_");
}

/// Verifies applying an in-scope type-constructor variable creates `Type::Apply`.
///
/// Inputs:
/// - A unary type-constructor variable `F` and application `F[T]`.
///
/// Output:
/// - Internal `Type::Apply` with `F` as the constructor and `T` as the
///   argument variable.
///
/// Transformation:
/// - Distinguishes higher-kinded type-variable application from ordinary named
///   type application such as `Option[T]`.
#[test]
fn type_parser_recognizes_higher_kind_type_application() {
    let mut vars = HashMap::from([("F".to_string(), 0usize), ("T".to_string(), 1usize)]);
    let mut next_var = 2usize;
    let ty = parse_type_expr("F[T]", &HashSet::new(), &mut vars, &mut next_var)
        .expect("parse higher-kinded type application");

    assert_eq!(
        ty,
        Type::Apply {
            constructor: 0,
            args: vec![Type::Var(1)],
        }
    );
    assert_eq!(pretty_type(&ty), "T0[T1]");
}

/// Verifies ordinary named generic applications stay named.
///
/// Inputs:
/// - A visible concrete alias name `Option` and generic argument `T`.
///
/// Output:
/// - Internal `Type::Named`, not `Type::Apply`.
///
/// Transformation:
/// - Protects the split between concrete generic types and HKT constructor
///   variables.
#[test]
fn type_parser_keeps_named_generic_application_concrete() {
    let mut vars = HashMap::from([("T".to_string(), 0usize)]);
    let mut next_var = 1usize;
    let aliases = HashSet::from(["Option".to_string()]);
    let ty = parse_type_expr("Option[T]", &aliases, &mut vars, &mut next_var)
        .expect("parse named generic application");

    assert_eq!(
        ty,
        Type::Named {
            module: None,
            name: "Option".to_string(),
            args: vec![Type::Var(0)],
        }
    );
    assert_eq!(pretty_type(&ty), "Option[T0]");
}

/// Verifies explicit substitution rewrites higher-kinded applications.
///
/// Inputs:
/// - Internal type `F[A]`.
/// - Explicit substitution `F = Option`.
///
/// Output:
/// - Concrete named type `Option[A]`.
///
/// Transformation:
/// - Exercises trait-specialization substitution for higher-kinded
///   constructor variables.
#[test]
fn substitute_type_vars_applies_higher_kind_constructor_mapping() {
    let ty = Type::Apply {
        constructor: 0,
        args: vec![Type::Var(1)],
    };
    let mapping = HashMap::from([(
        0usize,
        Type::Named {
            module: None,
            name: "Option".to_string(),
            args: Vec::new(),
        },
    )]);

    assert_eq!(
        substitute_type_vars(&ty, &mapping),
        Type::Named {
            module: None,
            name: "Option".to_string(),
            args: vec![Type::Var(1)],
        }
    );
}

/// Verifies inference substitution rewrites higher-kinded applications.
///
/// Inputs:
/// - Internal type `F[A]`.
/// - Inference substitution `F = Result[E]`.
///
/// Output:
/// - Concrete named type `Result[E, A]`.
///
/// Transformation:
/// - Exercises unification-time substitution for partially applied concrete
///   constructors with existing constructor arguments.
#[test]
fn apply_subst_applies_higher_kind_constructor_mapping() {
    let ty = Type::Apply {
        constructor: 0,
        args: vec![Type::Var(2)],
    };
    let subst = HashMap::from([(
        0usize,
        Type::Named {
            module: None,
            name: "Result".to_string(),
            args: vec![Type::Var(1)],
        },
    )]);

    assert_eq!(
        apply_subst(&ty, &subst),
        Type::Named {
            module: None,
            name: "Result".to_string(),
            args: vec![Type::Var(1), Type::Var(2)],
        }
    );
}

/// Verifies existential package types parse into scoped type variables.
///
/// Inputs:
/// - Type text `exists T. Box[T]`.
/// - Visible concrete alias name `Box`.
///
/// Output:
/// - Internal existential type whose body uses the bound variable.
///
/// Transformation:
/// - Allocates the existential binder in a nested type-variable scope and
///   parses the body without treating `Box` as a higher-kinded variable.
#[test]
fn type_parser_recognizes_existential_package_type() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let aliases = HashSet::from(["Box".to_string()]);
    let ty = parse_type_expr("exists T. Box[T]", &aliases, &mut vars, &mut next_var)
        .expect("parse existential type");

    assert_eq!(
        ty,
        Type::Existential {
            params: vec![0],
            body: Box::new(Type::Named {
                module: None,
                name: "Box".to_string(),
                args: vec![Type::Var(0)],
            }),
        }
    );
    assert_eq!(pretty_type(&ty), "exists T0. Box[T0]");
}

/// Verifies substitutions do not rewrite existentially bound variables.
///
/// Inputs:
/// - Existential type `exists T0. T0`.
/// - Outer substitution `T0 = Int`.
///
/// Output:
/// - Unchanged existential type.
///
/// Transformation:
/// - Confirms bound existential variables shadow outer substitution maps.
#[test]
fn substitute_type_vars_preserves_existential_bound_variables() {
    let ty = Type::Existential {
        params: vec![0],
        body: Box::new(Type::Var(0)),
    };
    let mapping = HashMap::from([(0usize, Type::Int)]);

    assert_eq!(substitute_type_vars(&ty, &mapping), ty);
}

/// Verifies existential package comparison allows alpha-renamed binders.
///
/// Inputs:
/// - Two existential package types with different internal binder ids.
///
/// Output:
/// - Alias-aware subtype relation accepts them as equivalent shapes.
///
/// Transformation:
/// - Exercises alpha-equivalence so binder identity remains scoped instead of
///   leaking numeric ids into assignability.
#[test]
fn existential_subtyping_accepts_alpha_equivalent_packages() {
    let lhs = Type::Existential {
        params: vec![0],
        body: Box::new(named_one("Box", Type::Var(0))),
    };
    let rhs = Type::Existential {
        params: vec![7],
        body: Box::new(named_one("Box", Type::Var(7))),
    };

    assert!(is_subtype(&lhs, &rhs));
}

/// Builds an opaque generic alias used only for subtype model tests.
///
/// Inputs:
/// - `name`: alias/type-constructor name.
/// - `variance`: variance for the single generic parameter.
///
/// Output:
/// - `TypeAlias` with one parameter and self-named opaque body.
///
/// Transformation:
/// - Creates enough alias metadata for named-type variance checks without
///   expanding into an unrelated structural body.
fn single_param_opaque_alias(name: &str, variance: Variance) -> TypeAlias {
    TypeAlias {
        params: vec![0],
        param_variance: vec![variance],
        body: Type::Named {
            module: None,
            name: name.to_string(),
            args: vec![Type::Var(0)],
        },
        constructor_param_names: Vec::new(),
        is_opaque: true,
    }
}

/// Builds a local single-argument named type for subtype model tests.
///
/// Inputs:
/// - `name`: named type constructor.
/// - `arg`: generic argument.
///
/// Output:
/// - `Type::Named` with no module qualifier.
///
/// Transformation:
/// - Keeps variance tests compact while still exercising the real named type
///   representation.
fn named_one(name: &str, arg: Type) -> Type {
    Type::Named {
        module: None,
        name: name.to_string(),
        args: vec![arg],
    }
}

/// Verifies covariant generic aliases preserve subtype direction.
///
/// Inputs:
/// - Alias metadata for `Box[+T]`.
/// - Candidate `Box[Int]` and expected `Box[Number]`.
///
/// Output:
/// - Candidate is accepted because `Int <: Number`.
///
/// Transformation:
/// - Exercises declared covariance without expanding the opaque alias body.
#[test]
fn alias_aware_subtyping_accepts_covariant_named_args() {
    let aliases = HashMap::from([(
        "Box".to_string(),
        single_param_opaque_alias("Box", Variance::Covariant),
    )]);

    assert!(is_subtype_with_aliases(
        &named_one("Box", Type::Int),
        &named_one("Box", Type::Number),
        &aliases
    ));
    assert!(!is_subtype_with_aliases(
        &named_one("Box", Type::Number),
        &named_one("Box", Type::Int),
        &aliases
    ));
}

/// Verifies contravariant generic aliases reverse subtype direction.
///
/// Inputs:
/// - Alias metadata for `Consumer[-T]`.
/// - Candidate `Consumer[Number]` and expected `Consumer[Int]`.
///
/// Output:
/// - Candidate is accepted because a `Number` consumer can consume `Int`.
///
/// Transformation:
/// - Exercises declared contravariance in named generic assignability.
#[test]
fn alias_aware_subtyping_accepts_contravariant_named_args() {
    let aliases = HashMap::from([(
        "Consumer".to_string(),
        single_param_opaque_alias("Consumer", Variance::Contravariant),
    )]);

    assert!(is_subtype_with_aliases(
        &named_one("Consumer", Type::Number),
        &named_one("Consumer", Type::Int),
        &aliases
    ));
    assert!(!is_subtype_with_aliases(
        &named_one("Consumer", Type::Int),
        &named_one("Consumer", Type::Number),
        &aliases
    ));
}

/// Verifies invariant generic aliases reject one-way subtype widening.
///
/// Inputs:
/// - Alias metadata for `Cell[T]`.
/// - Candidate `Cell[Int]` and expected `Cell[Number]`.
///
/// Output:
/// - Candidate is rejected because invariant parameters require equivalence.
///
/// Transformation:
/// - Preserves the default conservative assignability rule for mutable or
///   otherwise unmarked generic containers.
#[test]
fn alias_aware_subtyping_rejects_invariant_named_arg_widening() {
    let aliases = HashMap::from([(
        "Cell".to_string(),
        single_param_opaque_alias("Cell", Variance::Invariant),
    )]);

    assert!(!is_subtype_with_aliases(
        &named_one("Cell", Type::Int),
        &named_one("Cell", Type::Number),
        &aliases
    ));
    assert!(is_subtype_with_aliases(
        &named_one("Cell", Type::Int),
        &named_one("Cell", Type::Int),
        &aliases
    ));
}

/// Verifies phantom state parameters can encode valid transitions.
///
/// Inputs:
/// - Opaque `Form[State]` handles indexed by `Draft` and `Validated` atom
///   states.
/// - A validator function parameter converting `Form[Draft]` into
///   `Form[Validated]`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Typechecks the transition through a function-value parameter, proving
///   phantom state markers participate in ordinary function application and
///   can gate downstream consumers.
#[test]
fn syntax_output_accepts_phantom_state_transition() {
    let diagnostics = check_syntax_output(
        "\
module phantom_state_ok.\n\
pub type Draft = Atom[\"draft\"].\n\
pub type Validated = Atom[\"validated\"].\n\
pub opaque type Form[State].\n\
\n\
pub submit(form: Form[Validated]): Unit ->\n\
    Unit.\n\
\n\
pub demo(form: Form[Draft], validate: (Form[Draft]) -> Form[Validated]): Unit ->\n\
    submit(validate(form)).\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies phantom state parameters reject invalid transitions.
///
/// Inputs:
/// - Opaque `Form[Draft]` passed directly to a consumer requiring
///   `Form[Validated]`.
///
/// Output:
/// - A type mismatch diagnostic.
///
/// Transformation:
/// - Confirms phantom state parameters are not erased by the typechecker, so a
///   handle in one state cannot satisfy a function requiring another state.
#[test]
fn syntax_output_rejects_wrong_phantom_state_transition() {
    let diagnostics = check_syntax_output(
        "\
module phantom_state_bad.\n\
pub type Draft = Atom[\"draft\"].\n\
pub type Validated = Atom[\"validated\"].\n\
pub opaque type Form[State].\n\
\n\
pub submit(form: Form[Validated]): Unit ->\n\
    Unit.\n\
\n\
pub demo(form: Form[Draft]): Unit ->\n\
    submit(form).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("expected Validated found draft")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies opaque capability handles can encode open-resource access.
///
/// Inputs:
/// - Opaque `Handle[State]` indexed by `Open` and `Closed` capability states.
/// - A function that reads only `Handle[Open]`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Treats runtime resources as opaque values whose permitted operations are
///   carried by phantom type state instead of by structural fields.
#[test]
fn syntax_output_accepts_open_capability_handle_use() {
    let diagnostics = check_syntax_output(
        "\
module capability_handle_ok.\n\
pub type Open = Atom[\"open\"].\n\
pub type Closed = Atom[\"closed\"].\n\
pub opaque type Handle[State].\n\
\n\
pub read(handle: Handle[Open]): String ->\n\
    \"ok\".\n\
\n\
pub demo(handle: Handle[Open]): String ->\n\
    read(handle).\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies opaque capability handles reject closed-resource access.
///
/// Inputs:
/// - Opaque `Handle[Closed]` passed to `read`, which requires `Handle[Open]`.
///
/// Output:
/// - A type mismatch diagnostic.
///
/// Transformation:
/// - Confirms native/runtime resource capabilities can be modeled as phantom
///   states that forbid operations after a state transition such as close.
#[test]
fn syntax_output_rejects_closed_capability_handle_read() {
    let diagnostics = check_syntax_output(
        "\
module capability_handle_bad.\n\
pub type Open = Atom[\"open\"].\n\
pub type Closed = Atom[\"closed\"].\n\
pub opaque type Handle[State].\n\
\n\
pub read(handle: Handle[Open]): String ->\n\
    \"ok\".\n\
\n\
pub demo(handle: Handle[Closed]): String ->\n\
    read(handle).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected Open found closed")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies existential aliases can be passed without unpacking their witness.
///
/// Inputs:
/// - Opaque `Box[T]` package constructor.
/// - Existential `AnyBox = exists T. Box[T]`.
/// - A function accepting and forwarding `AnyBox`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Confirms existential package aliases are stable first-class types when no
///   concrete witness type is demanded by the caller.
#[test]
fn syntax_output_accepts_existential_package_alias_forwarding() {
    let diagnostics = check_syntax_output(
        "\
module existential_package_ok.\n\
pub opaque type Box[T].\n\
pub type AnyBox = exists T. Box[T].\n\
\n\
pub accept(value: AnyBox): Unit ->\n\
    Unit.\n\
\n\
pub demo(value: AnyBox): Unit ->\n\
    accept(value).\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

/// Verifies existential packages do not coerce to concrete instantiations.
///
/// Inputs:
/// - Existential `AnyBox = exists T. Box[T]`.
/// - Consumer requiring concrete `Box[Int]`.
///
/// Output:
/// - A type mismatch diagnostic.
///
/// Transformation:
/// - Keeps existential packaging opaque so callers cannot recover a hidden
///   witness type by passing the package to a concrete generic consumer.
#[test]
fn syntax_output_rejects_existential_package_as_concrete_generic() {
    let diagnostics = check_syntax_output(
        "\
module existential_package_bad.\n\
pub opaque type Box[T].\n\
pub type AnyBox = exists T. Box[T].\n\
\n\
pub accept(value: Box[Int]): Unit ->\n\
    Unit.\n\
\n\
pub demo(value: AnyBox): Unit ->\n\
    accept(value).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("exists T0. Box[T0]")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_collects_type_aliases_on_formal_path() {
    let module = parse_module_as_syntax_output(
        r#"
module aliases.

pub type Status = :active | :disabled.
pub type Boxed[T] = List[T].

pub struct User {
    id: Int,
    tags: Boxed[Binary]
}.

pub trait Named[T] {
    name(value: T): Binary.
}.

pub trait Show[T] extends Named[T] {
    show(value: T): Binary.
}.

template Profile from "./profile.terl.html" {
    title: Binary,
    user: User
}.

pub constructor Boxed[T] {
    (items: List[T]): Boxed[T] ->
        items;
    (...items: T): Boxed[T] ->
        items
}.

pub ok(): Status ->
    :active.

pub tag_count(tags: Boxed[Binary]): Int ->
    0.
"#,
    )
    .expect("parse syntax output type alias fixture");

    let aliases = collect_syntax_type_aliases(&module);
    let imported_aliases = HashMap::new();
    let imported_names = HashMap::new();
    let extra_names = collect_syntax_alias_extra_names(&module);
    let alias_names = collect_syntax_type_names(&module);
    let function_signatures = collect_syntax_function_signatures(
        &module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let constructor_signatures = collect_syntax_constructor_signatures(
        &module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let struct_fields = collect_syntax_struct_fields(&module, &alias_names);
    let template_schemes = collect_syntax_template_schemes(&module, &alias_names);
    let resolved = resolve_syntax_module_output(&module).module;
    let trait_signatures = collect_syntax_trait_signatures(&module, &resolved);

    let status = aliases.get("Status").expect("Status alias");
    assert!(matches!(
        &status.body,
        Type::Union(types)
            if types.contains(&Type::LiteralAtom("active".to_string()))
                && types.contains(&Type::LiteralAtom("disabled".to_string()))
    ));
    assert_eq!(aliases.get("Boxed").expect("Boxed alias").params.len(), 1);
    assert!(extra_names.contains("User"));
    let ok_signature = function_signatures
        .get(&("ok".to_string(), 0))
        .and_then(|signatures| signatures.first())
        .expect("ok function signature");
    assert_eq!(
        ok_signature.ret,
        Type::Named {
            module: None,
            name: "Status".to_string(),
            args: Vec::new(),
        }
    );
    let tag_count_signature = function_signatures
        .get(&("tag_count".to_string(), 1))
        .and_then(|signatures| signatures.first())
        .expect("tag_count function signature");
    assert_eq!(
        tag_count_signature.params,
        vec![Type::Named {
            module: None,
            name: "Boxed".to_string(),
            args: vec![Type::Binary],
        }]
    );
    assert_eq!(tag_count_signature.ret, Type::Int);
    let boxed_constructors = constructor_signatures
        .get("Boxed")
        .expect("Boxed constructor signatures");
    assert_eq!(boxed_constructors.len(), 2);
    assert_eq!(
        boxed_constructors[0].fixed_params,
        vec![Type::List(Box::new(Type::Var(0)))]
    );
    assert_eq!(boxed_constructors[0].min_arity, 1);
    assert_eq!(boxed_constructors[0].vararg, None);
    assert_eq!(
        boxed_constructors[0].ret,
        Type::List(Box::new(Type::Var(0)))
    );
    assert_eq!(boxed_constructors[1].fixed_params, Vec::<Type>::new());
    assert_eq!(boxed_constructors[1].min_arity, 0);
    assert_eq!(boxed_constructors[1].vararg, Some(Type::Var(0)));
    assert_eq!(
        boxed_constructors[1].ret,
        Type::List(Box::new(Type::Var(0)))
    );
    assert_eq!(
        struct_fields
            .get("User")
            .and_then(|fields| fields.get("id")),
        Some(&Type::Int)
    );
    assert_eq!(
        struct_fields
            .get("User")
            .and_then(|fields| fields.get("tags")),
        Some(&Type::Named {
            module: None,
            name: "Boxed".to_string(),
            args: vec![Type::Binary],
        })
    );
    assert_eq!(
        template_schemes
            .get("Profile")
            .and_then(|scheme| scheme.props.get("title"))
            .map(|prop| &prop.ty),
        Some(&Type::Binary)
    );
    assert_eq!(
        template_schemes
            .get("Profile")
            .and_then(|scheme| scheme.props.get("user"))
            .map(|prop| &prop.ty),
        Some(&Type::Named {
            module: None,
            name: "User".to_string(),
            args: Vec::new(),
        })
    );
    let show_trait = trait_signatures.get("Show").expect("Show trait signature");
    assert_eq!(show_trait.type_params, vec!["T".to_string()]);
    assert_eq!(show_trait.super_traits, vec!["Named[T]".to_string()]);
    let show_method = show_trait.methods.get("show").expect("show method");
    assert_eq!(
        show_method
            .params
            .iter()
            .map(|param| param.ty.as_str())
            .collect::<Vec<_>>(),
        vec!["T"]
    );
    assert_eq!(show_method.return_type, "Binary");
}
