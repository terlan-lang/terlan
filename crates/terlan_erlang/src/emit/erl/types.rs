//! Erlang type render model.
//!
//! This module owns type-expression rendering for generated Erlang specs.

use std::collections::{BTreeMap, BTreeSet};

/// Erlang type expression render model.
///
/// Inputs:
/// - Lowered type fragments from syntax or CoreIR type lowering.
///
/// Output:
/// - Erlang type syntax through `render`.
///
/// Transformation:
/// - Represents common type shapes structurally while allowing raw fragments
///   for backend-owned spec forms.
#[derive(Debug, Clone)]
pub(in crate::emit) enum ErlType {
    Raw(String),
    Named {
        name: String,
        args: Vec<ErlType>,
    },
    Tuple(Vec<ErlType>),
    List(Box<ErlType>),
    Map(Vec<ErlMapTypeField>),
    Union(Vec<ErlType>),
    Fun {
        args: Vec<ErlType>,
        ret: Box<ErlType>,
    },
}

impl ErlType {
    /// Renders an Erlang type expression.
    ///
    /// Input is an internal type expression. Output is Erlang type syntax. The
    /// transformation recursively renders named, tuple, list, union, and
    /// function types while preserving raw backend fragments.
    pub(in crate::emit) fn render(&self) -> String {
        match self {
            ErlType::Raw(text) => text.clone(),
            ErlType::Named { name, args } if args.is_empty() => format!("{}()", name),
            ErlType::Named { name, args } => {
                let args = args
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args)
            }
            ErlType::Tuple(items) => {
                let items = items
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlType::List(inner) => format!("[{}]", inner.render()),
            ErlType::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(ErlMapTypeField::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlType::Union(items) => items
                .iter()
                .map(ErlType::render)
                .collect::<Vec<_>>()
                .join(" | "),
            ErlType::Fun { args, ret } if args.is_empty() => format!("fun(() -> {})", ret.render()),
            ErlType::Fun { args, ret } => {
                let args = args
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fun(({}) -> {})", args, ret.render())
            }
        }
    }

    /// Normalizes degenerate Erlang type expressions.
    ///
    /// Input is an owned type expression. Output is the same expression with
    /// single-item unions collapsed. The transformation keeps all other type
    /// shapes unchanged.
    pub(in crate::emit) fn normalized(self) -> Self {
        match self {
            ErlType::Union(items) if items.len() == 1 => items.into_iter().next().unwrap(),
            other => other,
        }
    }

    /// Counts generic type variable occurrences.
    ///
    /// Inputs are a type expression and mutable occurrence map. Output is the
    /// mutated map. The transformation walks nested type shapes and increments
    /// counts only for raw names classified as generic type variables.
    pub(in crate::emit) fn collect_type_vars(&self, vars: &mut BTreeMap<String, usize>) {
        match self {
            ErlType::Raw(text) if super::super::util::is_generic_type_var(text) => {
                *vars.entry(text.clone()).or_insert(0) += 1;
            }
            ErlType::Raw(_) => {}
            ErlType::Named { args, .. } | ErlType::Tuple(args) | ErlType::Union(args) => {
                for arg in args {
                    arg.collect_type_vars(vars);
                }
            }
            ErlType::List(inner) => inner.collect_type_vars(vars),
            ErlType::Map(fields) => {
                for field in fields {
                    field.value.collect_type_vars(vars);
                }
            }
            ErlType::Fun { args, ret } => {
                for arg in args {
                    arg.collect_type_vars(vars);
                }
                ret.collect_type_vars(vars);
            }
        }
    }

    /// Renders an Erlang type expression with phantom variables marked.
    ///
    /// Inputs are a type expression and the set of variable names considered
    /// phantom. Output is Erlang type syntax. The transformation recursively
    /// renders the type while prefixing phantom raw variables with `_`.
    pub(in crate::emit) fn render_with_phantom_vars(
        &self,
        phantom_vars: &BTreeSet<String>,
    ) -> String {
        match self {
            ErlType::Raw(text) if phantom_vars.contains(text) => format!("_{}", text),
            ErlType::Raw(text) => text.clone(),
            ErlType::Named { name, args } if args.is_empty() => format!("{}()", name),
            ErlType::Named { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| arg.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args)
            }
            ErlType::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|item| item.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlType::List(inner) => format!("[{}]", inner.render_with_phantom_vars(phantom_vars)),
            ErlType::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(|field| field.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlType::Union(items) => items
                .iter()
                .map(|item| item.render_with_phantom_vars(phantom_vars))
                .collect::<Vec<_>>()
                .join(" | "),
            ErlType::Fun { args, ret } if args.is_empty() => {
                format!("fun(() -> {})", ret.render_with_phantom_vars(phantom_vars))
            }
            ErlType::Fun { args, ret } => {
                let args = args
                    .iter()
                    .map(|arg| arg.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "fun(({}) -> {})",
                    args,
                    ret.render_with_phantom_vars(phantom_vars)
                )
            }
        }
    }
}

/// Erlang map type field render model.
///
/// Inputs:
/// - Key text, value type, and requiredness marker.
///
/// Output:
/// - Map type field syntax.
///
/// Transformation:
/// - Selects Erlang `:=` or `=>` separators from the requiredness flag.
#[derive(Debug, Clone)]
pub(in crate::emit) struct ErlMapTypeField {
    pub(in crate::emit) key: String,
    pub(in crate::emit) value: ErlType,
    pub(in crate::emit) required: bool,
}

impl ErlMapTypeField {
    /// Renders an Erlang map type field.
    ///
    /// Input is a key, lowered value type, and requiredness flag. Output is
    /// Erlang map type syntax. The transformation selects `:=` for required
    /// keys and `=>` for optional/associative keys.
    fn render(&self) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, sep, self.value.render())
    }

    /// Renders an Erlang map type field with phantom type variables marked.
    ///
    /// Inputs are one map type field and the phantom-variable set collected
    /// from the enclosing spec. Output is Erlang map type syntax. The
    /// transformation delegates value rendering to the nested type mapper.
    pub(in crate::emit) fn render_with_phantom_vars(
        &self,
        phantom_vars: &BTreeSet<String>,
    ) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!(
            "{}{}{}",
            self.key,
            sep,
            self.value.render_with_phantom_vars(phantom_vars)
        )
    }
}
