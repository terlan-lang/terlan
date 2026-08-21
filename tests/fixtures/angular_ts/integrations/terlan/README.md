# AngularTS Terlan Integration

This package is the Terlan facade for authoring AngularTS applications.

The integration boundary is explicit:

- `src/terlan/angular/Todo.terl` is generated Terlan source.
- `src/terlan/angular/TodoSummary.terl.html` proves typed template slots are
  checked and emitted with the app module.
- `@angular-wave/angular.ts` is the default browser runtime for generated
  client-side behavior. Do not hand-roll DOM/SSE client libraries in the
  Terlan integration.
- HTTP/1.x live template updates use AngularTS `$sse`/`createSseService` over
  browser `EventSource`. HTTP/2 and HTTP/3 may use stream-native transports
  when those runtime lanes exist.
- `terlan.integration.json` declares the integration source, commands,
  artifacts, tests, examples, and import boundaries.
- `ROOT_MAKEFILE_HOOKS.md` documents the root Angular.ts Makefile lines needed
  to wire the integration into `generated-check` and `test-integrations`.
- `tool/generate_terlan_todo.mjs` owns Terlan source, app metadata, template,
  and deterministic Angular.ts adapter generation and freshness checks.
- `tool/generate_ng_namespace_manifest.mjs` pins `@types/namespace.d.ts` for
  Oxc-backed Terlan type generation.
- `tool/check_ng_namespace_bindings.mjs` verifies generated `ng` namespace
  aliases are materialized as Terlan types.
- `make build` compiles the source to `js.shared` ES modules.
- `make generate` refreshes the generated Terlan source.
- `make generate-check` verifies the generated Terlan source is fresh.
- `make artifact-check` verifies the generated JavaScript artifacts.
- `make test` runs the generated JavaScript through Node.
- `make harness-check` verifies the AngularTS-facing todo harness imports the
  generated Terlan boundary.
- `make app-ownership-check` executes every Todo transition from generated
  Terlan, mounts the generated app in a real browser when Angular.ts runtime
  dependencies are present, and rejects JavaScript-owned application behavior.
- `make browser-test` runs the create, toggle, edit, filter, and delete flows
  through the current Angular.ts runtime and the generated Terlan module.
- `make namespace-check` validates generated Terlan types from the real
  AngularTS `@types/namespace.d.ts` declaration file.
- `make check` runs the generated-artifact and runtime smoke checks.
- `make run` aliases the fast generated-module test command.
- `make clean` removes generated artifacts.

The todo example covers create, toggle, edit, delete, active/completed
filtering, empty/list rendering behavior, and an AngularTS HTML/JavaScript
harness under `examples/todo`.

Apply from the Terlan repository:

```bash
TERLAN_REPOSITORY_ROOT="$PWD" target/debug/terlc run scripts/self_validation/AngularTsIntegrationTest.terl -- --print-root-makefile-patch /path/to/angular.ts
TERLAN_REPOSITORY_ROOT="$PWD" target/debug/terlc run scripts/self_validation/AngularTsIntegrationTest.terl -- --print-application-patch /path/to/angular.ts > /tmp/angular-terlan.patch
TERLAN_REPOSITORY_ROOT="$PWD" target/debug/terlc run scripts/self_validation/AngularTsIntegrationTest.terl -- --materialize /path/to/angular.ts --patch-root-makefile
TERLAN_REPOSITORY_ROOT="$PWD" target/debug/terlc run scripts/self_validation/AngularTsIntegrationTest.terl -- --check-materialized /path/to/angular.ts
```
