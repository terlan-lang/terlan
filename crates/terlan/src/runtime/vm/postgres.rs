#[path = "postgres/batch.rs"]
mod batch;
#[path = "postgres/inspection.rs"]
mod inspection;
#[path = "postgres/libpq_worker.rs"]
pub(crate) mod libpq_worker;
#[path = "postgres/report.rs"]
mod report;
#[path = "postgres/state.rs"]
mod state;
#[path = "postgres/types.rs"]
mod types;
#[cfg(test)]
#[path = "postgres/worker_fixture_test.rs"]
mod worker;

#[cfg(test)]
#[path = "postgres_test.rs"]
mod postgres_test;

#[cfg(test)]
#[path = "postgres_report_test.rs"]
mod postgres_report_test;

#[cfg(test)]
#[path = "postgres_inspection_test.rs"]
mod postgres_inspection_test;

#[cfg(test)]
#[path = "actor_postgres_test.rs"]
mod actor_postgres_test;
include!("postgres_part_001.rs");
include!("postgres_part_002.rs");
