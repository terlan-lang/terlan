use crate::terlan_hir::{ModuleInterface, ParamSignature};

impl ModuleInterface {
    /// Renders this module interface as a full Terlan interface summary.
    ///
    /// Inputs:
    /// - `self`: resolved module interface metadata.
    ///
    /// Output:
    /// - Source-like `.terli` text containing docs and public interface items.
    ///
    /// Transformation:
    /// - Delegates to the shared renderer with documentation emission enabled.
    pub fn to_terlan_interface_text(&self) -> String {
        self.render_terlan_interface_text(true)
    }

    /// Renders this module interface as type-checking summary text.
    ///
    /// Inputs:
    /// - `self`: resolved module interface metadata.
    ///
    /// Output:
    /// - Source-like `.typi` text without documentation payloads.
    ///
    /// Transformation:
    /// - Delegates to the shared renderer with documentation emission disabled.
    pub fn to_terlan_interface_type_text(&self) -> String {
        self.render_terlan_interface_text(false)
    }

    /// Renders public documentation carried by this module interface.
    ///
    /// Inputs:
    /// - `self`: resolved module interface metadata.
    ///
    /// Output:
    /// - Documentation-only text grouped by module, public type, and public
    ///   function.
    ///
    /// Transformation:
    /// - Emits module docs first, then sorted public type docs, then sorted
    ///   public function docs so generated documentation remains stable.
    pub fn to_terlan_interface_doc_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("module {}\n", self.module));
        push_doc_lines(&mut out, "!", &self.docs);

        let mut public_types: Vec<_> = self.public_types.iter().cloned().collect();
        public_types.sort();
        for ty in &public_types {
            push_doc_lines(
                &mut out,
                "/",
                self.type_docs.get(ty).map(Vec::as_slice).unwrap_or(&[]),
            );
        }

        let mut public_functions: Vec<_> = self
            .function_overloads
            .iter()
            .flat_map(|(key, signatures)| {
                signatures
                    .iter()
                    .filter(|signature| signature.public)
                    .map(move |signature| (key, signature))
            })
            .collect();
        public_functions.sort_by(|(lhs, _), (rhs, _)| match lhs.0.cmp(&rhs.0) {
            std::cmp::Ordering::Equal => lhs.1.cmp(&rhs.1),
            ord => ord,
        });

        for (_key, function) in public_functions {
            push_doc_lines(&mut out, "/", &function.docs);
        }

        out
    }

    /// Renders the shared source-like interface representation.
    ///
    /// Inputs:
    /// - `self`: resolved module interface metadata.
    /// - `include_docs`: whether module, type, function, trait, and constructor
    ///   documentation should be emitted.
    ///
    /// Output:
    /// - Source-like interface text for either full `.terli` or lean `.typi`
    ///   summaries.
    ///
    /// Transformation:
    /// - Sorts public interface payloads for deterministic output, normalizes
    ///   type text, preserves receiver/mutability information, and serializes
    ///   constructors as grouped declaration blocks.
    fn render_terlan_interface_text(&self, include_docs: bool) -> String {
        let mut out = String::new();
        if include_docs {
            push_doc_lines(&mut out, "!", &self.docs);
        }
        out.push_str(&format!("module {}.\n\n", self.module));

        let mut public_types: Vec<_> = self.public_types.iter().cloned().collect();
        public_types.sort();
        for ty in &public_types {
            if include_docs {
                push_doc_lines(
                    &mut out,
                    "/",
                    self.type_docs.get(ty).map(Vec::as_slice).unwrap_or(&[]),
                );
            }
            if self.opaque_types.contains(ty) {
                out.push_str(&format!(
                    "pub opaque type {}{}.\n\n",
                    ty,
                    render_type_params(self.type_params.get(ty))
                ));
            } else if let Some(fields) = self.struct_fields.get(ty) {
                out.push_str(&format!("pub struct {} {{\n", ty));
                for (index, field) in fields.iter().enumerate() {
                    let suffix = if index + 1 == fields.len() { "" } else { "," };
                    let privacy = if field.is_private { "#" } else { "" };
                    out.push_str(&format!(
                        "    {}{}: {}{}\n",
                        privacy,
                        field.name,
                        normalize_type_text(&field.annotation),
                        suffix
                    ));
                }
                out.push_str("}.\n\n");
            } else {
                let params = render_type_params(self.type_params.get(ty));
                if let Some(body) = self.type_bodies.get(ty) {
                    out.push_str(&format!(
                        "pub type {}{} =\n    {}.\n\n",
                        ty,
                        params,
                        body.iter()
                            .map(|variant| normalize_type_text(variant))
                            .collect::<Vec<_>>()
                            .join("\n  | ")
                    ));
                } else {
                    out.push_str(&format!("pub type {}{}.\n\n", ty, params));
                }
            }
        }

        let mut public_functions: Vec<_> = self
            .function_overloads
            .iter()
            .flat_map(|(key, signatures)| {
                signatures
                    .iter()
                    .filter(|signature| signature.public)
                    .map(move |signature| (key, signature))
            })
            .collect();
        public_functions.sort_by(|(lhs, _), (rhs, _)| match lhs.0.cmp(&rhs.0) {
            std::cmp::Ordering::Equal => lhs.1.cmp(&rhs.1),
            ord => ord,
        });

        for (_key, function) in public_functions {
            if include_docs {
                push_doc_lines(&mut out, "/", &function.docs);
            }
            if function.receiver_method && !function.params.is_empty() {
                let receiver = &function.params[0];
                let receiver_mut = if function.receiver_mutable {
                    "mut "
                } else {
                    ""
                };
                let params = function
                    .params
                    .iter()
                    .skip(1)
                    .map(render_param_signature)
                    .collect::<Vec<_>>();
                out.push_str(&format!(
                    "pub ({}{}: {}) {}{}{}({}): {}.\n\n",
                    receiver_mut,
                    receiver.name,
                    normalize_type_text(&receiver.annotation),
                    function.name,
                    render_type_params(Some(&function.generic_params)),
                    render_generic_bounds(&function.generic_bounds),
                    params.join(", "),
                    normalize_type_text(&function.return_type)
                ));
                continue;
            }
            let params = function
                .params
                .iter()
                .map(render_param_signature)
                .collect::<Vec<_>>();
            out.push_str(&format!(
                "pub {}{}{}({}): {}.\n\n",
                function.name,
                render_type_params(Some(&function.generic_params)),
                render_generic_bounds(&function.generic_bounds),
                params.join(", "),
                normalize_type_text(&function.return_type)
            ));
        }

        let mut public_traits: Vec<_> = self.traits.values().collect();
        public_traits.sort_by(|left, right| left.name.cmp(&right.name));

        for trait_signature in public_traits {
            if include_docs {
                push_doc_lines(&mut out, "/", &trait_signature.docs);
            }

            let mut methods: Vec<_> = trait_signature.methods.iter().collect();
            methods.sort_by(|(left, _), (right, _)| left.cmp(right));

            let params = render_type_params(Some(&trait_signature.type_params));
            out.push_str(&format!("pub trait {}{}", trait_signature.name, params));
            if !trait_signature.super_traits.is_empty() {
                out.push_str(&format!(
                    " extends {}",
                    trait_signature.super_traits.join(", ")
                ));
            }
            out.push_str(" {\n");

            for (method_name, method) in methods {
                if include_docs {
                    push_doc_lines(&mut out, "/", &method.docs);
                }
                let params = method
                    .params
                    .iter()
                    .map(render_param_signature)
                    .collect::<Vec<_>>();
                out.push_str(&format!(
                    "    {}{}{}",
                    method_name,
                    render_type_params(Some(&method.generic_params)),
                    render_generic_bounds(&method.generic_bounds)
                ));
                out.push_str(&format!(
                    "({}): {}",
                    params.join(", "),
                    normalize_type_text(&method.return_type)
                ));
                if method.has_default {
                    out.push_str(" ->\n        terlan_interface_default");
                }
                out.push_str(".\n");
            }

            out.push_str("}.\n\n");
        }

        let mut public_conformances: Vec<_> = self
            .trait_conformances
            .iter()
            .filter(|conformance| conformance.public)
            .collect();
        public_conformances.sort();

        for conformance in public_conformances {
            out.push_str(&format!(
                "pub impl {} for {} {{\n}}.\n\n",
                normalize_type_text(&conformance.trait_ref),
                normalize_type_text(&conformance.for_type)
            ));
        }

        let mut public_constructors: Vec<_> = self
            .constructors
            .values()
            .flatten()
            .filter(|signature| signature.public)
            .collect();
        public_constructors.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.params.len().cmp(&right.params.len()))
                .then(left.varargs.cmp(&right.varargs))
        });

        let mut current_constructor = String::new();
        for (idx, constructor) in public_constructors.iter().enumerate() {
            if current_constructor != constructor.name {
                if !current_constructor.is_empty() {
                    out.push_str("}.\n\n");
                }
                current_constructor = constructor.name.clone();
                if include_docs {
                    push_doc_lines(&mut out, "/", &constructor.docs);
                }
                out.push_str(&format!(
                    "pub constructor {}{} {{\n",
                    constructor.name,
                    render_type_params(Some(&constructor.type_params))
                ));
            }

            let mut params = constructor
                .params
                .iter()
                .map(render_param_signature)
                .collect::<Vec<_>>();
            if let Some(vararg) = &constructor.vararg {
                params.push(format!(
                    "...{}: {}",
                    vararg.name,
                    normalize_type_text(&vararg.annotation)
                ));
            }

            out.push_str(&format!(
                "    ({}): {} ->\n        {}",
                params.join(", "),
                normalize_type_text(&constructor.return_type),
                normalize_expr_text(&constructor.body)
            ));

            let next_is_same_constructor = public_constructors
                .get(idx + 1)
                .is_some_and(|next| next.name == constructor.name);
            if next_is_same_constructor {
                out.push_str(";\n\n");
            } else {
                out.push('\n');
            }
        }
        if !current_constructor.is_empty() {
            out.push_str("}.\n\n");
        }

        let mut private_types: Vec<_> = self.private_types.iter().cloned().collect();
        private_types.sort();
        if !private_types.is_empty() {
            out.push_str("export type ");
            out.push_str(&private_types.join(", "));
            out.push_str(".\n\n");
        }

        normalize_final_newline(&mut out);
        out
    }
}

/// Normalizes generated interface EOF whitespace.
///
/// Inputs:
/// - `out`: rendered interface text.
///
/// Output:
/// - `out` ends with exactly one newline.
///
/// Transformation:
/// - Removes declaration-separator blank lines at EOF while preserving the
///   conventional final newline expected by text tools and release checks.
fn normalize_final_newline(out: &mut String) {
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Renders one public interface parameter.
///
/// Inputs:
/// - `param`: HIR parameter signature carrying name, type annotation, and
///   source mutability.
///
/// Output:
/// - A source-like parameter fragment such as `value: Int` or
///   `mut collection: C`. Defaulted params render as `name: Type = value`.
///
/// Transformation:
/// - Normalizes the type text and preserves `mut` so generated `.typi`
///   summaries do not erase trait/function parameter mutability or default
///   parameter metadata.
fn render_param_signature(param: &ParamSignature) -> String {
    let mut_prefix = if param.is_mutable { "mut " } else { "" };
    let mut rendered = format!(
        "{}{}: {}",
        mut_prefix,
        param.name,
        normalize_type_text(&param.annotation)
    );
    if let Some(default_text) = &param.default_text {
        rendered.push_str(" = ");
        rendered.push_str(&normalize_expr_text(default_text));
    }
    rendered
}

/// Renders generic type parameters for interface declarations.
///
/// Inputs:
/// - `params`: optional generic parameter list.
///
/// Output:
/// - Empty text for missing or empty params, otherwise `[T, U]` style text.
///
/// Transformation:
/// - Keeps type parameters in their resolved order and wraps them in Terlan's
///   type-parameter delimiter.
fn render_type_params(params: Option<&Vec<String>>) -> String {
    match params {
        Some(params) if !params.is_empty() => format!("[{}]", params.join(", ")),
        _ => String::new(),
    }
}

/// Renders callable generic bounds for interface declarations.
///
/// Inputs:
/// - `bounds`: preserved callable constraint texts.
///
/// Output:
/// - Empty text for no bounds, otherwise `<...>` with comma-separated bounds.
///
/// Transformation:
/// - Keeps generated interface summaries source-like so function and method
///   constraints round-trip through the parser.
fn render_generic_bounds(bounds: &[String]) -> String {
    if bounds.is_empty() {
        String::new()
    } else {
        format!("<{}>", bounds.join(", "))
    }
}

/// Appends Terlan doc comments to rendered interface text.
///
/// Inputs:
/// - `out`: rendered interface buffer being built.
/// - `marker`: doc-comment marker suffix, either `!` for module docs or `/`
///   for item docs.
/// - `docs`: normalized documentation blocks collected from source syntax.
///
/// Output:
/// - `out` contains comment-prefixed documentation lines followed by a blank
///   separator when at least one doc block is present.
///
/// Transformation:
/// - Splits multiline documentation blocks into physical lines and prefixes
///   every line with `//!` or `///` so generated `.typi` files remain valid
///   Terlan interface source.
fn push_doc_lines(out: &mut String, marker: &str, docs: &[String]) {
    for block in docs {
        for line in block.lines() {
            if line.is_empty() {
                out.push_str(&format!("//{}\n", marker));
            } else {
                out.push_str(&format!("//{} {}\n", marker, line));
            }
        }
    }
    if !docs.is_empty() {
        out.push('\n');
    }
}

/// Normalizes rendered type text for stable interface summaries.
///
/// Inputs:
/// - `input`: source-derived type or expression text.
///
/// Output:
/// - Stable, compact text with normalized bracket, comma, and whitespace
///   spacing.
///
/// Transformation:
/// - Applies conservative whitespace cleanup without changing identifiers or
///   operators.
pub(crate) fn normalize_type_text(input: &str) -> String {
    input
        .replace(" [", "[")
        .replace("[ ", "[")
        .replace(" ]", "]")
        .replace(" }", "}")
        .replace(" ,", ",")
        .replace(",", ", ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(", ]", "]")
}

/// Normalizes rendered expression text for interface constructor bodies.
///
/// Inputs:
/// - `input`: source-derived expression text.
///
/// Output:
/// - Stable, compact expression text.
///
/// Transformation:
/// - Reuses type-text normalization because interface constructor bodies only
///   need conservative whitespace cleanup at this layer.
fn normalize_expr_text(input: &str) -> String {
    normalize_type_text(input)
}
