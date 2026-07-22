use super::*;

/// Applies inference substitutions to a higher-kinded constructor application.
///
/// Inputs:
/// - `constructor`: type variable id used as an applied type constructor.
/// - `args`: applied type arguments.
/// - `subst`: inference substitution table produced by unification.
///
/// Output:
/// - A concrete named type when the constructor variable has been inferred as
///   a named type constructor.
/// - A still-higher-kinded application when the constructor remains a type
///   variable.
///
/// Transformation:
/// - Mirrors `substitute_type_constructor_application` for inference-time
///   substitutions so `F[A]` and values of type `Option[A]` can unify through
///   ordinary trait dispatch and receiver checking.
pub(super) fn apply_type_constructor_subst(
    constructor: TypeVarId,
    args: &[Type],
    subst: &HashMap<TypeVarId, Type>,
) -> Type {
    let args = args
        .iter()
        .map(|arg| apply_subst(arg, subst))
        .collect::<Vec<_>>();

    match subst.get(&constructor) {
        Some(Type::Named {
            module,
            name,
            args: constructor_args,
        }) => {
            let mut applied_args = constructor_args
                .iter()
                .map(|arg| apply_subst(arg, subst))
                .collect::<Vec<_>>();
            applied_args.extend(args);
            Type::Named {
                module: module.clone(),
                name: name.clone(),
                args: applied_args,
            }
        }
        Some(Type::Var(next_constructor)) => Type::Apply {
            constructor: *next_constructor,
            args,
        },
        _ => Type::Apply { constructor, args },
    }
}
