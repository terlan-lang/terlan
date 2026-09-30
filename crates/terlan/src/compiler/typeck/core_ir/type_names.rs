use super::{CoreTupleTypeElem, CoreType};

impl CoreType {
    /// Visits nominal heads without rewriting field names or atom literals.
    pub(crate) fn visit_names_mut(&mut self, visit: &mut impl FnMut(&mut String)) {
        match self {
            Self::Named(name) => visit(name),
            Self::Apply { constructor, args } => {
                visit(constructor);
                args.iter_mut().for_each(|ty| ty.visit_names_mut(visit));
            }
            Self::List(item) => item.visit_names_mut(visit),
            Self::Tuple(items) => items.iter_mut().for_each(|item| match item {
                CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                    ty.visit_names_mut(visit);
                }
            }),
            Self::Struct { name, fields } => {
                visit(name);
                fields
                    .iter_mut()
                    .for_each(|field| field.ty.visit_names_mut(visit));
            }
            Self::Map(fields) => fields
                .iter_mut()
                .for_each(|field| field.value.visit_names_mut(visit)),
            Self::Arrow {
                params,
                return_type,
            } => {
                params.iter_mut().for_each(|ty| ty.visit_names_mut(visit));
                return_type.visit_names_mut(visit);
            }
            Self::Union(types) => types.iter_mut().for_each(|ty| ty.visit_names_mut(visit)),
            Self::Int
            | Self::Float
            | Self::Number
            | Self::String
            | Self::Binary
            | Self::Atom
            | Self::Bool
            | Self::Term
            | Self::Dynamic
            | Self::Never
            | Self::AtomLiteral(_) => {}
        }
    }
}
