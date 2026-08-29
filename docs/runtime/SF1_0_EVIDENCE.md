# SF1.0 Evidence — Standard Terlan Web Service Foundation

Status: implemented and validated on 2026-08-15.

The aggregate command is:

```bash
make web-service-foundation-check
```

It proves:

- the checked-in `std.log` and `std.service` source APIs compile from the
  compiler's embedded interfaces;
- structured values, secret rejection, metric declarations/cardinality, W3C
  context, actor propagation, bounded drain, sink loss, and sink-failure
  containment pass the portable contract suite;
- `terlc serve` installs implicit request, connection, route, handler, source,
  release, actor, and trace identity around VM handler output;
- disabled, local, bounded-memory, and Foundations adapters consume the same
  ordered semantic corpus;
- Foundations 5.9.2 is exact-pinned with `default-features = false`, and only
  `logging`, `metrics`, and `testing` enabled; tracing and OTLP remain excluded
  until their upstream OpenTelemetry dependency closure is patched;
- the portable contract and normal Terlan runtime dependency closures exclude
  the optional Foundations adapter and its telemetry stack;
- the complete dependency, feature, platform, security, lifecycle, semantic,
  pruning, and source-hash inventory is persisted in
  `web-service-foundation-report.json`.

The follow-up standard-library inventory gates are:

```bash
make stdlib-summary-inventory-check \
  stdlib-summary-drift-check \
  stdlib-embedded-interface-contract-check
```

Cloud TC1.2 may consume SF1.0. The adapter does not authorize a second server
lifecycle: Pingora remains the sole gateway listener and lifecycle owner.
