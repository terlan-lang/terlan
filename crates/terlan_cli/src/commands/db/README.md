# DB Command Internals

This directory owns `terlc db`. It validates Terlan SQL migration files,
compares them with applied database history, and applies migrations through the
maintained Rust/Tokio Postgres adapter in `terlan_safenative`.

The DB command path must not shell out to `psql`, hand-roll the Postgres wire
protocol, or embed database-driver behavior in the CLI router. Live validation
uses Docker-backed Postgres gates so local and CI tests exercise the same
maintained adapter path.

## Responsibilities

- Parse `terlc db` command-local arguments.
- Discover migration files from a migration directory.
- Validate timestamped migration filenames.
- Split SQL files into `Up` and optional `Down` sections.
- Compute deterministic migration checksums.
- Read applied migration history from Postgres.
- Render pending, applied, missing, and divergent status rows.
- Apply pending migrations through SafeNative Postgres.
- Refuse destructive `rebuild --dev` / `reset --dev` unless the target is
  development-scoped.

## Public Surface

- `args`: command-local parser for `init`, `new`, `validate`, `status`,
  `migrate`, `rebuild`, and `reset`.
- `execution`: SafeNative-backed migration executor for `migrate`,
  `rebuild --dev`, and `reset --dev`.
- `history`: SafeNative-backed applied-history reader for `status` and pending
  migration planning.
- `migration`: migration file discovery, checksum loading, marker parsing, and
  execution-input shaping.
- `status`: migration-history table SQL, row validation, pending selection, and
  status comparison.
- `mod`: CLI dispatch, output rendering, database URL resolution, and
  development-target safety checks.

## Migration Format

Migration files are plain SQL with Terlan-owned markers:

```sql
-- +terlan Up
CREATE TABLE users (id BIGSERIAL PRIMARY KEY);

-- +terlan Down
DROP TABLE users;
```

Important invariants:

- `Up` is required.
- `Down` is optional and must follow `Up`.
- Filenames use `YYYYMMDDHHMMSS_name.sql`.
- Duplicate migration timestamps are rejected.
- Discovery is shallow; nested directories are ignored.
- Checksums are SHA-256 hashes of the complete migration source file.
- Duplicate markers are rejected.
- Unknown `-- +terlan ...` markers are rejected.
- Empty `Up` sections are rejected.

## Live Adapter Contract

`terlc db status`, `terlc db migrate`, and destructive development commands use
`terlan_safenative::postgres`, backed by `deadpool-postgres` and
`tokio-postgres`.

Execution flow:

1. Resolve `--database-url` or `TERLAN_DATABASE_URL`.
2. Validate the URL through the SafeNative Postgres config validator.
3. Connect through the maintained pool.
4. Ensure `terlan_schema_migrations` exists.
5. Load applied history for status or pending planning.
6. Apply pending migrations in transactions.
7. Insert migration history rows through parameter binding.

`rebuild --dev` and `reset --dev` additionally drop and recreate the `public`
schema before applying migrations. They refuse production-looking targets before
connecting.

## Testing Notes

- `migration_test.rs` covers pure migration parsing and status comparison.
- `history_test.rs` covers SafeNative row conversion into applied history.
- `execution_test.rs` covers executor request/report and SQL batch shaping.
- `mod_test.rs` covers CLI argument parsing, command routing, destructive
  guards, unreachable-database behavior, and Docker-backed live lifecycle.
- `make db-command-check` runs the DB command gate.
- `make safenative-postgres-docker-check` starts disposable Postgres for live
  adapter validation.
