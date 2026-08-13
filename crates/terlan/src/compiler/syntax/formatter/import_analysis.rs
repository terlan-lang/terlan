use super::declaration_formatting::*;
use super::reference_rewriting::*;
use super::*;
use crate::terlan_syntax::Span;

pub(super) const DEFAULT_MAX_LINE_LENGTH: usize = 100;
pub(super) const BINARY_LAYOUT_MAX_INLINE_LENGTH: usize = 120;

/// Formats canonical Terlan source text.
/// - `source`: raw `.terl` module text.
///
/// Output:
/// - Pretty-printed Terlan source on success.
/// - `ParseError` when the source cannot be parsed as a canonical module.
///
/// Transformation:
/// - Parses the source into the parser's private parse tree and immediately
///   renders it back to canonical source text. The parse tree is not exposed to
///   callers.
pub fn format_source_module(source: &str) -> Result<String, ParseError> {
    parse_module(source).map(|module| format_module(&module))
}

/// Formats canonical Terlan interface text.
/// Inputs:
/// - `source`: raw `.terli` interface summary text.
///
/// Output:
/// - Pretty-printed interface text on success.
/// - `ParseError` when the source cannot be parsed as an interface module.
///
/// Transformation:
/// - Parses interface-only declaration forms such as export summaries into the
///   parser's private parse tree and renders them without exposing that tree.
pub fn format_interface_source_module(source: &str) -> Result<String, ParseError> {
    parse_interface_module(source).map(|module| format_module(&module))
}

/// Formats a headerless `.terls` script while retaining its optional shebang.
pub fn format_script_source(source: &str) -> Result<String, ParseError> {
    let shebang = source
        .starts_with("#!")
        .then(|| source.lines().next().unwrap_or_default().to_string());
    parse_script_for_format(source, "script.Formatter")
        .map(|module| format_script_module(&module, shebang))
}

fn format_script_module(module: &Module, shebang: Option<String>) -> String {
    let mut authored = module.clone();
    for index in (0..authored.declarations.len()).rev() {
        if matches!(
            &authored.declarations[index],
            Decl::Import(import) if import.span == Span::new(0, 0)
        ) {
            authored.declarations.remove(index);
            authored.declaration_annotations.remove(index);
        }
    }
    let promoted_module = promote_repeated_direct_type_imports(&authored);
    let promoted_module = promote_repeated_direct_value_imports(&promoted_module);
    let mut script = collapse_compatible_imports(&promoted_module);
    let entry_index = script
        .declarations
        .iter()
        .position(|declaration| {
            matches!(declaration, Decl::Function(function) if function.name == "main" && function.params.is_empty())
        })
        .expect("script parser always synthesizes main/0");
    let Decl::Function(entry) = script.declarations.remove(entry_index) else {
        unreachable!("script entry index points at a function")
    };
    script.declaration_annotations.remove(entry_index);
    let body = &entry.clauses[0].body;

    let mut out = String::new();
    if let Some(shebang) = shebang {
        out.push_str(&shebang);
        out.push('\n');
    }
    if !script.docs.is_empty() {
        out.push_str(&format_docs(&script.docs, 0));
        out.push('\n');
    }
    let ordered_declarations = ordered_declarations_for_format(&script);
    for (index, (declaration, annotations)) in ordered_declarations.iter().enumerate() {
        if index > 0 {
            let previous = ordered_declarations[index - 1].0;
            if declarations_need_blank_line(previous, declaration) {
                out.push('\n');
            }
        }
        let docs = format_decl_docs(declaration);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        let annotations = format_decl_annotations(annotations);
        if !annotations.is_empty() {
            out.push_str(&annotations);
            out.push('\n');
        }
        out.push_str(&format_decl(declaration));
        out.push('\n');
    }
    if !ordered_declarations.is_empty() {
        out.push('\n');
    }
    out.push_str(&format_script_body(body));
    out.push_str(".\n");
    out
}

/// Renders a script's outer statement sequence without expression grouping.
///
/// The ordinary expression formatter parenthesizes nested sequences because
/// that is required inside declarations. A script body is already the outer
/// execution boundary, so its statements should instead use one canonical
/// line each. Simple immutable bindings use the script-only `name = value`
/// shorthand; refutable patterns retain ordinary `let` syntax.
fn format_script_body(body: &Expr) -> String {
    let mut parts = Vec::new();
    collect_script_statement_parts(body, &mut parts);
    format_statement_parts(parts, 0)
}

fn collect_script_statement_parts(body: &Expr, parts: &mut Vec<String>) {
    match body {
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } if else_clauses.is_empty() => {
            parts.extend(bindings.iter().map(|binding| {
                let prefix = if matches!(&binding.pattern, Pattern::Var(_)) {
                    ""
                } else {
                    "let "
                };
                format_let_binding_assignment(prefix, &binding.pattern, &binding.value, 0)
            }));
            if let Some(body) = body {
                collect_script_statement_parts(body, parts);
            }
        }
        Expr::Sequence(expressions) => {
            for expression in expressions {
                collect_script_statement_parts(expression, parts);
            }
        }
        expression => parts.push(format_expr(expression, 0)),
    }
}

/// Formats a parsed Terlan module or interface parse tree back into source text.
///
/// Inputs:
/// - `module`: parsed parse tree from either the canonical `.terl` source parser or the
///   `.terli` interface parser.
///
/// Output:
/// - Pretty-printed Terlan text with a module header and formatted declarations.
///
/// Transformation:
/// - Renders imports first in canonical alphabetical order, then walks the
///   remaining declarations in source order. Normal `.terl` parsing rejects
///   `Decl::Export`; if export declarations appear here they are interface
///   summaries from `.terli` parsing.
pub(crate) fn format_module(module: &Module) -> String {
    let promoted_module = promote_repeated_direct_type_imports(module);
    let promoted_module = promote_repeated_direct_value_imports(&promoted_module);
    let collapsed_module = collapse_compatible_imports(&promoted_module);
    format_module_inner(&collapsed_module)
}

/// Formats a parsed module after formatter-owned normalization passes.
pub(super) fn format_module_inner(module: &Module) -> String {
    let mut out = String::new();
    if !module.docs.is_empty() {
        out.push_str(&format_docs(&module.docs, 0));
        out.push('\n');
    }
    out.push_str("module ");
    out.push_str(&module.name);
    out.push_str(".\n\n");

    let ordered_declarations = ordered_declarations_for_format(module);
    for (i, (decl, annotations)) in ordered_declarations.iter().enumerate() {
        if i > 0 {
            let previous = ordered_declarations[i - 1].0;
            if declarations_need_blank_line(previous, decl) {
                out.push('\n');
            }
        }
        let docs = format_decl_docs(decl);
        if !docs.is_empty() {
            out.push_str(&docs);
            out.push('\n');
        }
        let annotations = format_decl_annotations(annotations);
        if !annotations.is_empty() {
            out.push_str(&annotations);
            out.push('\n');
        }
        out.push_str(&format_decl(decl));
        out.push('\n');
    }

    out
}

/// Promotes repeated fully-qualified type references into selected type imports.
///
/// Inputs:
/// - `module`: parsed source or interface module.
///
/// Output:
/// - A cloned module with repeated references such as
///   `std.js.Number.JsNumber` shortened to `JsNumber`, plus a synthesized
///   `import type std.js.Number.{JsNumber}.` when needed.
///
/// Transformation:
/// - Counts formatter-visible type expressions, keeps only repeated
///   fully-qualified type names, avoids local/import name conflicts, rewrites
///   matching type text, and appends synthetic imports for the normal import
///   ordering pass to place.
pub(super) fn promote_repeated_direct_type_imports(module: &Module) -> Module {
    let mut counts = BTreeMap::new();
    collect_type_refs_from_module(module, &mut counts);
    let blocked_names = blocked_type_promotion_names(module);
    let existing_type_imports = existing_type_imports(module);
    let mut replacements = BTreeMap::new();
    let mut imports = BTreeSet::new();

    for (qualified, count) in counts {
        if count < 2 {
            continue;
        }
        let Some((module_name, type_name)) = split_qualified_type_ref(&qualified) else {
            continue;
        };
        if blocked_names.contains(&type_name) {
            continue;
        }
        match existing_type_imports.get(&type_name) {
            Some(existing_module) if existing_module == &module_name => {
                replacements.insert(qualified, type_name);
            }
            Some(_) => {}
            None => {
                replacements.insert(qualified, type_name.clone());
                imports.insert((module_name, type_name));
            }
        }
    }

    if replacements.is_empty() {
        return module.clone();
    }

    let mut promoted = module.clone();
    rewrite_type_refs_in_module(&mut promoted, &replacements);
    for (module_name, type_name) in imports {
        promoted.declarations.push(Decl::Import(ImportDecl {
            kind: ImportKind::Module,
            module_name,
            items: vec![ImportItem {
                name: type_name,
                as_alias: None,
                span: promoted.span,
            }],
            is_type: true,
            is_selected: true,
            source_path: None,
            span: promoted.span,
        }));
        promoted.declaration_annotations.push(Vec::new());
    }
    promoted
}

pub(super) fn promote_repeated_direct_value_imports(module: &Module) -> Module {
    let mut counts = BTreeMap::new();
    collect_value_call_refs_from_module(module, &mut counts);
    let blocked_names = blocked_value_promotion_names(module);
    let existing_value_imports = existing_value_imports(module);
    let selected_value_import_modules = selected_value_import_modules(module);
    let mut replacements = BTreeMap::new();
    let mut imports = BTreeSet::new();

    for ((module_name, function_name), count) in counts {
        if blocked_names.contains(&function_name) {
            continue;
        }
        if count < 2 && !selected_value_import_modules.contains(&module_name) {
            continue;
        }
        match existing_value_imports.get(&function_name) {
            Some(existing_module) if existing_module == &module_name => {
                replacements.insert((module_name, function_name.clone()), function_name);
            }
            Some(_) => {}
            None => {
                replacements.insert(
                    (module_name.clone(), function_name.clone()),
                    function_name.clone(),
                );
                imports.insert((module_name, function_name));
            }
        }
    }

    if replacements.is_empty() {
        return module.clone();
    }

    let mut promoted = module.clone();
    rewrite_value_call_refs_in_module(&mut promoted, &replacements);
    for (module_name, function_name) in imports {
        promoted.declarations.push(Decl::Import(ImportDecl {
            kind: ImportKind::Module,
            module_name,
            items: vec![ImportItem {
                name: function_name,
                as_alias: None,
                span: promoted.span,
            }],
            is_type: false,
            is_selected: true,
            source_path: None,
            span: promoted.span,
        }));
        promoted.declaration_annotations.push(Vec::new());
    }

    promoted
}

pub(super) fn collapse_compatible_imports(module: &Module) -> Module {
    let mut collapsed = module.clone();
    let mut declarations = Vec::new();
    let mut annotations = Vec::new();
    let mut import_indexes = BTreeMap::new();

    for (index, decl) in module.declarations.iter().enumerate() {
        let decl_annotations = module
            .declaration_annotations
            .get(index)
            .cloned()
            .unwrap_or_default();
        let Decl::Import(import) = decl else {
            declarations.push(decl.clone());
            annotations.push(decl_annotations);
            continue;
        };
        if !decl_annotations.is_empty() || !is_collapsible_import(import) {
            declarations.push(decl.clone());
            annotations.push(decl_annotations);
            continue;
        }

        let import = normalize_default_selected_import(import);
        let key = (import.is_type, import.module_name.clone());
        if let Some(existing_index) = import_indexes.get(&key).copied() {
            let Decl::Import(existing) = &mut declarations[existing_index] else {
                unreachable!("import index points at non-import declaration");
            };
            existing.is_selected = existing.is_selected || import.is_selected;
            for item in &import.items {
                if !existing
                    .items
                    .iter()
                    .any(|existing_item| import_items_equal(existing_item, item))
                {
                    existing.items.push(item.clone());
                }
            }
            existing.items.sort_by(import_item_sort_key);
            if existing.items.len() > 1 {
                existing.is_selected = true;
            }
        } else {
            let mut merged_import = import;
            merged_import.items.sort_by(import_item_sort_key);
            import_indexes.insert(key, declarations.len());
            declarations.push(Decl::Import(merged_import));
            annotations.push(decl_annotations);
        }
    }

    collapsed.declarations = declarations;
    collapsed.declaration_annotations = annotations;
    collapsed
}

pub(super) fn collect_type_refs_from_module(module: &Module, counts: &mut BTreeMap<String, usize>) {
    for decl in &module.declarations {
        collect_type_refs_from_decl(decl, counts);
    }
}

pub(super) fn collect_value_call_refs_from_module(
    module: &Module,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    for decl in &module.declarations {
        collect_value_call_refs_from_decl(decl, counts);
    }
}

pub(super) fn collect_value_call_refs_from_decl(
    decl: &Decl,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    match decl {
        Decl::Constant(decl) => collect_value_call_refs_from_expr(&decl.value, counts),
        Decl::ConstFunction(decl) => collect_value_call_refs_from_expr(&decl.body, counts),
        Decl::Struct(decl) => {
            for field in &decl.fields {
                if let Some(default) = &field.default {
                    collect_value_call_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &decl.clauses {
                for param in &clause.params {
                    if let Some(default) = &param.default {
                        collect_value_call_refs_from_expr(default, counts);
                    }
                }
                collect_value_call_refs_from_expr(&clause.body, counts);
            }
        }
        Decl::Function(decl) => {
            for param in &decl.params {
                if let Some(default) = &param.default {
                    collect_value_call_refs_from_expr(default, counts);
                }
            }
            for clause in &decl.clauses {
                collect_value_call_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Method(decl) => {
            if let Some(default) = &decl.receiver.default {
                collect_value_call_refs_from_expr(default, counts);
            }
            for param in &decl.params {
                if let Some(default) = &param.default {
                    collect_value_call_refs_from_expr(default, counts);
                }
            }
            for clause in &decl.clauses {
                collect_value_call_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Trait(decl) => {
            for method in &decl.methods {
                for param in &method.params {
                    if let Some(default) = &param.default {
                        collect_value_call_refs_from_expr(default, counts);
                    }
                }
                if let Some(default_body) = &method.default_body {
                    collect_value_call_refs_from_expr(default_body, counts);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            for method in &decl.methods {
                collect_value_call_refs_from_decl(&Decl::Function(method.clone()), counts);
            }
        }
        Decl::Template(decl) => {
            for prop in &decl.props {
                if let Some(default) = &prop.default {
                    collect_value_call_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Type(_)
        | Decl::Import(_)
        | Decl::Export(_)
        | Decl::AnnotationSchema(_)
        | Decl::Shape(_)
        | Decl::Raw(_) => {}
    }
}

pub(super) fn collect_value_call_refs_from_function_clause(
    clause: &crate::terlan_syntax::parse_tree::FunctionClause,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    if let Some(guard) = &clause.guard {
        collect_value_call_refs_from_expr(guard, counts);
    }
    collect_value_call_refs_from_expr(&clause.body, counts);
}

pub(super) fn collect_value_call_refs_from_case_clause(
    clause: &CaseClause,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    if let Some(guard) = &clause.guard {
        collect_value_call_refs_from_expr(guard, counts);
    }
    collect_value_call_refs_from_expr(&clause.body, counts);
}

pub(super) fn collect_type_refs_from_decl(decl: &Decl, counts: &mut BTreeMap<String, usize>) {
    match decl {
        Decl::Constant(decl) => {
            collect_type_refs_from_type_text(&decl.annotation.text, counts);
            collect_type_refs_from_expr(&decl.value, counts);
        }
        Decl::ConstFunction(decl) => {
            for param in &decl.params {
                collect_type_refs_from_param(param, counts);
            }
            collect_type_refs_from_type_text(&decl.return_type.text, counts);
            collect_type_refs_from_expr(&decl.body, counts);
        }
        Decl::Type(decl) => {
            for ty in decl.variants.iter().chain(decl.implements.iter()) {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
        }
        Decl::Struct(decl) => {
            for include in &decl.includes {
                collect_type_refs_from_type_text(include, counts);
            }
            for implement in &decl.implements {
                collect_type_refs_from_type_text(&implement.text, counts);
            }
            for field in &decl.fields {
                collect_type_refs_from_type_text(&field.annotation.text, counts);
                if let Some(default) = &field.default {
                    collect_type_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &decl.clauses {
                collect_type_refs_from_constructor_clause(clause, counts);
            }
        }
        Decl::Function(decl) => {
            for bound in &decl.generic_bounds {
                collect_type_refs_from_type_text(bound, counts);
            }
            for param in &decl.params {
                collect_type_refs_from_param(param, counts);
            }
            collect_type_refs_from_type_text(&decl.return_type.text, counts);
            for clause in &decl.clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Method(decl) => {
            for bound in &decl.generic_bounds {
                collect_type_refs_from_type_text(bound, counts);
            }
            collect_type_refs_from_param(&decl.receiver, counts);
            for param in &decl.params {
                collect_type_refs_from_param(param, counts);
            }
            collect_type_refs_from_type_text(&decl.return_type.text, counts);
            for clause in &decl.clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Decl::Trait(decl) => {
            for super_trait in &decl.super_traits {
                collect_type_refs_from_type_text(super_trait, counts);
            }
            for method in &decl.methods {
                for bound in &method.generic_bounds {
                    collect_type_refs_from_type_text(bound, counts);
                }
                for param in &method.params {
                    collect_type_refs_from_param(param, counts);
                }
                collect_type_refs_from_type_text(&method.return_type.text, counts);
                if let Some(default_body) = &method.default_body {
                    collect_type_refs_from_expr(default_body, counts);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            collect_type_refs_from_type_text(&decl.trait_ref.text, counts);
            collect_type_refs_from_type_text(&decl.for_type.text, counts);
            for method in &decl.methods {
                collect_type_refs_from_decl(&Decl::Function(method.clone()), counts);
            }
        }
        Decl::Template(decl) => {
            for prop in &decl.props {
                collect_type_refs_from_type_text(&prop.annotation.text, counts);
                if let Some(default) = &prop.default {
                    collect_type_refs_from_expr(default, counts);
                }
            }
        }
        Decl::Import(_)
        | Decl::Export(_)
        | Decl::AnnotationSchema(_)
        | Decl::Shape(_)
        | Decl::Raw(_) => {}
    }
}

pub(super) fn collect_type_refs_from_constructor_clause(
    clause: &ConstructorClause,
    counts: &mut BTreeMap<String, usize>,
) {
    for param in &clause.params {
        collect_type_refs_from_type_text(&param.annotation.text, counts);
        if let Some(default) = &param.default {
            collect_type_refs_from_expr(default, counts);
        }
    }
    collect_type_refs_from_type_text(&clause.return_type.text, counts);
    collect_type_refs_from_expr(&clause.body, counts);
}

pub(super) fn collect_type_refs_from_function_clause(
    clause: &crate::terlan_syntax::parse_tree::FunctionClause,
    counts: &mut BTreeMap<String, usize>,
) {
    if let Some(guard) = &clause.guard {
        collect_type_refs_from_expr(guard, counts);
    }
    collect_type_refs_from_expr(&clause.body, counts);
}

pub(super) fn collect_type_refs_from_param(param: &Param, counts: &mut BTreeMap<String, usize>) {
    collect_type_refs_from_type_text(&param.annotation.text, counts);
    if let Some(default) = &param.default {
        collect_type_refs_from_expr(default, counts);
    }
}

pub(super) fn collect_type_refs_from_expr(expr: &Expr, counts: &mut BTreeMap<String, usize>) {
    match expr {
        Expr::Tuple(items)
        | Expr::List(items)
        | Expr::FixedArray(items)
        | Expr::Sequence(items) => {
            for item in items {
                collect_type_refs_from_expr(item, counts);
            }
        }
        Expr::ListCons(left, right)
        | Expr::Index(left, right)
        | Expr::BinaryOp { left, right, .. } => {
            collect_type_refs_from_expr(left, counts);
            collect_type_refs_from_expr(right, counts);
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => {
            collect_type_refs_from_expr(collection, counts);
            collect_type_refs_from_expr(index, counts);
            collect_type_refs_from_expr(value, counts);
        }
        Expr::Map(fields)
        | Expr::RecordUpdate { fields, .. }
        | Expr::RecordConstruct { fields, .. } => {
            for field in fields {
                collect_type_refs_from_expr(&field.value, counts);
            }
            if let Expr::RecordUpdate { value, .. } = expr {
                collect_type_refs_from_expr(value, counts);
            }
        }
        Expr::BinaryLayout { fields, .. } => {
            for field in fields {
                collect_type_refs_from_type_text(&field.descriptor.text, counts);
            }
        }
        Expr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            collect_type_refs_from_expr(expr, counts);
            for generator in generators {
                collect_type_refs_from_expr(&generator.source, counts);
            }
            for guard in guards {
                collect_type_refs_from_expr(guard, counts);
            }
        }
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            for binding in bindings {
                collect_type_refs_from_expr(&binding.value, counts);
            }
            for clause in else_clauses {
                collect_type_refs_from_case_clause(clause, counts);
            }
            if let Some(body) = body {
                collect_type_refs_from_expr(body, counts);
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            ..
        } => {
            collect_type_refs_from_expr(callee, counts);
            for ty in type_args {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
            for arg in args {
                collect_type_refs_from_expr(arg, counts);
            }
        }
        Expr::Case { scrutinee, clauses } => {
            collect_type_refs_from_expr(scrutinee, counts);
            for clause in clauses {
                collect_type_refs_from_case_clause(clause, counts);
            }
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_type_refs_from_expr(body, counts);
            for clause in of_clauses.iter().chain(catch_clauses.iter()) {
                collect_type_refs_from_case_clause(clause, counts);
            }
            if let Some(after) = after_clause {
                collect_type_refs_from_expr(&after.trigger, counts);
                collect_type_refs_from_expr(&after.body, counts);
            }
        }
        Expr::If { clauses } => {
            for clause in clauses {
                collect_type_refs_from_expr(&clause.condition, counts);
                collect_type_refs_from_expr(&clause.body, counts);
            }
        }
        Expr::Fun { clauses } => {
            for clause in clauses {
                collect_type_refs_from_function_clause(clause, counts);
            }
        }
        Expr::MacroCall { args, .. } => {
            for arg in args {
                collect_type_refs_from_expr(arg, counts);
            }
        }
        Expr::RawMacro {
            type_args,
            interpolations,
            ..
        } => {
            for ty in type_args {
                collect_type_refs_from_type_text(&ty.text, counts);
            }
            for interpolation in interpolations {
                collect_type_refs_from_expr(interpolation, counts);
            }
        }
        Expr::HtmlBlock(block) => collect_type_refs_from_html_block(block, counts),
        Expr::RecordAccess { value, .. } | Expr::FieldAccess { value, .. } => {
            collect_type_refs_from_expr(value, counts);
        }
        Expr::ConstructorChain { base, record } => {
            collect_type_refs_from_expr(base, counts);
            collect_type_refs_from_expr(record, counts);
        }
        Expr::UnaryOp { expr, .. } | Expr::Quote(expr) | Expr::Unquote(expr) => {
            collect_type_refs_from_expr(expr, counts);
        }
        Expr::Cast { expr, target_type } => {
            collect_type_refs_from_expr(expr, counts);
            collect_type_refs_from_type_text(&target_type.text, counts);
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_) => {}
    }
}

pub(super) fn collect_type_refs_from_case_clause(
    clause: &CaseClause,
    counts: &mut BTreeMap<String, usize>,
) {
    if let Some(guard) = &clause.guard {
        collect_type_refs_from_expr(guard, counts);
    }
    collect_type_refs_from_expr(&clause.body, counts);
}

pub(super) fn collect_type_refs_from_html_block(
    block: &HtmlBlockExpr,
    counts: &mut BTreeMap<String, usize>,
) {
    for node in &block.nodes {
        collect_type_refs_from_html_node(node, counts);
    }
}

pub(super) fn collect_type_refs_from_html_node(
    node: &HtmlNode,
    counts: &mut BTreeMap<String, usize>,
) {
    match node {
        HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(HtmlAttrValue::Expr(expr)) = &attr.value {
                    collect_type_refs_from_expr(expr, counts);
                }
            }
            for child in &element.children {
                collect_type_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Expr(expr) => collect_type_refs_from_expr(expr, counts),
        HtmlNode::NamedSlot(slot) => {
            for child in &slot.children {
                collect_type_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Text(_) => {}
    }
}
