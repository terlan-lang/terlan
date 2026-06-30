# Terlan 0.0.7 Roadmap

This roadmap captures planned work for the 0.0.7 development line. It is an
execution document, not a public release promise.

## Primary Theme

0.0.7 should make Terlan feel like one coherent application platform:
compiler, AngularTS web UI, reactive UI processes, native/mobile shell,
SafeNative, and Terlan VM instrumentation should share typed boundaries instead
of growing as separate stacks.

## Terlan Mobile Shell

Goal: provide a Strada/Hotwire-style mobile shell that lets Terlan and
AngularTS applications run on iOS and Android while upgrading selected routes,
widgets, and platform capabilities to native shell behavior.

This is preferred over a Flutter-first target for the near term because it
reuses AngularTS and the Terlan process/message model instead of requiring a
second UI backend.

Reference shape:

- Android shell modeled after the Hotwire Native/AngularTS demo:
  `https://github.com/angular-wave/angular.ts/tree/native/hotwire-native-android`
- Future iOS shell with the same bridge and route concepts.
- AngularTS remains the primary UI rendering layer.
- Native shell owns navigation chrome, fragments/screens, platform widgets,
  native permissions, and platform services.
- Terlan compiler owns typed declarations, bridge code generation, and reactive
  UI process wiring.

### Architecture Contract

Reactive UI processes must be able to wire directly into the mobile shell:

```text
Terlan reactive UI process
  -> AngularTS component/state binding
  -> Angular Native bridge
  -> native shell component/navigation action
  -> native event reply
  -> Terlan process message/update
```

Hard rules:

- UI state is owned by Terlan processes created through ordinary constructors
  and configuration values.
- AngularTS renders process state and sends typed UI events.
- Native shell performs platform actions only through declared bridge
  capabilities.
- Native replies become typed Terlan process messages.
- Raw JSON is allowed only as an early transport detail; the product contract
  is typed bridge declarations.
- Native shell code must not own Terlan application semantics.
- Terlan mobile shell must not require Flutter or a Dart backend.
- Do not introduce `actor` or `on` language keywords for this work. Reactive UI
  behavior must use existing module/function syntax plus typed constructors and
  message-handler declarations.

### Compiler Work

- [ ] Add a `mobile` profile to `terlc init`.
- [ ] Add target profile planning for `mobile.shell`, `mobile.android`, and
  `mobile.ios`.
- [ ] Add a typed mobile bridge declaration model.
- [ ] Generate bridge metadata for navigation, native components, permissions,
  files, camera, geolocation, storage, push notifications, and platform
  environment.
- [ ] Generate route/native presentation configuration from Terlan/AngularTS
  route declarations.
- [ ] Generate source-to-bridge debug identity so native replies can map back
  to Terlan module/function/source span.
- [ ] Validate bridge declarations during typechecking.
- [ ] Reject undeclared native capability use.
- [ ] Add tests for malformed bridge declarations, duplicate bridge names,
  arity/type mismatches, missing permissions, stale generated metadata, and
  source identity mismatches.

### AngularTS Work

- [ ] Define standard mobile widget declarations for native-upgradable
  components.
- [ ] Add AngularTS bridge runtime helpers for sending typed commands and
  receiving typed native events.
- [ ] Expose platform/theme environment values from the native shell to
  AngularTS as typed environment data and CSS variables.
- [ ] Support native route presentation hints such as default, modal,
  bottom-sheet, clear-all, replace, restore, and native-fragment upgrade.
- [ ] Add tests for bridge command encoding, native event dispatch, component
  mount/update/unmount, route presentation hints, and missing native shell
  fallback.

### Reactive UI Process Work

- [ ] Define a canonical reactive UI process contract for state, event, effect,
  and reply handling.
- [ ] Wire process state updates into AngularTS component bindings.
- [ ] Wire AngularTS events into Terlan process messages.
- [ ] Wire native bridge replies into Terlan process messages.
- [ ] Add typed effect helpers for navigation, native component commands,
  platform permissions, and SafeNative/native resources.
- [ ] Ensure process-driven UI remains deterministic in tests when native bridge
  replies are supplied as fixtures.
- [ ] Add full-cycle tests: process state -> AngularTS render -> native command
  -> native reply -> process update.

### Android Shell Work

- [ ] Vendor or template a minimal Android shell structure with reusable core,
  navigation, and demo/app modules.
- [ ] Support a generated path configuration file.
- [ ] Support native fragment route upgrades.
- [ ] Support native bottom sheet/modal route presentation.
- [ ] Support standard bridge components: toolbar action, bottom sheet menu,
  drawer, card, image, file picker, camera, and geolocation permission.
- [ ] Inject platform/theme environment into the WebView.
- [ ] Add bridge protocol tests for invalid JSON, unknown target, unknown
  method, missing ids, duplicate component ids, stale mounted components, and
  native reply delivery.

### iOS Shell Work

- [ ] Define the Swift shell layout matching the Android route/bridge model.
- [ ] Support route presentation, native screen upgrades, and bridge
  components with the same typed declarations.
- [ ] Keep iOS-specific behavior behind typed platform capability declarations.
- [ ] Add parity tests or generated fixtures to ensure Android and iOS consume
  the same bridge manifest.

### Build And Packaging

- [ ] Add `terlc build --target mobile.android` planning.
- [ ] Add `terlc build --target mobile.ios` planning.
- [ ] Package AngularTS web output into the shell build inputs.
- [ ] Emit route configuration, bridge manifest, native shell config, and
  source identity metadata.
- [ ] Keep mobile shell generation separate from ordinary web builds.
- [ ] Add smoke tests for generated mobile project layout.

### Safety And Capability Model

- [ ] Treat native mobile capabilities like SafeNative-style typed resources.
- [ ] Require explicit capability declarations for native services.
- [ ] Redact secrets and sensitive config from bridge inspection output.
- [ ] Map native errors into typed Terlan errors.
- [ ] Support cancellation and timeout behavior for async native calls.
- [ ] Add adversarial tests for denied permissions, stale handles, duplicate
  requests, late replies, malformed native payloads, and native-shell restart.

### Gates

Planned gates:

```bash
make mobile-shell-profile-check
make mobile-bridge-typecheck
make mobile-bridge-runtime-check
make mobile-reactive-process-check
make mobile-android-shell-smoke
make mobile-ios-shell-smoke
```

Stop condition: do not expose mobile shell as a stable public target until a
full-cycle reactive UI process test can drive AngularTS, issue a typed native
bridge command, receive a typed native event, and update process state without
raw application JSON in the public API.

## Terlan VM Instrumentation UI

Goal: use the same typed inspection/control philosophy for local VM and cloud
operator TUIs.

- [ ] Keep local VM TUI independent from Terlan Cloud.
- [ ] Use Ratatui for local terminal dashboards.
- [ ] Provide local and cloud providers over shared UI components.
- [ ] Keep v1 read-only.
- [ ] Add guarded operator mode later for hot reload, deploy, rollback, node
  drain, service restart, and replica promotion.

## Atom Exhaustion Safety

Goal: make atom exhaustion impossible in ordinary Terlan code by treating atoms
as finite compiler-known symbols, not runtime-created names.

Hard rules:

- Terlan source examples must use `Atom["name"]` or typed constructors for
  symbolic values. Do not introduce colon atom syntax such as `:name`.
- `Atom["name"]` is a source-level singleton symbol. Its payload must be a
  compile-time literal that is recorded in a compiler-emitted atom manifest.
- Runtime `String` to `Atom` creation is forbidden for ordinary Terlan code.
- Decoders for JSON, TOML, YAML, HTTP headers, query strings, forms, database
  rows, SafeNative payloads, and mobile/native bridge messages must keep
  dynamic keys and values as `String` or map them into finite typed values.
- Dynamic text may map to atoms only through an explicit finite table that
  returns `Result`, for example by matching `"ready"` to a declared
  `Atom["ready"]` alias and rejecting unknown input.
- Generated Erlang must not call unbounded atom-creating functions such as
  `binary_to_atom` or `list_to_atom` on runtime input.
- Existing-atom lookup may exist only as an explicit checked API returning
  `Result`; it must not create new atoms.
- Terlan VM targets must reject runtime atom creation outside verified module
  loading and compiler-emitted atom manifests.

Compiler work:

- [ ] Add an atom-safety validation pass over CoreIR and backend lowering.
- [ ] Emit per-package atom manifests from `Atom["name"]` aliases and compiler
  generated symbolic values.
- [ ] Reject dynamic atom construction in source and generated backend calls.
- [ ] Add tests for JSON/object key decoding, HTTP input, database enum/text
  mapping, SafeNative/mobile bridge payloads, and generated Erlang output.
- [ ] Add a gate that scans generated Erlang for unsafe runtime atom creation.

Planned gates:

```bash
make atom-safety-check
make erlang-atom-safety-check
```

## Roadmap Maintenance

- [ ] Inventory this roadmap after each 0.0.7 execution slice.
- [ ] Move completed items into release notes only when implemented and gated.
- [ ] Keep experimental or hidden features out of user-visible help until they
  are intentionally exposed.
