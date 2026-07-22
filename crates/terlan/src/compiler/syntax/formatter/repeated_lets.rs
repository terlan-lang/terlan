use crate::terlan_syntax::parser::{
    parse_module_for_repeated_let_migration, repeated_let_migration_offsets, ParseError,
};

use super::format_module;

/// Formats retired implicit bindings as canonical repeated `let` syntax.
pub fn format_source_module_migrating_repeated_lets(source: &str) -> Result<String, ParseError> {
    parse_module_for_repeated_let_migration(source).map(|module| format_module(&module))
}

/// Inserts missing repeated `let` keywords without reformatting source text.
pub fn migrate_repeated_let_source(source: &str) -> Result<String, ParseError> {
    let mut migrated = source.to_string();
    for offset in repeated_let_migration_offsets(source)?.into_iter().rev() {
        migrated.insert_str(offset, "let ");
    }
    Ok(migrated)
}
