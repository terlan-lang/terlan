# DB Command Internals

This directory owns `terlc db`. It validates Terlan SQL migration files,
compares them with applied database history, and applies migrations through the
VM-owned nonblocking libpq worker.

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
- Render pending, applied, missing, out-of-order, checksum-mismatch, and
  name-mismatch status rows.
- Apply pending migrations through VM-owned Postgres resources.
- Capture deterministic schema snapshots with migration and catalog
  fingerprints, and reject snapshot corruption or drift.
- Refuse destructive `rebuild` / `reset` unless both `--dev` and `--confirm`
  are present and the target is explicitly loopback-local.
- Discover and start validated project-owned Compose dependencies before local
  live database commands, while never touching them for remote targets. Preserve
  pre-existing containers and remove only containers owned by the command's
  scoped dependency session.

## Public Surface

- `args`: command-local parser for `init`, `new`, `validate`, `status`,
  `snapshot`, `migrate`, `rebuild`, and `reset`.
- `execution`: VM-backed migration executor for `migrate`,
  `rebuild --dev --confirm`, and `reset --dev --confirm`.
- `history`: VM-backed applied-history reader for `status` and pending
  migration planning.
- `migration`: migration file discovery, checksum loading, marker parsing, and
  execution-input shaping.
- `snapshot`: canonical Postgres relation, column, constraint, index, and enum
  introspection plus deterministic snapshot write/check behavior.
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
- Duplicate IDs fail with `error[db.migration.duplicate_id]` and identify the
  conflicting timestamp.
- Discovery is shallow; nested directories are ignored.
- Checksums are SHA-256 hashes of the complete migration source file.
- Duplicate markers are rejected.
- Unknown `-- +terlan ...` markers are rejected.
- Empty `Up` sections are rejected.

## Live Adapter Contract

`terlc db status`, `terlc db migrate`, and destructive development commands use
the VM-owned Postgres actor runtime and nonblocking libpq worker.

Execution flow:

1. Resolve `--database-url` or `TERLAN_DATABASE_URL`.
2. Validate the URL through the shared Postgres config validator.
3. For loopback targets, discover the nearest `terlan.toml`, validate any
   project-owned Compose file through the shared typed parser, classify existing
   containers as external, and await its Postgres healthcheck.
4. Connect through the VM-owned pool resource.
5. Begin one transaction for the complete migration command.
6. Acquire the nonblocking database-scoped migration advisory lock.
7. Perform any development reset and ensure `terlan_schema_migrations` exists.
8. Reload history while holding the lock and reject divergent concurrent work.
9. Reject any unapplied local migration ordered before a compatible applied row.
10. Apply all pending migration bodies and parameterized history rows atomically.
11. Commit and release the lock, or roll back both schema and history changes.

A migration body or its history insert that fails is rolled back and reported as
`error[db.migration.failed]` with the migration ID. The diagnostic preserves the
VM Postgres error but never includes migration SQL, checksums, database URLs, or
credentials.

A concurrent migration command fails immediately with
`error[db.migration.lock_conflict]`; it does not wait indefinitely or execute
against stale planning state. Transaction-scoped lock ownership also guarantees
that a database error or client exit releases the lock.

A non-prefix history such as applied migrations `1` and `3` with local migration
`2` still pending is reported as `out-of-order` and migration execution fails with
`error[db.migration.out_of_order]`. Terlan never applies migration `2` after `3`.

Status rows include the database-recorded `applied_at` value as canonical RFC
3339 UTC with microsecond precision. Pending rows print `-`; migration identity
and deterministic schema snapshots never incorporate this wall-clock metadata.

Initial planning distinguishes absent local files, edited checksums, and renamed
migrations as `error[db.migration.file_missing]`,
`error[db.migration.checksum_mismatch]`, and
`error[db.migration.name_mismatch]`. `error[db.migration.history_divergent]` is
reserved for compatible history that changes while the command waits for and
then acquires the database lock.

`rebuild --dev --confirm` and `reset --dev --confirm` additionally drop and
recreate the `public` schema before applying migrations. Before reading migration
files or opening a socket, they reject every non-loopback host and URLs containing
strict TLS or certificate options such as `sslmode=require`, `sslcert`, `sslkey`,
or `sslrootcert`. A database name containing `dev`, `test`, or `local` never makes
a remote target eligible for destructive work.

`terlc db snapshot` writes `db/schema.snapshot.json` by default. `--output`
selects another path, and `--check` compares the persisted snapshot with the
current database and ordered migration checksums without rewriting it. The
snapshot excludes Terlan's internal migration-history relation and never stores
the database URL or credentials.

Snapshot checking distinguishes unmanaged schema changes from an outdated
snapshot. An unchanged migration identity with a different live schema emits
`error[db.schema.dirty]`; changed migration identity emits
`error[db.snapshot.drift]`. Both diagnostics report fingerprints without
including SQL or connection credentials.

Malformed snapshot JSON and forged schema fingerprints emit
`error[db.snapshot.corrupt]` with the concrete integrity failure. They cannot be
treated as ordinary drift or used as a new baseline by `--check`.

Snapshot format admission requires the exact versioned Terlan schema and
PostgreSQL product identity. Unsupported versions or database products fail with
`error[db.snapshot.unsupported_contract]` before fingerprint or drift checks.

## Testing Notes

- `migration_test.rs` covers pure migration parsing and status comparison.
- `history_test.rs` covers native-boundary row conversion into applied history.
- `execution_test.rs` covers executor request/report, SQL batch shaping,
  advisory-lock protocol validation, and post-lock history revalidation.
- `snapshot_test.rs` covers deterministic fingerprints, round trips,
  corruption, drift, and default path selection.
- `mod_test.rs` covers CLI argument parsing, command routing, destructive
  guards, and unreachable-database behavior.
- `live_test.rs` covers the configured and Docker-backed Postgres lifecycle,
  including migration contention, command-wide rollback, lock release, and
  snapshot drift.
- `make db-command-check` runs the DB command gate.
- `make vm-db-migration-command-check` composes that runtime gate with the
  deterministic, redacted `target/quality/vm-db-migration-report.json`
  evidence contract before SQL macro validation.
- `make native-boundary-postgres-docker-check` runs live Postgres adapter
  validation when a test database URL is configured.
