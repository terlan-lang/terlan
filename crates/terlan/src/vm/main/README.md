# VM CLI Main Internals

This directory owns focused output and reporting helpers for the `terlan-vm`
binary entry point.

## Responsibilities

- Render stable machine-readable benchmark and diagnostic reports.
- Admit and execute self-describing `.tvm` images without a JSON sidecar.
- Keep command execution separate from presentation details.
- Preserve schema fields and units consumed by quality gates and benchmarks.

## Testing Notes

Run VM CLI parsing, report-schema, benchmark, and warnings-as-errors checks.
