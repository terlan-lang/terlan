//! Terlan SQL migration marker parsing.
//!
//! This module owns only the Terlan marker splitter. It does not execute SQL,
//! open database connections, or model migration history.

use std::fs;
use std::path::{Path, PathBuf};

use crate::support::sha256sum_file;

pub(crate) use super::status::{
    applied_migration_from_history_row, migration_history_insert_sql, migration_history_select_sql,
    migration_history_table_sql, migration_status, pending_migration_engine_inputs,
    AppliedMigration, MigrationStatusEntry, MigrationStatusState, MIGRATION_HISTORY_TABLE,
};

/// Parsed Terlan migration filename.
///
/// Inputs:
/// - Produced by `parse_migration_file_name` from one basename.
///
/// Output:
/// - `version`: fourteen-digit timestamp string.
/// - `name`: descriptive snake-case migration name.
///
/// Transformation:
/// - Keeps ordering metadata separate from SQL section parsing so migration
///   discovery can sort and diagnose files before reading SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationFileName {
    pub(crate) version: String,
    pub(crate) name: String,
}

/// Discovered migration file with parsed metadata.
///
/// Inputs:
/// - Produced by `discover_migration_files` from filesystem entries.
///
/// Output:
/// - `path`: filesystem path to the migration file.
/// - `file_name`: basename used for diagnostics.
/// - `parsed`: timestamp/name metadata used for ordering.
///
/// Transformation:
/// - Ties parsed migration metadata back to the concrete file path that
///   validation and execution read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationFile {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) parsed: MigrationFileName,
}

/// Fully loaded migration file with parsed SQL sections.
///
/// Inputs:
/// - Produced by `load_migration_file` or `load_migration_files` from
///   discovered migration metadata.
///
/// Output:
/// - `file`: discovered migration path/name/version metadata.
/// - `sections`: parsed `Up` and optional `Down` SQL sections.
/// - `checksum`: SHA-256 hash of the complete migration source file.
///
/// Transformation:
/// - Combines filesystem discovery with marker parsing while keeping database
///   execution out of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedMigration {
    pub(crate) file: MigrationFile,
    pub(crate) sections: MigrationSections,
    pub(crate) checksum: String,
}

/// Database migration-engine input.
///
/// Inputs:
/// - Produced by `migration_engine_inputs` from loaded Terlan migration files.
///
/// Output:
/// - Version/name metadata, SQL bodies, checksums, and source line offsets
///   needed by the migration execution adapter.
///
/// Transformation:
/// - Flattens Terlan marker sections into an execution-oriented shape without
///   opening a database or choosing the final migration engine API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationEngineInput {
    pub(crate) version: String,
    pub(crate) name: String,
    pub(crate) up_sql: String,
    pub(crate) up_start_line: usize,
    pub(crate) down_sql: Option<String>,
    pub(crate) down_start_line: Option<usize>,
    pub(crate) checksum: String,
}

/// Parsed SQL sections for one Terlan migration file.
///
/// Inputs:
/// - Produced by `split_migration_sections` from one migration source string.
///
/// Output:
/// - `up`: required SQL section.
/// - `down`: optional SQL section.
///
/// Transformation:
/// - Keeps the user-authored SQL text together with section line offsets so
///   later validation can point database diagnostics back at the migration
///   file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationSections {
    pub(crate) up: MigrationSection,
    pub(crate) down: Option<MigrationSection>,
}

/// One parsed SQL migration section.
///
/// Inputs:
/// - SQL lines captured after a Terlan section marker.
///
/// Output:
/// - `sql`: section body text.
/// - `start_line`: one-based line number for the first body line.
///
/// Transformation:
/// - Preserves source offsets independently from marker line numbers so a SQL
///   parser or database error can be translated back to the original file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationSection {
    pub(crate) sql: String,
    pub(crate) start_line: usize,
}

/// Stable migration parser diagnostic.
///
/// Inputs:
/// - Produced when a migration file violates Terlan marker rules.
///
/// Output:
/// - `line`: one-based line number closest to the failure.
/// - `message`: stable human-readable diagnostic text.
///
/// Transformation:
/// - Keeps parser failures independent from command formatting so future
///   `terlc db validate` can render the same diagnostics in CLI and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationDiagnostic {
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// Stable filesystem discovery diagnostic for migrations.
///
/// Inputs:
/// - Produced by `discover_migration_files` when directory access or filename
///   validation fails.
///
/// Output:
/// - `path`: path being read or validated.
/// - `message`: stable human-readable diagnostic text.
///
/// Transformation:
/// - Normalizes filesystem and filename failures into one command-facing shape
///   without adding command formatting to this pure migration module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationDiscoveryDiagnostic {
    pub(crate) path: PathBuf,
    pub(crate) message: String,
}

/// Stable migration file loading diagnostic.
///
/// Inputs:
/// - Produced by `load_migration_file` or `load_migration_files` when source
///   text cannot be read or parsed.
///
/// Output:
/// - `path`: migration file path closest to the failure.
/// - `line`: one-based source line for parser diagnostics, or `1` for file
///   read errors.
/// - `message`: stable human-readable diagnostic text.
///
/// Transformation:
/// - Normalizes IO and marker parsing failures so command rendering can point
///   at a concrete migration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationLoadDiagnostic {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// SQL section marker found in a migration file.
///
/// Inputs:
/// - Parsed from migration comments such as `-- migrate:up`.
///
/// Output:
/// - `Up` for forward migration SQL or `Down` for rebuild/development reset
///   SQL.
///
/// Transformation:
/// - Keeps marker handling typed while the parser scans migration lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    Up,
    Down,
}

/// Parses one Terlan migration filename.
///
/// Inputs:
/// - `file_name`: basename such as `20260619123000_create_users.sql`.
///
/// Output:
/// - `Ok(MigrationFileName)` when the filename matches the Terlan migration
///   convention.
/// - `Err(MigrationDiagnostic)` when the name, timestamp, slug, or extension
///   is invalid.
///
/// Transformation:
/// - Validates the filename without touching the filesystem, splits timestamp
///   and slug, and returns stable diagnostics with line `1` because filenames
///   have no source line context.
pub(crate) fn parse_migration_file_name(
    file_name: &str,
) -> Result<MigrationFileName, MigrationDiagnostic> {
    let Some(stem) = file_name.strip_suffix(".sql") else {
        return Err(diagnostic(1, "migration filename must end with `.sql`"));
    };
    let Some((version, name)) = stem.split_once('_') else {
        return Err(diagnostic(
            1,
            "migration filename must use `YYYYMMDDHHMMSS_name.sql`",
        ));
    };
    if version.len() != 14 || !version.chars().all(|character| character.is_ascii_digit()) {
        return Err(diagnostic(
            1,
            "migration filename timestamp must use fourteen digits",
        ));
    }
    if !is_valid_migration_name(name) {
        return Err(diagnostic(
            1,
            "migration filename name must be snake_case letters, digits, and underscores",
        ));
    }

    Ok(MigrationFileName {
        version: version.to_string(),
        name: name.to_string(),
    })
}

/// Discovers migration files in one directory.
///
/// Inputs:
/// - `directory`: filesystem directory expected to contain migration files.
///
/// Output:
/// - `Ok(Vec<MigrationFile>)` sorted by migration timestamp.
/// - `Err(MigrationDiscoveryDiagnostic)` when the directory cannot be read, an
///   entry cannot be inspected, a filename is invalid, or duplicate timestamps
///   are present.
///
/// Transformation:
/// - Reads immediate directory entries, accepts only regular files, validates
///   each filename with `parse_migration_file_name`, sorts by timestamp, and
///   keeps paths attached for future SQL loading.
pub(crate) fn discover_migration_files(
    directory: &Path,
) -> Result<Vec<MigrationFile>, MigrationDiscoveryDiagnostic> {
    let entries = fs::read_dir(directory).map_err(|error| {
        discovery_diagnostic(
            directory.to_path_buf(),
            &format!("cannot read migration directory: {error}"),
        )
    })?;

    let mut migrations = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            discovery_diagnostic(
                directory.to_path_buf(),
                &format!("cannot read migration directory entry: {error}"),
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            discovery_diagnostic(
                path.clone(),
                &format!("cannot inspect migration file: {error}"),
            )
        })?;
        if !file_type.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return Err(discovery_diagnostic(
                path,
                "migration filename must be valid UTF-8",
            ));
        };
        let file_name = file_name.to_string();
        let parsed = parse_migration_file_name(&file_name).map_err(|diagnostic| {
            discovery_diagnostic(
                path.clone(),
                &format!("{} at line {}", diagnostic.message, diagnostic.line),
            )
        })?;
        migrations.push(MigrationFile {
            path,
            file_name,
            parsed,
        });
    }

    migrations.sort_by(|left, right| left.parsed.version.cmp(&right.parsed.version));
    for pair in migrations.windows(2) {
        if pair[0].parsed.version == pair[1].parsed.version {
            return Err(discovery_diagnostic(
                pair[1].path.clone(),
                "duplicate migration timestamp in migration filenames",
            ));
        }
    }

    Ok(migrations)
}

/// Loads and parses one discovered migration file.
///
/// Inputs:
/// - `file`: migration metadata returned by `discover_migration_files`.
///
/// Output:
/// - `Ok(LoadedMigration)` containing metadata and parsed SQL sections.
/// - `Err(MigrationLoadDiagnostic)` when the file cannot be read or its marker
///   sections are invalid, or when checksum generation fails.
///
/// Transformation:
/// - Hashes and reads UTF-8 source text from disk, applies
///   `split_migration_sections`, and attaches diagnostics to the migration
///   path.
pub(crate) fn load_migration_file(
    file: &MigrationFile,
) -> Result<LoadedMigration, MigrationLoadDiagnostic> {
    let checksum = sha256sum_file(&file.path).map_err(|error| MigrationLoadDiagnostic {
        path: file.path.clone(),
        line: 1,
        message: format!("cannot checksum migration file: {error}"),
    })?;
    let source = fs::read_to_string(&file.path).map_err(|error| MigrationLoadDiagnostic {
        path: file.path.clone(),
        line: 1,
        message: format!("cannot read migration file: {error}"),
    })?;
    let sections =
        split_migration_sections(&source).map_err(|diagnostic| MigrationLoadDiagnostic {
            path: file.path.clone(),
            line: diagnostic.line,
            message: diagnostic.message,
        })?;

    Ok(LoadedMigration {
        file: file.clone(),
        sections,
        checksum,
    })
}

/// Loads and parses discovered migration files in deterministic order.
///
/// Inputs:
/// - `files`: ordered migration metadata returned by `discover_migration_files`.
///
/// Output:
/// - `Ok(Vec<LoadedMigration>)` preserving input order.
/// - `Err(MigrationLoadDiagnostic)` for the first unreadable or invalid file.
///
/// Transformation:
/// - Maps discovered files into parsed migration records without performing
///   checksum, history, or database execution work.
pub(crate) fn load_migration_files(
    files: &[MigrationFile],
) -> Result<Vec<LoadedMigration>, MigrationLoadDiagnostic> {
    files.iter().map(load_migration_file).collect()
}

/// Converts loaded Terlan migrations into migration-engine inputs.
///
/// Inputs:
/// - `loaded`: validated, checksummed migration files in deterministic order.
///
/// Output:
/// - One migration-engine input per loaded migration, preserving order.
///
/// Transformation:
/// - Copies parsed filename metadata, `Up` SQL, optional `Down` SQL, section
///   line offsets, and full-file checksum into a database-independent shape
///   suitable for the maintained live execution adapter.
pub(crate) fn migration_engine_inputs(loaded: &[LoadedMigration]) -> Vec<MigrationEngineInput> {
    loaded
        .iter()
        .map(|migration| MigrationEngineInput {
            version: migration.file.parsed.version.clone(),
            name: migration.file.parsed.name.clone(),
            up_sql: migration.sections.up.sql.clone(),
            up_start_line: migration.sections.up.start_line,
            down_sql: migration
                .sections
                .down
                .as_ref()
                .map(|section| section.sql.clone()),
            down_start_line: migration
                .sections
                .down
                .as_ref()
                .map(|section| section.start_line),
            checksum: migration.checksum.clone(),
        })
        .collect()
}

/// Splits one Terlan migration file into `Up` and optional `Down` sections.
///
/// Inputs:
/// - `source`: complete migration file text.
///
/// Output:
/// - `Ok(MigrationSections)` when the file has one non-empty `Up` section and
///   at most one `Down` section after `Up`.
/// - `Err(MigrationDiagnostic)` when markers are missing, duplicated,
///   unknown, or out of order.
///
/// Transformation:
/// - Scans source lines for exact `-- +terlan Up` and `-- +terlan Down`
///   markers, appends following SQL lines to the active section, and records
///   the first SQL line for each section.
pub(crate) fn split_migration_sections(
    source: &str,
) -> Result<MigrationSections, MigrationDiagnostic> {
    let mut up = SectionBuilder::new();
    let mut down = SectionBuilder::new();
    let mut active = None;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        if let Some(marker) = parse_marker(line, line_number)? {
            match marker {
                Marker::Up => {
                    if up.seen {
                        return Err(diagnostic(line_number, "duplicate `-- +terlan Up` marker"));
                    }
                    up.mark_seen(line_number + 1);
                    active = Some(Marker::Up);
                }
                Marker::Down => {
                    if !up.seen {
                        return Err(diagnostic(
                            line_number,
                            "`-- +terlan Down` marker must follow `-- +terlan Up`",
                        ));
                    }
                    if down.seen {
                        return Err(diagnostic(
                            line_number,
                            "duplicate `-- +terlan Down` marker",
                        ));
                    }
                    down.mark_seen(line_number + 1);
                    active = Some(Marker::Down);
                }
            }
            continue;
        }

        match active {
            Some(Marker::Up) => up.push(line),
            Some(Marker::Down) => down.push(line),
            None => {}
        }
    }

    if !up.seen {
        return Err(diagnostic(1, "missing required `-- +terlan Up` marker"));
    }

    let up_section = up.finish();
    if up_section.sql.trim().is_empty() {
        return Err(diagnostic(
            up_section.start_line,
            "`-- +terlan Up` section must not be empty",
        ));
    }

    Ok(MigrationSections {
        up: up_section,
        down: down.seen.then(|| down.finish()),
    })
}

/// Parses a Terlan migration marker line.
///
/// Inputs:
/// - `line`: one source line.
/// - `line_number`: one-based line number for diagnostics.
///
/// Output:
/// - `Ok(Some(Marker))` for known markers.
/// - `Ok(None)` for normal SQL/comment lines.
/// - `Err(MigrationDiagnostic)` for unknown Terlan marker comments.
///
/// Transformation:
/// - Trims surrounding whitespace, recognizes exact marker spellings, and
///   rejects misspelled Terlan marker names instead of silently treating them
///   as SQL comments.
fn parse_marker(line: &str, line_number: usize) -> Result<Option<Marker>, MigrationDiagnostic> {
    let trimmed = line.trim();
    match trimmed {
        "-- +terlan Up" => Ok(Some(Marker::Up)),
        "-- +terlan Down" => Ok(Some(Marker::Down)),
        _ if trimmed.starts_with("-- +terlan ") => {
            Err(diagnostic(line_number, "unknown Terlan migration marker"))
        }
        _ => Ok(None),
    }
}

/// Builds a migration parser diagnostic.
///
/// Inputs:
/// - `line`: one-based source line number.
/// - `message`: stable diagnostic message.
///
/// Output:
/// - `MigrationDiagnostic` containing both fields.
///
/// Transformation:
/// - Allocates the message once at the parser boundary.
fn diagnostic(line: usize, message: &str) -> MigrationDiagnostic {
    MigrationDiagnostic {
        line,
        message: message.to_string(),
    }
}

/// Builds a migration discovery diagnostic.
///
/// Inputs:
/// - `path`: filesystem path closest to the failure.
/// - `message`: stable diagnostic message.
///
/// Output:
/// - `MigrationDiscoveryDiagnostic`.
///
/// Transformation:
/// - Allocates path/message once at the filesystem boundary.
fn discovery_diagnostic(path: PathBuf, message: &str) -> MigrationDiscoveryDiagnostic {
    MigrationDiscoveryDiagnostic {
        path,
        message: message.to_string(),
    }
}

/// Returns whether a migration descriptive name is valid.
///
/// Inputs:
/// - `name`: filename stem after the timestamp separator.
///
/// Output:
/// - `true` when the name is non-empty snake_case starting with a lowercase
///   ASCII letter.
///
/// Transformation:
/// - Applies Terlan's conservative migration filename slug rule without
///   allocating.
pub(crate) fn is_valid_migration_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !name.ends_with('_')
        && !name.contains("__")
}

/// Incrementally builds one migration SQL section.
///
/// Inputs:
/// - Marker observations and SQL lines from the migration parser.
///
/// Output:
/// - Section metadata and collected SQL text.
///
/// Transformation:
/// - Tracks whether a section marker was seen, the marker line, and all SQL
///   lines that belong to that section.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionBuilder {
    seen: bool,
    start_line: usize,
    lines: Vec<String>,
}

impl SectionBuilder {
    /// Creates an empty section builder.
    ///
    /// Inputs:
    /// - No runtime input.
    ///
    /// Output:
    /// - Builder with no marker observed and no SQL lines.
    ///
    /// Transformation:
    /// - Initializes parser-owned section state for one marker kind.
    fn new() -> Self {
        Self {
            seen: false,
            start_line: 1,
            lines: Vec::new(),
        }
    }

    /// Marks this section as present.
    ///
    /// Inputs:
    /// - `start_line`: one-based line where section SQL starts.
    ///
    /// Output:
    /// - Updates this builder in place.
    ///
    /// Transformation:
    /// - Records marker presence and the first body line used for later
    ///   diagnostics.
    fn mark_seen(&mut self, start_line: usize) {
        self.seen = true;
        self.start_line = start_line;
    }

    /// Appends one SQL line to this section.
    ///
    /// Inputs:
    /// - `line`: source SQL line without trailing newline.
    ///
    /// Output:
    /// - Updates this builder in place.
    ///
    /// Transformation:
    /// - Stores the line as owned text so the final section can be returned
    ///   independently from the input buffer.
    fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    /// Finishes this builder into a section.
    ///
    /// Inputs:
    /// - This builder's captured lines and start line.
    ///
    /// Output:
    /// - `MigrationSection`.
    ///
    /// Transformation:
    /// - Joins captured SQL lines with `\n`, preserving blank lines inside the
    ///   section.
    fn finish(self) -> MigrationSection {
        MigrationSection {
            sql: self.lines.join("\n"),
            start_line: self.start_line,
        }
    }
}
