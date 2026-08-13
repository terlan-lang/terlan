use super::declaration_formatting::*;
use super::import_analysis::*;
use super::*;

pub(super) fn collect_value_call_refs_from_expr(
    expr: &Expr,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    match expr {
        Expr::Tuple(items)
        | Expr::List(items)
        | Expr::FixedArray(items)
        | Expr::Sequence(items) => {
            for item in items {
                collect_value_call_refs_from_expr(item, counts);
            }
        }
        Expr::ListCons(left, right)
        | Expr::Index(left, right)
        | Expr::BinaryOp { left, right, .. } => {
            collect_value_call_refs_from_expr(left, counts);
            collect_value_call_refs_from_expr(right, counts);
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => {
            collect_value_call_refs_from_expr(collection, counts);
            collect_value_call_refs_from_expr(index, counts);
            collect_value_call_refs_from_expr(value, counts);
        }
        Expr::Map(fields)
        | Expr::RecordUpdate { fields, .. }
        | Expr::RecordConstruct { fields, .. } => {
            for field in fields {
                collect_value_call_refs_from_expr(&field.value, counts);
            }
            if let Expr::RecordUpdate { value, .. } = expr {
                collect_value_call_refs_from_expr(value, counts);
            }
        }
        Expr::BinaryLayout { .. } => {}
        Expr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            collect_value_call_refs_from_expr(expr, counts);
            for generator in generators {
                collect_value_call_refs_from_expr(&generator.source, counts);
            }
            for guard in guards {
                collect_value_call_refs_from_expr(guard, counts);
            }
        }
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            for binding in bindings {
                collect_value_call_refs_from_expr(&binding.value, counts);
            }
            for clause in else_clauses {
                collect_value_call_refs_from_case_clause(clause, counts);
            }
            if let Some(body) = body {
                collect_value_call_refs_from_expr(body, counts);
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            remote,
            ..
        } => {
            if type_args.is_empty() {
                if let (Some(remote), Some(function_name)) =
                    (remote, value_call_callee_name(callee))
                {
                    if is_value_import_candidate(remote, function_name) {
                        *counts
                            .entry((remote.clone(), function_name.clone()))
                            .or_insert(0) += 1;
                    }
                }
            }
            collect_value_call_refs_from_expr(callee, counts);
            for arg in args {
                collect_value_call_refs_from_expr(arg, counts);
            }
        }
        Expr::Case { scrutinee, clauses } => {
            collect_value_call_refs_from_expr(scrutinee, counts);
            for clause in clauses {
                collect_value_call_refs_from_case_clause(clause, counts);
            }
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_value_call_refs_from_expr(body, counts);
            for clause in of_clauses.iter().chain(catch_clauses.iter()) {
                collect_value_call_refs_from_case_clause(clause, counts);
            }
            if let Some(after) = after_clause {
                collect_value_call_refs_from_expr(&after.trigger, counts);
                collect_value_call_refs_from_expr(&after.body, counts);
            }
        }
        Expr::If { clauses } => {
            for clause in clauses {
                collect_value_call_refs_from_expr(&clause.condition, counts);
                collect_value_call_refs_from_expr(&clause.body, counts);
            }
        }
        Expr::Fun { clauses } => {
            for clause in clauses {
                collect_value_call_refs_from_function_clause(clause, counts);
            }
        }
        Expr::MacroCall { args, .. } => {
            for arg in args {
                collect_value_call_refs_from_expr(arg, counts);
            }
        }
        Expr::RawMacro { interpolations, .. } => {
            for interpolation in interpolations {
                collect_value_call_refs_from_expr(interpolation, counts);
            }
        }
        Expr::HtmlBlock(block) => collect_value_call_refs_from_html_block(block, counts),
        Expr::RecordAccess { value, .. } | Expr::FieldAccess { value, .. } => {
            collect_value_call_refs_from_expr(value, counts);
        }
        Expr::ConstructorChain { base, record } => {
            collect_value_call_refs_from_expr(base, counts);
            collect_value_call_refs_from_expr(record, counts);
        }
        Expr::UnaryOp { expr, .. } | Expr::Quote(expr) | Expr::Unquote(expr) => {
            collect_value_call_refs_from_expr(expr, counts);
        }
        Expr::Cast { expr, .. } => collect_value_call_refs_from_expr(expr, counts),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_) => {}
    }
}

pub(super) fn collect_value_call_refs_from_html_block(
    block: &HtmlBlockExpr,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    for node in &block.nodes {
        collect_value_call_refs_from_html_node(node, counts);
    }
}

pub(super) fn collect_value_call_refs_from_html_node(
    node: &HtmlNode,
    counts: &mut BTreeMap<(String, String), usize>,
) {
    match node {
        HtmlNode::Element(element) => {
            for attr in &element.attrs {
                if let Some(HtmlAttrValue::Expr(expr)) = &attr.value {
                    collect_value_call_refs_from_expr(expr, counts);
                }
            }
            for child in &element.children {
                collect_value_call_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Expr(expr) => collect_value_call_refs_from_expr(expr, counts),
        HtmlNode::NamedSlot(slot) => {
            for child in &slot.children {
                collect_value_call_refs_from_html_node(child, counts);
            }
        }
        HtmlNode::Text(_) => {}
    }
}

pub(super) fn collect_type_refs_from_type_text(text: &str, counts: &mut BTreeMap<String, usize>) {
    for token in type_ref_tokens(text) {
        if is_qualified_type_ref_candidate(token) {
            *counts.entry(token.to_string()).or_insert(0) += 1;
        }
    }
}

pub(super) fn blocked_type_promotion_names(module: &Module) -> BTreeSet<String> {
    let mut blocked = BTreeSet::new();
    for decl in &module.declarations {
        match decl {
            Decl::Type(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Struct(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Trait(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Import(import) if !import.is_type => {
                for item in &import.items {
                    blocked.insert(import_local_name(item));
                }
            }
            _ => {}
        }
    }
    blocked
}

pub(super) fn existing_type_imports(module: &Module) -> BTreeMap<String, String> {
    let mut imports = BTreeMap::new();
    for decl in &module.declarations {
        let Decl::Import(import) = decl else {
            continue;
        };
        if !import.is_type {
            continue;
        }
        for item in &import.items {
            imports.insert(import_local_name(item), import.module_name.clone());
        }
    }
    imports
}

pub(super) fn blocked_value_promotion_names(module: &Module) -> BTreeSet<String> {
    let mut blocked = BTreeSet::new();
    for decl in &module.declarations {
        match decl {
            Decl::Function(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Method(decl) => {
                blocked.insert(decl.name.clone());
            }
            Decl::Import(import) if import.is_type => {
                for item in &import.items {
                    blocked.insert(import_local_name(item));
                }
            }
            _ => {}
        }
    }
    blocked
}

pub(super) fn existing_value_imports(module: &Module) -> BTreeMap<String, String> {
    let mut imports = BTreeMap::new();
    for decl in &module.declarations {
        let Decl::Import(import) = decl else {
            continue;
        };
        if import.is_type {
            continue;
        }
        for item in &import.items {
            imports.insert(import_local_name(item), import.module_name.clone());
        }
    }
    imports
}

pub(super) fn selected_value_import_modules(module: &Module) -> BTreeSet<String> {
    let mut modules = BTreeSet::new();
    for decl in &module.declarations {
        let Decl::Import(import) = decl else {
            continue;
        };
        if import.is_type || !matches!(import.kind, ImportKind::Module) {
            continue;
        }
        if import.is_selected || import.items.len() > 1 {
            modules.insert(import.module_name.clone());
        }
    }
    modules
}

pub(super) fn is_collapsible_import(import: &ImportDecl) -> bool {
    matches!(import.kind, ImportKind::Module)
        && import.source_path.is_none()
        && import.items.iter().all(|item| item.name != "*")
}

pub(super) fn normalize_default_selected_import(import: &ImportDecl) -> ImportDecl {
    if import.items.len() != 1 || import.items[0].as_alias.is_some() {
        return import.clone();
    }
    let item = &import.items[0];
    let Some((parent_module, leaf)) = import.module_name.rsplit_once('.') else {
        return import.clone();
    };
    if leaf != item.name {
        return import.clone();
    }

    let mut normalized = import.clone();
    normalized.module_name = parent_module.to_string();
    normalized.is_selected = false;
    normalized
}

pub(super) fn import_items_equal(left: &ImportItem, right: &ImportItem) -> bool {
    left.name == right.name && left.as_alias == right.as_alias
}

pub(super) fn import_item_sort_key(left: &ImportItem, right: &ImportItem) -> std::cmp::Ordering {
    (&left.name, &left.as_alias).cmp(&(&right.name, &right.as_alias))
}

pub(super) fn is_value_import_candidate(module_name: &str, function_name: &str) -> bool {
    if module_name.is_empty() || function_name.is_empty() {
        return false;
    }
    let mut module_segments = module_name.split('.');
    let Some(root_segment) = module_segments.next() else {
        return false;
    };
    if module_segments.next().is_none() {
        return false;
    }
    if !root_segment
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
    {
        return false;
    }
    if !module_name
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifier_segment(segment))
    {
        return false;
    }
    function_name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
        && is_identifier_segment(function_name)
}

pub(super) fn value_call_callee_name(callee: &Expr) -> Option<&String> {
    match callee {
        Expr::Atom(name) | Expr::Var(name) => Some(name),
        _ => None,
    }
}

pub(super) fn import_local_name(item: &ImportItem) -> String {
    item.as_alias.clone().unwrap_or_else(|| item.name.clone())
}

pub(super) fn split_qualified_type_ref(text: &str) -> Option<(String, String)> {
    if !is_qualified_type_ref_candidate(text) {
        return None;
    }
    let (module_name, type_name) = text.rsplit_once('.')?;
    Some((module_name.to_string(), type_name.to_string()))
}

pub(super) fn is_qualified_type_ref_candidate(text: &str) -> bool {
    let Some((module_name, type_name)) = text.rsplit_once('.') else {
        return false;
    };
    if module_name.is_empty() || type_name.is_empty() {
        return false;
    }
    if !text
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifier_segment(segment))
    {
        return false;
    }
    type_name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && module_name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
}

pub(super) fn is_identifier_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(super) fn type_ref_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if is_type_ref_token_char(ch) {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            tokens.push(&text[token_start..index]);
        }
    }
    if let Some(token_start) = start {
        tokens.push(&text[token_start..]);
    }
    tokens
}

pub(super) fn is_type_ref_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

pub(super) fn rewrite_type_refs_in_module(
    module: &mut Module,
    replacements: &BTreeMap<String, String>,
) {
    for decl in &mut module.declarations {
        rewrite_type_refs_in_decl(decl, replacements);
    }
}

pub(super) fn rewrite_value_call_refs_in_module(
    module: &mut Module,
    replacements: &BTreeMap<(String, String), String>,
) {
    for decl in &mut module.declarations {
        rewrite_value_call_refs_in_decl(decl, replacements);
    }
}

pub(super) fn rewrite_value_call_refs_in_decl(
    decl: &mut Decl,
    replacements: &BTreeMap<(String, String), String>,
) {
    match decl {
        Decl::Constant(decl) => rewrite_value_call_refs_in_expr(&mut decl.value, replacements),
        Decl::ConstFunction(decl) => rewrite_value_call_refs_in_expr(&mut decl.body, replacements),
        Decl::Struct(decl) => {
            for field in &mut decl.fields {
                if let Some(default) = &mut field.default {
                    rewrite_value_call_refs_in_expr(default, replacements);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &mut decl.clauses {
                for param in &mut clause.params {
                    if let Some(default) = &mut param.default {
                        rewrite_value_call_refs_in_expr(default, replacements);
                    }
                }
                rewrite_value_call_refs_in_expr(&mut clause.body, replacements);
            }
        }
        Decl::Function(decl) => {
            for param in &mut decl.params {
                if let Some(default) = &mut param.default {
                    rewrite_value_call_refs_in_expr(default, replacements);
                }
            }
            for clause in &mut decl.clauses {
                rewrite_value_call_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Method(decl) => {
            if let Some(default) = &mut decl.receiver.default {
                rewrite_value_call_refs_in_expr(default, replacements);
            }
            for param in &mut decl.params {
                if let Some(default) = &mut param.default {
                    rewrite_value_call_refs_in_expr(default, replacements);
                }
            }
            for clause in &mut decl.clauses {
                rewrite_value_call_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Trait(decl) => {
            for method in &mut decl.methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        rewrite_value_call_refs_in_expr(default, replacements);
                    }
                }
                if let Some(default_body) = &mut method.default_body {
                    rewrite_value_call_refs_in_expr(default_body, replacements);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            for method in &mut decl.methods {
                for param in &mut method.params {
                    if let Some(default) = &mut param.default {
                        rewrite_value_call_refs_in_expr(default, replacements);
                    }
                }
                for clause in &mut method.clauses {
                    rewrite_value_call_refs_in_function_clause(clause, replacements);
                }
            }
        }
        Decl::Template(decl) => {
            for prop in &mut decl.props {
                if let Some(default) = &mut prop.default {
                    rewrite_value_call_refs_in_expr(default, replacements);
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

pub(super) fn rewrite_value_call_refs_in_function_clause(
    clause: &mut crate::terlan_syntax::parse_tree::FunctionClause,
    replacements: &BTreeMap<(String, String), String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_value_call_refs_in_expr(guard, replacements);
    }
    rewrite_value_call_refs_in_expr(&mut clause.body, replacements);
}

pub(super) fn rewrite_value_call_refs_in_case_clause(
    clause: &mut CaseClause,
    replacements: &BTreeMap<(String, String), String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_value_call_refs_in_expr(guard, replacements);
    }
    rewrite_value_call_refs_in_expr(&mut clause.body, replacements);
}

pub(super) fn rewrite_type_refs_in_decl(decl: &mut Decl, replacements: &BTreeMap<String, String>) {
    match decl {
        Decl::Constant(decl) => {
            rewrite_type_text(&mut decl.annotation.text, replacements);
            rewrite_type_refs_in_expr(&mut decl.value, replacements);
        }
        Decl::ConstFunction(decl) => {
            for param in &mut decl.params {
                rewrite_type_refs_in_param(param, replacements);
            }
            rewrite_type_text(&mut decl.return_type.text, replacements);
            rewrite_type_refs_in_expr(&mut decl.body, replacements);
        }
        Decl::Type(decl) => {
            for ty in decl.variants.iter_mut().chain(decl.implements.iter_mut()) {
                rewrite_type_text(&mut ty.text, replacements);
            }
        }
        Decl::Struct(decl) => {
            for include in &mut decl.includes {
                rewrite_type_text(include, replacements);
            }
            for implement in &mut decl.implements {
                rewrite_type_text(&mut implement.text, replacements);
            }
            for field in &mut decl.fields {
                rewrite_type_text(&mut field.annotation.text, replacements);
                if let Some(default) = &mut field.default {
                    rewrite_type_refs_in_expr(default, replacements);
                }
            }
        }
        Decl::Constructor(decl) => {
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_constructor_clause(clause, replacements);
            }
        }
        Decl::Function(decl) => {
            for bound in &mut decl.generic_bounds {
                rewrite_type_text(bound, replacements);
            }
            for param in &mut decl.params {
                rewrite_type_refs_in_param(param, replacements);
            }
            rewrite_type_text(&mut decl.return_type.text, replacements);
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Method(decl) => {
            for bound in &mut decl.generic_bounds {
                rewrite_type_text(bound, replacements);
            }
            rewrite_type_refs_in_param(&mut decl.receiver, replacements);
            for param in &mut decl.params {
                rewrite_type_refs_in_param(param, replacements);
            }
            rewrite_type_text(&mut decl.return_type.text, replacements);
            for clause in &mut decl.clauses {
                rewrite_type_refs_in_function_clause(clause, replacements);
            }
        }
        Decl::Trait(decl) => {
            for super_trait in &mut decl.super_traits {
                rewrite_type_text(super_trait, replacements);
            }
            for method in &mut decl.methods {
                for bound in &mut method.generic_bounds {
                    rewrite_type_text(bound, replacements);
                }
                for param in &mut method.params {
                    rewrite_type_refs_in_param(param, replacements);
                }
                rewrite_type_text(&mut method.return_type.text, replacements);
                if let Some(default_body) = &mut method.default_body {
                    rewrite_type_refs_in_expr(default_body, replacements);
                }
            }
        }
        Decl::TraitImpl(decl) => {
            rewrite_type_text(&mut decl.trait_ref.text, replacements);
            rewrite_type_text(&mut decl.for_type.text, replacements);
            for method in &mut decl.methods {
                rewrite_type_refs_in_decl(&mut Decl::Function(method.clone()), replacements);
                for bound in &mut method.generic_bounds {
                    rewrite_type_text(bound, replacements);
                }
                for param in &mut method.params {
                    rewrite_type_refs_in_param(param, replacements);
                }
                rewrite_type_text(&mut method.return_type.text, replacements);
                for clause in &mut method.clauses {
                    rewrite_type_refs_in_function_clause(clause, replacements);
                }
            }
        }
        Decl::Template(decl) => {
            for prop in &mut decl.props {
                rewrite_type_text(&mut prop.annotation.text, replacements);
                if let Some(default) = &mut prop.default {
                    rewrite_type_refs_in_expr(default, replacements);
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

pub(super) fn rewrite_type_refs_in_constructor_clause(
    clause: &mut ConstructorClause,
    replacements: &BTreeMap<String, String>,
) {
    for param in &mut clause.params {
        rewrite_type_text(&mut param.annotation.text, replacements);
        if let Some(default) = &mut param.default {
            rewrite_type_refs_in_expr(default, replacements);
        }
    }
    rewrite_type_text(&mut clause.return_type.text, replacements);
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

pub(super) fn rewrite_type_refs_in_function_clause(
    clause: &mut crate::terlan_syntax::parse_tree::FunctionClause,
    replacements: &BTreeMap<String, String>,
) {
    if let Some(guard) = &mut clause.guard {
        rewrite_type_refs_in_expr(guard, replacements);
    }
    rewrite_type_refs_in_expr(&mut clause.body, replacements);
}

pub(super) fn rewrite_type_refs_in_param(
    param: &mut Param,
    replacements: &BTreeMap<String, String>,
) {
    rewrite_type_text(&mut param.annotation.text, replacements);
    if let Some(default) = &mut param.default {
        rewrite_type_refs_in_expr(default, replacements);
    }
}

pub(super) fn rewrite_value_call_refs_in_expr(
    expr: &mut Expr,
    replacements: &BTreeMap<(String, String), String>,
) {
    match expr {
        Expr::Tuple(items)
        | Expr::List(items)
        | Expr::FixedArray(items)
        | Expr::Sequence(items) => {
            for item in items {
                rewrite_value_call_refs_in_expr(item, replacements);
            }
        }
        Expr::ListCons(left, right)
        | Expr::Index(left, right)
        | Expr::BinaryOp { left, right, .. } => {
            rewrite_value_call_refs_in_expr(left, replacements);
            rewrite_value_call_refs_in_expr(right, replacements);
        }
        Expr::IndexAssign {
            collection,
            index,
            value,
        } => {
            rewrite_value_call_refs_in_expr(collection, replacements);
            rewrite_value_call_refs_in_expr(index, replacements);
            rewrite_value_call_refs_in_expr(value, replacements);
        }
        Expr::Map(fields) | Expr::RecordConstruct { fields, .. } => {
            for field in fields {
                rewrite_value_call_refs_in_expr(&mut field.value, replacements);
            }
        }
        Expr::BinaryLayout { .. } => {}
        Expr::RecordUpdate { value, fields, .. } => {
            rewrite_value_call_refs_in_expr(value, replacements);
            for field in fields {
                rewrite_value_call_refs_in_expr(&mut field.value, replacements);
            }
        }
        Expr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite_value_call_refs_in_expr(expr, replacements);
            for generator in generators {
                rewrite_value_call_refs_in_expr(&mut generator.source, replacements);
            }
            for guard in guards {
                rewrite_value_call_refs_in_expr(guard, replacements);
            }
        }
        Expr::Let {
            bindings,
            else_clauses,
            body,
        } => {
            for binding in bindings {
                rewrite_value_call_refs_in_expr(&mut binding.value, replacements);
            }
            for clause in else_clauses {
                rewrite_value_call_refs_in_case_clause(clause, replacements);
            }
            if let Some(body) = body {
                rewrite_value_call_refs_in_expr(body, replacements);
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            remote,
            ..
        } => {
            if type_args.is_empty() {
                if let (Some(remote_name), Some(function_name)) =
                    (remote.as_ref(), value_call_callee_name(callee))
                {
                    if let Some(local_name) =
                        replacements.get(&(remote_name.clone(), function_name.clone()))
                    {
                        **callee = Expr::Atom(local_name.clone());
                        *remote = None;
                    }
                }
            }
            rewrite_value_call_refs_in_expr(callee, replacements);
            for arg in args {
                rewrite_value_call_refs_in_expr(arg, replacements);
            }
        }
        Expr::Case { scrutinee, clauses } => {
            rewrite_value_call_refs_in_expr(scrutinee, replacements);
            for clause in clauses {
                rewrite_value_call_refs_in_case_clause(clause, replacements);
            }
        }
        Expr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite_value_call_refs_in_expr(body, replacements);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                rewrite_value_call_refs_in_case_clause(clause, replacements);
            }
            if let Some(after) = after_clause {
                rewrite_value_call_refs_in_expr(&mut after.trigger, replacements);
                rewrite_value_call_refs_in_expr(&mut after.body, replacements);
            }
        }
        Expr::If { clauses } => {
            for clause in clauses {
                rewrite_value_call_refs_in_expr(&mut clause.condition, replacements);
                rewrite_value_call_refs_in_expr(&mut clause.body, replacements);
            }
        }
        Expr::Fun { clauses } => {
            for clause in clauses {
                rewrite_value_call_refs_in_function_clause(clause, replacements);
            }
        }
        Expr::MacroCall { args, .. } => {
            for arg in args {
                rewrite_value_call_refs_in_expr(arg, replacements);
            }
        }
        Expr::RawMacro { interpolations, .. } => {
            for interpolation in interpolations {
                rewrite_value_call_refs_in_expr(interpolation, replacements);
            }
        }
        Expr::HtmlBlock(block) => rewrite_value_call_refs_in_html_block(block, replacements),
        Expr::RecordAccess { value, .. } | Expr::FieldAccess { value, .. } => {
            rewrite_value_call_refs_in_expr(value, replacements);
        }
        Expr::ConstructorChain { base, record } => {
            rewrite_value_call_refs_in_expr(base, replacements);
            rewrite_value_call_refs_in_expr(record, replacements);
        }
        Expr::UnaryOp { expr, .. } | Expr::Quote(expr) | Expr::Unquote(expr) => {
            rewrite_value_call_refs_in_expr(expr, replacements);
        }
        Expr::Cast { expr, .. } => rewrite_value_call_refs_in_expr(expr, replacements),
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Atom(_)
        | Expr::AtomLiteral(_)
        | Expr::Binary(_)
        | Expr::Var(_) => {}
    }
}
