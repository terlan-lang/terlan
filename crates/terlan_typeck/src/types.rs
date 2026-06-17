use terlan_syntax::span::Span;

pub type TypeVarId = usize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Number,
    Binary,
    Atom,
    Bool,
    Term,
    Dynamic,
    Never,

    LiteralAtom(String),
    LiteralInt(i64),

    Var(TypeVarId),
    List(Box<Type>),
    Tuple(Vec<Type>),
    Union(Vec<Type>),
    Map(Vec<MapFieldType>),
    FixedArray {
        size: usize,
        elem: Box<Type>,
    },

    Named {
        module: Option<String>,
        name: String,
        args: Vec<Type>,
    },

    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MapFieldType {
    pub(crate) key: String,
    pub(crate) value: Type,
    pub(crate) required: bool,
}

#[derive(Debug, Clone)]
pub enum DiagSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
    pub severity: DiagSeverity,
}

/// Renders a type into the stable, user-facing diagnostic spelling.
///
/// Inputs:
/// - `ty`: inferred, declared, or normalized Terlan type.
///
/// Output:
/// - A display string suitable for diagnostics, summaries, and release tests.
///
/// Transformation:
/// - Recursively maps structural and named type shapes to canonical source-like
///   text without exposing internal type-variable storage details beyond stable
///   `T{id}` placeholders.
pub fn pretty_type(ty: &Type) -> String {
    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Number => "Number".to_string(),
        Type::Binary => "Binary".to_string(),
        Type::Atom => "Atom".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::Term => "Term".to_string(),
        Type::Dynamic => "Dynamic".to_string(),
        Type::Never => "Never".to_string(),
        Type::LiteralAtom(atom) => atom.to_string(),
        Type::LiteralInt(value) => format!("{}", value),
        Type::Var(id) => format!("T{}", id),
        Type::List(inner) => format!("List[{}]", pretty_type(inner)),
        Type::FixedArray { size, elem } => {
            format!("FixedArray[{}, {}]", size, pretty_type(elem))
        }
        Type::Tuple(items) => format!(
            "({})",
            items.iter().map(pretty_type).collect::<Vec<_>>().join(", ")
        ),
        Type::Map(fields) => format!(
            "#{{{}}}",
            fields
                .iter()
                .map(|field| {
                    let sep = if field.required { ":=" } else { "=>" };
                    format!("{}{}{}", field.key, sep, pretty_type(&field.value))
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Union(items) => items
            .iter()
            .map(pretty_type)
            .collect::<Vec<_>>()
            .join(" | "),
        Type::Named { module, name, args } => {
            let qualified = if let Some(module_name) = module {
                format!("{}.{}", module_name, name)
            } else {
                name.clone()
            };
            if args.is_empty() {
                qualified
            } else {
                format!(
                    "{}[{}]",
                    qualified,
                    args.iter().map(pretty_type).collect::<Vec<_>>().join(", ")
                )
            }
        }
        Type::Function { params, ret } => {
            format!(
                "({}) -> {}",
                params
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join(", "),
                pretty_type(ret)
            )
        }
    }
}
