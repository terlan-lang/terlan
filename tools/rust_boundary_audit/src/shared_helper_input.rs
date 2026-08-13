use std::fmt::Write as _;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use quote::ToTokens;
use sha2::{Digest, Sha256};
use syn::visit::{self, Visit};
use syn::{Arm, Block, ImplItemFn, ItemFn, Stmt};

use super::AuditError;

const MIN_COMPLEXITY_UNITS: usize = 12;

struct HelperRow {
    hash: String,
    complexity_units: usize,
    path: String,
    line: usize,
    name: String,
}

#[derive(Default)]
struct Complexity {
    units: usize,
}

impl<'ast> Visit<'ast> for Complexity {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        self.units += 1;
        visit::visit_stmt(self, statement);
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.units += 1;
        visit::visit_arm(self, arm);
    }
}

struct HelperVisitor<'a> {
    path: &'a str,
    rows: Vec<HelperRow>,
}

impl HelperVisitor<'_> {
    fn record(&mut self, name: &syn::Ident, block: &Block) {
        let mut complexity = Complexity::default();
        complexity.visit_block(block);
        if complexity.units < MIN_COMPLEXITY_UNITS {
            return;
        }
        let normalized = block.to_token_stream().to_string();
        let digest = Sha256::digest(normalized.as_bytes());
        let mut hash = String::with_capacity(16);
        for byte in &digest[..8] {
            let _ = write!(hash, "{byte:02x}");
        }
        self.rows.push(HelperRow {
            hash,
            complexity_units: complexity.units,
            path: self.path.to_owned(),
            line: name.span().start().line,
            name: name.to_string(),
        });
    }
}

impl<'ast> Visit<'ast> for HelperVisitor<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.record(&item.sig.ident, &item.block);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.record(&item.sig.ident, &item.block);
        visit::visit_impl_item_fn(self, item);
    }
}

fn implementation_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    path.extension().and_then(|extension| extension.to_str()) == Some("rs")
        && !text.contains("/tests/")
        && !text.contains("_test/")
        && !text.contains("/fixtures/")
        && !text.contains("/generated/")
        && !text.ends_with("_test.rs")
}

fn collect_files(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), AuditError> {
    for entry in fs::read_dir(path).map_err(|error| {
        AuditError::Message(format!("cannot read `{}`: {error}", path.display()))
    })? {
        let child = entry
            .map_err(|error| {
                AuditError::Message(format!("cannot inspect `{}`: {error}", path.display()))
            })?
            .path();
        if child.is_dir() {
            collect_files(&child, output)?;
        } else if implementation_path(&child) {
            output.push(child);
        }
    }
    Ok(())
}

pub(super) fn write_shared_helper_input(root: &Path, path: &Path) -> Result<(), AuditError> {
    let mut files = Vec::new();
    collect_files(&root.join("crates"), &mut files)?;
    files.sort();
    let mut rows = Vec::new();
    for file_path in files {
        let source = fs::read_to_string(&file_path).map_err(|error| {
            AuditError::Message(format!("cannot read `{}`: {error}", file_path.display()))
        })?;
        let relative = file_path
            .strip_prefix(root)
            .map_err(|error| AuditError::Message(error.to_string()))?
            .to_string_lossy()
            .into_owned();
        let syntax = syn::parse_file(&source).map_err(|error| {
            AuditError::Message(format!(
                "cannot parse `{relative}` for shared helpers: {error}"
            ))
        })?;
        let mut visitor = HelperVisitor {
            path: &relative,
            rows: Vec::new(),
        };
        visitor.visit_file(&syntax);
        rows.extend(visitor.rows);
    }
    let file = fs::File::create(path).map_err(|error| {
        AuditError::Message(format!("cannot create `{}`: {error}", path.display()))
    })?;
    let mut output = BufWriter::new(file);
    writeln!(output, "schema\tterlan.rust-shared-helper-input.v1")
        .and_then(|_| writeln!(output, "hash\tcomplexity_units\tpath\tline\tname"))
        .map_err(|error| AuditError::Message(error.to_string()))?;
    for row in rows {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            row.hash, row.complexity_units, row.path, row.line, row.name
        )
        .map_err(|error| AuditError::Message(error.to_string()))?;
    }
    output
        .flush()
        .map_err(|error| AuditError::Message(error.to_string()))
}
