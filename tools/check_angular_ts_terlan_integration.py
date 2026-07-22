#!/usr/bin/env python3
"""Validate the Terlan side of the Angular.ts integration contract.

Inputs:
- The current Terlan compiler from `TERLC`, or `cargo run -p terlan --bin terlc`.
- Optional `ANGULAR_TS_ROOT` pointing at the external Angular.ts checkout.
- Optional `--materialize ANGULAR_TS_ROOT` to write the generated
  `integrations/terlan` package under a chosen Angular.ts root.
- Optional `--force` with `--materialize` to overwrite an existing
  `integrations/terlan` package.
- Optional `--check-materialized ANGULAR_TS_ROOT` to validate an existing
  `integrations/terlan` package under a chosen Angular.ts root.
- Optional `--namespace-generation-check` to validate only the generated
  namespace manifests and Terlan binding outputs for the JS and Wasm Terlan
  integration packages.
- Optional `--facade-parity-check` to validate generated Angular.ts facade
  modules, namespace bindings, skip manifests, and handwritten wrapper tests.
- Optional `--facade-parity-hardening-check` to compile the generated facade,
  exercise drift/hash recovery, and run family-by-family adversarial checks.
- Optional `--app-ownership-check` to validate that generated Terlan source
  owns Angular.ts app metadata and behavior while JS remains an adapter.
- Optional `--patch-root-makefile` with `--materialize` or
  `--check-materialized` to insert the required root Makefile hooks.
- Optional `--print-root-makefile-patch ANGULAR_TS_ROOT` to print a unified
  diff for the required root Makefile hooks without writing files.
- Optional `--print-application-patch ANGULAR_TS_ROOT` to print a unified diff
  containing the root Makefile hook changes and the generated
  `integrations/terlan` package without writing files.

Outputs:
- Exit status 0 when a generated `integrations/terlan` package builds a
  Terlan todo module to runnable JavaScript and its emitted ES module behaves
  as the Angular.ts bridge expects.
- Exit status 0 when `--materialize` writes the integration package layout.
- Exit status 0 when `--check-materialized` validates an existing integration
  package layout and package-local `make check`.
- Exit status 0 when `--facade-parity-check` validates that generated
  Angular.ts wrappers are executable Terlan facade modules rather than
  unresolved TypeScript aliases or marker-only stubs.
- Exit status 1 with stable diagnostics when generation, build, manifest
  emission, or JavaScript execution regresses.

Transformation:
- Creates a temporary Angular.ts-style `integrations/terlan` package with
  README, Makefile, source, and test files, runs its package-local
  `make check`, validates the JS manifest and emitted module path, then
  imports the generated module with Node and executes the todo boundary
  functions.
- When the external Angular.ts checkout is visible, also materializes the
  Terlan package into a writable temp root seeded with the real Angular.ts
  root Makefile, patches that Makefile, and validates the materialized package
  there. The real checkout is never written by the default gate.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import io
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from angular_ts_terlan_app import (
    TODO_APP_MANIFEST,
    TODO_APP_MANIFEST_TEXT,
    TODO_BROWSER_TEST,
    TODO_BOUNDARY_TEST,
    TODO_HARNESS_CONTRACT_TEST,
    TODO_HARNESS_HTML,
    TODO_HARNESS_JS,
    TODO_PLAYWRIGHT_CONFIG,
    TODO_SOURCE,
    TODO_TYPED_TEMPLATE,
)


ROOT = Path(__file__).resolve().parents[1]
README = """# AngularTS Terlan Integration

This package is the Terlan facade for authoring AngularTS applications.

The integration boundary is explicit:

- `src/terlan/angular/Todo.terl` is generated Terlan source.
- `src/terlan/angular/TodoSummary.terl.html` proves typed template slots are
  checked and emitted with the app module.
- `angular-wave/angular.ts` is the default browser runtime for generated
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
python3 -B tools/check_angular_ts_terlan_integration.py --print-root-makefile-patch /path/to/angular.ts
python3 -B tools/check_angular_ts_terlan_integration.py --print-application-patch /path/to/angular.ts > /tmp/angular-terlan.patch
python3 -B tools/check_angular_ts_terlan_integration.py --materialize /path/to/angular.ts --patch-root-makefile
python3 -B tools/check_angular_ts_terlan_integration.py --check-materialized /path/to/angular.ts
```
"""
WASM_TERLAN_SOURCE = """module terlan.angular.wasm.App.

pub title(): String ->
    "Terlan Wasm Angular App".

pub remaining(open: Int, completed: Int): Int ->
    open - completed.

pub status(open: Int, completed: Int): String ->
    if {
        remaining(open, completed) == 0 -> "complete";
        true -> "active"
    }.
"""
WASM_TERLAN_TOML = """[package]
name = "angular-ts-terlan-wasm"
version = "0.1.0"
namespace = "terlan.angular.wasm"

[build]
source_roots = ["src"]
artifact = "wasm-browser"

[target.wasm]
profile = "browser"
exports = ["terlan.angular.wasm.App.title", "terlan.angular.wasm.App.remaining", "terlan.angular.wasm.App.status"]
bridge = "generated-js"
capabilities = ["browser.console", "browser.scope"]
validation_engine = "browser-playwright"
"""
WASM_TERLAN_MANIFEST = {
    "name": "terlan-wasm",
    "source": "src/terlan/angular/wasm/App.terl",
    "project": "terlan.toml",
    "generator": "tool/generate_terlan_wasm_app.mjs",
    "commands": {
        "generate": "make generate",
        "generate_check": "make generate-check",
        "manifest_check": "make manifest-check",
        "namespace_check": "make namespace-check",
        "reserved_build_check": "make reserved-build-check",
        "check": "make check",
        "clean": "make clean",
    },
    "artifacts": [
        "build/ng-namespace/std/js/manifests/std_js_bindings.json",
        "build/ng-namespace/std/js/manifests/std_js_skipped.json",
    ],
    "tests": [
        "tests/wasm_app_contract.test.mjs",
        "tool/check_ng_namespace_parity.mjs",
    ],
    "examples": [
        "examples/basic_app/index.html",
        "examples/basic_app/angular-ts.json",
    ],
    "typescript_declaration_inputs": [
        "@types/namespace.d.ts",
    ],
    "status": "reserved-backend-integration",
}
WASM_TERLAN_README = """# AngularTS Terlan Wasm Integration

This package is the Terlan/Wasm integration boundary for AngularTS.

The Rust integration remains the reference implementation for the live browser
Wasm bridge. This package gives Terlan the same external integration shape:

- `src/terlan/angular/wasm/App.terl` is the Terlan-authored app boundary.
- `terlan.toml` declares the browser Wasm artifact, exports, bridge, and
  capabilities.
- `examples/basic_app/angular-ts.json` mirrors the Rust integration's app
  manifest shape.
- `tool/generate_ng_namespace_manifest.mjs` pins the real
  `@types/namespace.d.ts` input.
- `tool/check_ng_namespace_parity.mjs` verifies every direct `ng` namespace
  type is either generated as Terlan or explicitly skipped.
- `make reserved-build-check` proves `terlc build --target wasm.browser` still
  stops at the reserved backend boundary until Terlan CoreIR-to-Wasm lowering
  is implemented.

The current executable contract is app definition, namespace generation, and
reserved backend diagnostics. Once the Wasm backend is promoted, this package is
where the real browser Wasm build and Playwright smoke should land.
"""
WASM_APP_INDEX = """<!doctype html>
<html lang="en" ng-app="terlanWasmDemo">
  <head>
    <meta charset="utf-8">
    <title>Terlan Wasm Angular Demo</title>
  </head>
  <body>
    <main>
      <h3>Terlan-authored AngularTS Wasm App</h3>
      <p id="terlan-wasm-status">reserved backend integration</p>
    </main>
  </body>
</html>
"""
WASM_APP_ANGULAR_TS_JSON = """{
  "module": "terlanWasmDemo",
  "package": "../pkg/terlan_angular_wasm_app.js",
  "appTemplatePath": "index.html",
  "registrations": [
    {
      "kind": "component",
      "name": "terlanWasmApp",
      "templatePath": "templates/terlan-wasm-app.html"
    }
  ]
}
"""
WASM_APP_CONTRACT_TEST = """import fs from "node:fs";

const source = fs.readFileSync("src/terlan/angular/wasm/App.terl", "utf8");
const project = fs.readFileSync("terlan.toml", "utf8");
const manifest = JSON.parse(fs.readFileSync("examples/basic_app/angular-ts.json", "utf8"));
const packageManifest = JSON.parse(fs.readFileSync("terlan.wasm.integration.json", "utf8"));

const sourceMarkers = [
  "module terlan.angular.wasm.App.",
  "pub title(): String ->",
  "pub remaining(open: Int, completed: Int): Int ->",
  "pub status(open: Int, completed: Int): String ->",
];

const projectMarkers = [
  'artifact = "wasm-browser"',
  '[target.wasm]',
  'profile = "browser"',
  'bridge = "generated-js"',
  'browser.scope',
];

for (const marker of sourceMarkers) {
  if (!source.includes(marker)) {
    throw new Error(`missing Terlan Wasm app source marker: ${marker}`);
  }
}

for (const marker of projectMarkers) {
  if (!project.includes(marker)) {
    throw new Error(`missing Terlan Wasm project marker: ${marker}`);
  }
}

if (manifest.module !== "terlanWasmDemo") {
  throw new Error("unexpected Terlan Wasm AngularTS module name");
}
if (!manifest.registrations.some((entry) => entry.kind === "component" && entry.name === "terlanWasmApp")) {
  throw new Error("missing Terlan Wasm component registration");
}
if (packageManifest.status !== "reserved-backend-integration") {
  throw new Error("Terlan Wasm integration status must remain explicit");
}
"""
ANGULAR_NAMESPACE_FIXTURE = """import type { Angular as TAngular } from "./angular.ts";

declare global {
  export namespace ng {
    type Angular = TAngular;
    type NgModule = { name: string };
    type Component = { template: string };
    type Directive<TController = unknown> = { controller?: TController };
    type Scope = { $id: number };
    type HttpService = string;
    type HttpResponse<T> = { data: T; status: number };
    type SseConfig = TSseConfig;
    type SseConnection = TSseConnection;
    type SseService = TSseService;
    type RealtimeProtocolEventDetail<T = unknown, TSource = unknown> = { data: T; source: TSource };
    type RealtimeProtocolMessage = { type: string; data?: unknown };
    type TemplateCacheService = Map<string, string>;
    type Machine<TContract = unknown> = TMachine<TContract>;
    type MachineConfig<TContract = unknown> = TMachineConfig<TContract>;
    type MachineSendResult<TState = string> = TMachineSendResult<TState>;
    type MachineService = TMachineService;
    type MachineSnapshot<TContract = unknown> = TMachineSnapshot<TContract>;
    type Workflow<TContract = unknown> = TWorkflow<TContract>;
    type WorkflowResult<TOutput = unknown> = TWorkflowResult<TOutput>;
    type WorkflowService = TWorkflowService;
    type WorkflowSnapshot<TContract = unknown> = TWorkflowSnapshot<TContract>;
    type WebSocketConfig = TWebSocketConfig;
    type WebSocketConnection = TWebSocketConnection;
    type WebSocketService = TWebSocketService;
    type WorkerConfig<TReceive = unknown> = TWorkerConfig<TReceive>;
    type WorkerHandle<TSend = unknown, TReceive = unknown> = TWorkerHandle<TSend, TReceive>;
    type WorkerService = TWorkerService;
  }
}
"""
NG_NAMESPACE_MANIFEST_GENERATOR = """import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const integrationRoot = process.cwd();
const angularRoot = path.resolve(integrationRoot, "../..");
const sourcePath = path.join(angularRoot, "@types", "namespace.d.ts");
const outputPath = path.join("generated", "ng_namespace_inputs.json");
const version = process.env.TERLAN_BINDGEN_VERSION || "";

if (!version) {
  console.error("TERLAN_BINDGEN_VERSION is required");
  process.exit(1);
}

if (!fs.existsSync(sourcePath)) {
  console.error(`missing AngularTS namespace declaration file: ${sourcePath}`);
  process.exit(1);
}

const source = fs.readFileSync(sourcePath);
const sha256 = crypto.createHash("sha256").update(source).digest("hex");
const manifest = {
  schema: "terlan.std.js.input-manifest.v1",
  generator: {
    name: "terlc",
    version,
    profile: "angular-ts-namespace",
    oxc_parser: true
  },
  target_profile: "js.browser",
  source_package: {
    name: "typescript",
    version: "local",
    resolution: "@types/namespace.d.ts"
  },
  inputs: [
    {
      path: "@types/namespace.d.ts",
      sha256,
      kind: "typescript-declaration",
      namespace: "terlan.angular"
    }
  ]
};

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\\n`);
"""
NG_NAMESPACE_BINDINGS_CHECK = """import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const angularRoot = path.resolve(process.cwd(), "../..");
const namespacePath = path.join(angularRoot, "@types", "namespace.d.ts");
const outputRoot = path.join("build", "ng-namespace");
const manifestPath = path.join(outputRoot, "std", "js", "manifests", "std_js_bindings.json");
const skippedPath = path.join(outputRoot, "std", "js", "manifests", "std_js_skipped.json");
const requiredTypes = [
  "Angular",
  "NgModule",
  "Component",
  "Directive",
  "Scope",
  "HttpService",
  "HttpResponse",
  "TemplateCacheService",
  "Machine",
  "MachineConfig",
  "MachineSendResult",
  "MachineService",
  "MachineSnapshot",
  "Workflow",
  "WorkflowResult",
  "WorkflowService",
  "WorkflowSnapshot",
  "SseConfig",
  "SseConnection",
  "SseService",
  "WebSocketConfig",
  "WebSocketConnection",
  "WebSocketService",
  "WorkerConfig",
  "WorkerHandle",
  "WorkerService"
];
const requiredCoveredTypes = [
  "SseConfig",
  "SseConnection",
  "SseService",
  "RealtimeProtocolEventDetail",
  "RealtimeProtocolMessage"
];
const requiredFacadeMarkers = [
  "module terlan.angular.Ng.",
  "import type std.js.String.{JsString}.",
  "import type terlan.angular.ng.{",
  "pub angular(): terlan.angular.ng.Angular.Angular",
  "pub ng_module(name: JsString): NgModule",
  "pub ng_module_with_dependencies(name: JsString, dependencies: List[JsString]): NgModule",
  "pub register_component(",
  "pub register_directive(target: NgModule",
  "pub register_controller(target: NgModule",
  "pub apply_scope(scope: terlan.angular.ng.Scope.Scope): Unit",
  "pub template_put(",
  "pub template_get(templates: TemplateCacheService",
  "pub template_remove(templates: TemplateCacheService",
  "pub http_get(",
  "pub machine(",
  "pub machine_send(",
  "pub machine_snapshot(",
  "pub workflow(",
  "pub workflow_run(",
  "pub workflow_snapshot(",
  "pub sse_connect(service: SseService",
  "pub sse_connect_with_config(",
  "pub sse_reconnect(connection: SseConnection): Unit",
  "pub sse_close(connection: SseConnection): Unit",
  "pub websocket_connect(service: WebSocketService",
  "pub websocket_connect_with_config(",
  "pub websocket_send(connection: WebSocketConnection",
  "pub websocket_close(connection: WebSocketConnection): Unit",
  "pub worker_start(service: WorkerService",
  "pub worker_start_with_config(",
  "pub worker_on_message(worker: WorkerHandle[Dynamic, Dynamic]",
  "pub worker_on_error(worker: WorkerHandle[Dynamic, Dynamic]",
  "pub worker_terminate(worker: WorkerHandle[Dynamic, Dynamic]): Unit",
  "pub directive_with_link("
];

if (!fs.existsSync(manifestPath)) {
  throw new Error(`missing generated namespace binding manifest: ${manifestPath}`);
}
if (!fs.existsSync(namespacePath)) {
  throw new Error(`missing AngularTS namespace declaration file: ${namespacePath}`);
}
if (!fs.existsSync(skippedPath)) {
  throw new Error(`missing generated skipped manifest: ${skippedPath}`);
}

const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
const skipped = JSON.parse(fs.readFileSync(skippedPath, "utf8"));
const outputEntries = validateBindingManifest(manifest, manifestPath, outputRoot);
const modules = new Set(outputEntries.map((entry) => entry.module));
const skippedEntries = validateSkippedManifest(skipped, skippedPath);
const skippedSources = new Set(skippedEntries.map((entry) => entry.source));
const ngTypes = readNgNamespaceTypes(namespacePath);

for (const typeName of requiredTypes) {
  const moduleName = `terlan.angular.ng.${typeName}`;
  const sourcePath = path.join(outputRoot, "terlan", "angular", "ng", `${typeName}.terl`);
  if (!modules.has(moduleName)) {
    throw new Error(`missing generated ng namespace module: ${moduleName}`);
  }
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`missing generated ng namespace source: ${sourcePath}`);
  }
  const source = fs.readFileSync(sourcePath, "utf8");
  if (!source.includes(`module ${moduleName}.`)) {
    throw new Error(`generated source has wrong module declaration: ${sourcePath}`);
  }
  if (!source.includes(`pub type ${typeName}`)) {
    throw new Error(`generated source has no type alias declaration: ${sourcePath}`);
  }
  const unresolvedAliasPattern = new RegExp(`pub type ${typeName}(?:\\\\[[^\\\\]]+\\\\])? =\\\\s*\\\\n\\\\s*T[A-Z]`);
  if (unresolvedAliasPattern.test(source)) {
    throw new Error(`generated core facade leaks unresolved TypeScript alias: ${sourcePath}`);
  }
}

const facadeModule = "terlan.angular.Ng";
const facadeSourcePath = path.join(outputRoot, "terlan", "angular", "Ng.terl");
const facadeSummaryPath = path.join(outputRoot, "std", "summaries", "terlan.angular.Ng.typi");
const facadeTestPath = path.join(outputRoot, "terlan", "angular", "NgTest.terl");
if (!modules.has(facadeModule)) {
  throw new Error(`missing generated AngularTS facade module: ${facadeModule}`);
}
for (const filePath of [facadeSourcePath, facadeSummaryPath, facadeTestPath]) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`missing generated AngularTS facade artifact: ${filePath}`);
  }
}
const facadeSource = fs.readFileSync(facadeSourcePath, "utf8");
for (const marker of requiredFacadeMarkers) {
  if (!facadeSource.includes(marker)) {
    throw new Error(`generated AngularTS facade missing executable marker: ${marker}`);
  }
}
if (facadeSource.includes("pub opaque type Ng") || facadeSource.includes("pub type Ng")) {
  throw new Error("generated AngularTS facade regressed to type-only output");
}

for (const typeName of requiredCoveredTypes) {
  const source = `terlan.angular.ng.${typeName}`;
  if (!ngTypes.has(typeName)) {
    throw new Error(`missing required AngularTS ng namespace type: ${typeName}`);
  }
  if (!modules.has(source) && !skippedSources.has(source)) {
    throw new Error(`required AngularTS ng namespace type has no generated or skipped entry: ${typeName}`);
  }
}

const missing = [];
for (const typeName of ngTypes) {
  const source = `terlan.angular.ng.${typeName}`;
  if (!modules.has(source) && !skippedSources.has(source)) {
    missing.push(typeName);
  }
}
if (missing.length > 0) {
  throw new Error(`ng namespace types missing generated or skipped entries: ${missing.join(", ")}`);
}

function validateBindingManifest(manifest, manifestPath, outputRoot) {
  if (manifest.schema !== "terlan.std.js.bindings.v1") {
    throw new Error(`generated binding manifest has wrong schema: ${manifestPath}`);
  }
  if (manifest.target_profile !== "js.browser") {
    throw new Error(`generated binding manifest has wrong target profile: ${manifestPath}`);
  }
  if (manifest.skipped_manifest !== "std/js/manifests/std_js_skipped.json") {
    throw new Error(`generated binding manifest has wrong skipped manifest path: ${manifestPath}`);
  }
  if (!Array.isArray(manifest.outputs)) {
    throw new Error(`generated binding manifest must contain outputs array: ${manifestPath}`);
  }
  if (!Array.isArray(manifest.generated_files)) {
    throw new Error(`generated binding manifest must contain generated_files array: ${manifestPath}`);
  }
  const generatedFiles = validateGeneratedFileHashes(manifest.generated_files, outputRoot);
  const modules = new Set();
  const sources = new Set();
  for (const [index, entry] of manifest.outputs.entries()) {
    if (!entry || typeof entry !== "object") {
      throw new Error(`generated binding manifest output ${index} must be an object`);
    }
    for (const field of ["module", "source", "summary", "test"]) {
      if (typeof entry[field] !== "string" || entry[field].length === 0) {
        throw new Error(`generated binding manifest output ${index} missing ${field}`);
      }
    }
    if (modules.has(entry.module)) {
      throw new Error(`generated binding manifest has duplicate module: ${entry.module}`);
    }
    modules.add(entry.module);
    if (sources.has(entry.source)) {
      throw new Error(`generated binding manifest has duplicate source path: ${entry.source}`);
    }
    sources.add(entry.source);
    for (const field of ["source", "summary", "test"]) {
      const filePath = path.join(outputRoot, entry[field]);
      if (!fs.existsSync(filePath)) {
        throw new Error(`generated binding manifest references missing ${field}: ${entry[field]}`);
      }
      if (!generatedFiles.has(entry[field])) {
        throw new Error(`generated binding manifest missing hash for ${field}: ${entry[field]}`);
      }
    }
  }
  return manifest.outputs;
}

function validateGeneratedFileHashes(entries, outputRoot) {
  const files = new Set();
  for (const [index, entry] of entries.entries()) {
    if (!entry || typeof entry !== "object") {
      throw new Error(`generated file hash entry ${index} must be an object`);
    }
    if (typeof entry.path !== "string" || entry.path.length === 0) {
      throw new Error(`generated file hash entry ${index} missing path`);
    }
    if (files.has(entry.path)) {
      throw new Error(`generated binding manifest has duplicate generated file hash: ${entry.path}`);
    }
    if (typeof entry.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(entry.sha256)) {
      throw new Error(`generated file hash entry ${entry.path} has invalid SHA-256`);
    }
    const filePath = path.join(outputRoot, entry.path);
    if (!fs.existsSync(filePath)) {
      throw new Error(`generated file hash references missing file: ${entry.path}`);
    }
    const actual = crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
    if (actual !== entry.sha256) {
      throw new Error(`generated file hash mismatch for ${entry.path}: expected ${entry.sha256}, found ${actual}`);
    }
    files.add(entry.path);
  }
  return files;
}

function validateSkippedManifest(skipped, skippedPath) {
  if (skipped.schema !== "terlan.std.js.skipped-declarations.v1") {
    throw new Error(`generated skipped manifest has wrong schema: ${skippedPath}`);
  }
  if (!Array.isArray(skipped.skipped)) {
    throw new Error(`generated skipped manifest must contain skipped array: ${skippedPath}`);
  }
  const sources = new Set();
  for (const [index, entry] of skipped.skipped.entries()) {
    if (!entry || typeof entry !== "object") {
      throw new Error(`generated skipped manifest entry ${index} must be an object`);
    }
    if (typeof entry.source !== "string" || entry.source.length === 0) {
      throw new Error(`generated skipped manifest entry ${index} missing source`);
    }
    if (sources.has(entry.source)) {
      throw new Error(`generated skipped manifest has duplicate source: ${entry.source}`);
    }
    sources.add(entry.source);
    if (typeof entry.reason !== "string" || !entry.reason.startsWith("ts_bindgen.")) {
      throw new Error(`generated skipped manifest entry ${entry.source} has unstable reason`);
    }
    if (typeof entry.detail !== "string" || entry.detail.length === 0) {
      throw new Error(`generated skipped manifest entry ${entry.source} missing detail`);
    }
  }
  return skipped.skipped;
}

function readNgNamespaceTypes(filePath) {
  const lines = fs.readFileSync(filePath, "utf8").split(/\\r?\\n/);
  const types = new Set();
  let inNamespace = false;
  let depth = 0;
  for (const line of lines) {
    if (!inNamespace && line.includes("export namespace ng")) {
      inNamespace = true;
      depth += braceDelta(line);
      continue;
    }
    if (!inNamespace) {
      continue;
    }
    const match = /^\\s*type\\s+([A-Za-z_$][A-Za-z0-9_$]*)\\b/.exec(line);
    if (match) {
      types.add(match[1]);
    }
    depth += braceDelta(line);
    if (depth <= 0) {
      break;
    }
  }
  return types;
}

function braceDelta(line) {
  let delta = 0;
  for (const char of line) {
    if (char === "{") delta += 1;
    if (char === "}") delta -= 1;
  }
  return delta;
}
"""
WASM_NAMESPACE_MANIFEST_GENERATOR = NG_NAMESPACE_MANIFEST_GENERATOR.replace(
    'const angularRoot = path.resolve(integrationRoot, "../..");',
    'const angularRoot = path.resolve(integrationRoot, "../../..");',
)
WASM_NAMESPACE_BINDINGS_CHECK = NG_NAMESPACE_BINDINGS_CHECK.replace(
    'const angularRoot = path.resolve(process.cwd(), "../..");',
    'const angularRoot = path.resolve(process.cwd(), "../../..");',
)
ROOT_MAKEFILE_HOOKS = """# AngularTS Root Makefile Hooks

Insert these recipe lines into the existing AngularTS root `Makefile` targets
after materializing `integrations/terlan`. Keep any existing prerequisites such
as `generated-check: types` or `test-integrations: ensure-deps`.

```make
generated-check:
\t@$(MAKE) -C integrations/terlan generate-check
\t@$(MAKE) -C integrations/wasm/terlan generate-check

test-integrations:
\t@$(MAKE) -C integrations/terlan check
\t@$(MAKE) -C integrations/wasm/terlan check
```

These hooks keep Terlan generation freshness and the runnable todo harness in
the same root gates as the other AngularTS integrations.
"""
INTEGRATION_MANIFEST = {
    "name": "terlan",
    "source": "src/terlan/angular/Todo.terl",
    "generator": "tool/generate_terlan_todo.mjs",
    "commands": {
        "build": "make build",
        "generate": "make generate",
        "generate_check": "make generate-check",
        "artifact_check": "make artifact-check",
        "test": "make test",
        "harness_check": "make harness-check",
        "app_ownership_check": "make app-ownership-check",
        "browser_test": "make browser-test",
        "namespace_check": "make namespace-check",
        "check": "make check",
        "run": "make run",
        "clean": "make clean",
    },
    "artifacts": [
        "build/js/manifest.json",
        "build/js/modules/terlan/angular/Todo.js",
        "examples/todo/angular-ts.json",
        "src/terlan/angular/TodoSummary.terl.html",
    ],
    "tests": [
        "tests/todo_boundary.test.mjs",
        "tests/todo_harness_contract.test.mjs",
        "terlan.test.ts",
        "tool/check_ng_namespace_bindings.mjs",
    ],
    "examples": [
        "examples/todo/index.html",
        "examples/todo/todo.js",
        "examples/todo/angular-ts.json",
        "src/terlan/angular/TodoSummary.terl.html",
    ],
    "angular_ts_imports": [
        "../../../../dist/index.js",
    ],
    "terlan_js_imports": [
        "../../build/js/modules/terlan/angular/Todo.js",
    ],
    "typescript_declaration_inputs": [
        "@types/namespace.d.ts",
    ],
    "client_runtime": {
        "package": "@angular-wave/angular.ts",
        "source_checkout": "ANGULAR_TS_ROOT",
        "default": True,
        "handrolled_browser_runtime": False,
    },
    "app_ownership": TODO_APP_MANIFEST,
    "live_update_transport": {
        "http1": "sse",
        "http2": "stream-native",
        "http3": "stream-native",
        "angular_ts_service": "$sse",
        "factory": "createSseService",
        "browser_api": "EventSource",
    },
    "root_makefile_hooks": [
        "generated-check: @$(MAKE) -C integrations/terlan generate-check",
        "test-integrations: @$(MAKE) -C integrations/terlan check",
    ],
}


def terlc_make_command() -> str:
    """Return the compiler command embedded in the generated Makefile."""

    if terlc := os.environ.get("TERLC"):
        return terlc
    return f"cargo run --manifest-path {ROOT / 'Cargo.toml'} -p terlan --bin terlc --"


def run_checked(command: list[str], cwd: Path) -> None:
    """Run one command and render captured output when it fails."""

    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode == 0:
        return
    print(
        f"angular-ts Terlan integration failed: command exited {result.returncode}: {' '.join(command)}",
        file=sys.stderr,
    )
    if result.stdout:
        print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
    if result.stderr:
        print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
    sys.exit(result.returncode)


def integration_make_command(
    integration: Path,
    target: str,
    *,
    browser: bool = False,
) -> list[str]:
    """Return a package-local make command using the current Terlan compiler."""

    command = [
        "make",
        "-C",
        str(integration),
        target,
        f"TERLC={terlc_make_command()}",
    ]
    if not browser:
        command.append("APP_BROWSER_TARGET=")
    return command


def run_expected_failure(command: list[str], cwd: Path, expected: str) -> None:
    """Run one command that must fail with a stable diagnostic fragment."""

    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode == 0:
        print(
            f"angular-ts Terlan integration failed: command unexpectedly succeeded: {' '.join(command)}",
            file=sys.stderr,
        )
        sys.exit(1)
    output = f"{result.stdout}\n{result.stderr}"
    if expected not in output:
        print(
            f"angular-ts Terlan integration failed: expected diagnostic fragment {expected!r}",
            file=sys.stderr,
        )
        if result.stdout:
            print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
        if result.stderr:
            print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
        sys.exit(1)


def write_integration_layout(workspace: Path) -> Path:
    """Write a temporary Angular.ts-style Terlan integration package."""

    integration = workspace / "integrations" / "terlan"
    source = integration / "src" / "terlan" / "angular" / "Todo.terl"
    typed_template = integration / "src" / "terlan" / "angular" / "TodoSummary.terl.html"
    test = integration / "tests" / "todo_boundary.test.mjs"
    harness_test = integration / "tests" / "todo_harness_contract.test.mjs"
    harness_html = integration / "examples" / "todo" / "index.html"
    harness_js = integration / "examples" / "todo" / "todo.js"
    app_manifest = integration / "examples" / "todo" / "angular-ts.json"
    playwright_config = integration / "playwright.config.ts"
    browser_test = integration / "terlan.test.ts"
    generator = integration / "tool" / "generate_terlan_todo.mjs"
    namespace_manifest_generator = integration / "tool" / "generate_ng_namespace_manifest.mjs"
    namespace_bindings_check = integration / "tool" / "check_ng_namespace_bindings.mjs"
    source.parent.mkdir(parents=True, exist_ok=True)
    test.parent.mkdir(parents=True, exist_ok=True)
    harness_html.parent.mkdir(parents=True, exist_ok=True)
    generator.parent.mkdir(parents=True, exist_ok=True)
    ensure_namespace_fixture(workspace)
    source.write_text(TODO_SOURCE, encoding="utf-8")
    typed_template.write_text(TODO_TYPED_TEMPLATE, encoding="utf-8")
    test.write_text(TODO_BOUNDARY_TEST, encoding="utf-8")
    harness_test.write_text(TODO_HARNESS_CONTRACT_TEST, encoding="utf-8")
    harness_html.write_text(TODO_HARNESS_HTML, encoding="utf-8")
    harness_js.write_text(TODO_HARNESS_JS, encoding="utf-8")
    app_manifest.write_text(TODO_APP_MANIFEST_TEXT, encoding="utf-8")
    playwright_config.write_text(TODO_PLAYWRIGHT_CONFIG, encoding="utf-8")
    browser_test.write_text(TODO_BROWSER_TEST, encoding="utf-8")
    namespace_manifest_generator.write_text(NG_NAMESPACE_MANIFEST_GENERATOR, encoding="utf-8")
    namespace_bindings_check.write_text(NG_NAMESPACE_BINDINGS_CHECK, encoding="utf-8")
    (integration / "ROOT_MAKEFILE_HOOKS.md").write_text(ROOT_MAKEFILE_HOOKS, encoding="utf-8")
    (integration / "terlan.integration.json").write_text(
        json.dumps(INTEGRATION_MANIFEST, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    generator.write_text(
        f"""import fs from "node:fs";
import path from "node:path";

const generated = new Map([
  [path.join("src", "terlan", "angular", "Todo.terl"), {json.dumps(TODO_SOURCE)}],
  [path.join("src", "terlan", "angular", "TodoSummary.terl.html"), {json.dumps(TODO_TYPED_TEMPLATE)}],
  [path.join("examples", "todo", "index.html"), {json.dumps(TODO_HARNESS_HTML)}],
  [path.join("examples", "todo", "todo.js"), {json.dumps(TODO_HARNESS_JS)}],
  [path.join("examples", "todo", "angular-ts.json"), {json.dumps(TODO_APP_MANIFEST_TEXT)}],
  ["playwright.config.ts", {json.dumps(TODO_PLAYWRIGHT_CONFIG)}],
  ["terlan.test.ts", {json.dumps(TODO_BROWSER_TEST)}],
]);
const check = process.argv.includes("--check");

if (check) {{
  for (const [generatedPath, expected] of generated) {{
    const actual = fs.existsSync(generatedPath) ? fs.readFileSync(generatedPath, "utf8") : "";
    if (actual !== expected) {{
      console.error(`${{generatedPath}} is stale; run make generate`);
      process.exit(1);
    }}
  }}
  process.exit(0);
}}

for (const [generatedPath, expected] of generated) {{
  fs.mkdirSync(path.dirname(generatedPath), {{ recursive: true }});
  fs.writeFileSync(generatedPath, expected);
}}
""",
        encoding="utf-8",
    )
    (integration / "README.md").write_text(README, encoding="utf-8")
    (integration / "Makefile").write_text(
        """TERLC ?= terlc
NODE ?= node
PLAYWRIGHT ?= ../../node_modules/.bin/playwright
BUILD_DIR ?= build
APP_BROWSER_TARGET ?= browser-test

.PHONY: all build generate generate-check artifact-check test harness-check browser-test app-ownership-check check run clean

all: check

build:
\t$(TERLC) build src --target js.shared --out-dir $(BUILD_DIR)

generate:
\t$(NODE) tool/generate_terlan_todo.mjs

generate-check:
\t$(NODE) tool/generate_terlan_todo.mjs --check

artifact-check: build
\ttest -f $(BUILD_DIR)/js/manifest.json
\ttest -f $(BUILD_DIR)/js/modules/terlan/angular/Todo.js

namespace-manifest:
\tTERLAN_BINDGEN_VERSION="$$( $(TERLC) --version | tail -n 1 | awk '{print $$2}' )" $(NODE) tool/generate_ng_namespace_manifest.mjs

namespace-check: namespace-manifest
\trm -rf $(BUILD_DIR)/ng-namespace
\tcd ../.. && $(TERLC) bind js-dom --manifest integrations/terlan/generated/ng_namespace_inputs.json --out integrations/terlan/$(BUILD_DIR)/ng-namespace
\t$(NODE) tool/check_ng_namespace_bindings.mjs

test: build
\t$(NODE) tests/todo_boundary.test.mjs

harness-check: build
\t$(NODE) tests/todo_harness_contract.test.mjs

browser-test: build
\t$(PLAYWRIGHT) test --config playwright.config.ts

app-ownership-check: generate-check build $(APP_BROWSER_TARGET)
\t$(NODE) tests/todo_boundary.test.mjs
\t$(NODE) tests/todo_harness_contract.test.mjs

check: generate generate-check artifact-check app-ownership-check namespace-check

run: test

clean:
\trm -rf $(BUILD_DIR)
""",
        encoding="utf-8",
    )
    return integration


def write_wasm_integration_layout(workspace: Path) -> Path:
    """Write the Angular.ts Wasm Terlan integration package."""

    integration = workspace / "integrations" / "wasm" / "terlan"
    source = integration / "src" / "terlan" / "angular" / "wasm" / "App.terl"
    project = integration / "terlan.toml"
    app_manifest = integration / "examples" / "basic_app" / "angular-ts.json"
    app_index = integration / "examples" / "basic_app" / "index.html"
    generator = integration / "tool" / "generate_terlan_wasm_app.mjs"
    namespace_manifest_generator = integration / "tool" / "generate_ng_namespace_manifest.mjs"
    namespace_check = integration / "tool" / "check_ng_namespace_parity.mjs"
    app_test = integration / "tests" / "wasm_app_contract.test.mjs"
    source.parent.mkdir(parents=True, exist_ok=True)
    app_manifest.parent.mkdir(parents=True, exist_ok=True)
    generator.parent.mkdir(parents=True, exist_ok=True)
    app_test.parent.mkdir(parents=True, exist_ok=True)
    ensure_namespace_fixture(workspace)
    source.write_text(WASM_TERLAN_SOURCE, encoding="utf-8")
    project.write_text(WASM_TERLAN_TOML, encoding="utf-8")
    app_manifest.write_text(WASM_APP_ANGULAR_TS_JSON, encoding="utf-8")
    app_index.write_text(WASM_APP_INDEX, encoding="utf-8")
    app_test.write_text(WASM_APP_CONTRACT_TEST, encoding="utf-8")
    namespace_manifest_generator.write_text(WASM_NAMESPACE_MANIFEST_GENERATOR, encoding="utf-8")
    namespace_check.write_text(WASM_NAMESPACE_BINDINGS_CHECK, encoding="utf-8")
    (integration / "terlan.wasm.integration.json").write_text(
        json.dumps(WASM_TERLAN_MANIFEST, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    generator.write_text(
        f"""import fs from "node:fs";
import path from "node:path";

const files = new Map([
  [path.join("src", "terlan", "angular", "wasm", "App.terl"), {json.dumps(WASM_TERLAN_SOURCE)}],
  ["terlan.toml", {json.dumps(WASM_TERLAN_TOML)}],
  [path.join("examples", "basic_app", "angular-ts.json"), {json.dumps(WASM_APP_ANGULAR_TS_JSON)}],
  [path.join("examples", "basic_app", "index.html"), {json.dumps(WASM_APP_INDEX)}],
]);
const check = process.argv.includes("--check");

if (check) {{
  for (const [filePath, expected] of files) {{
    const actual = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
    if (actual !== expected) {{
      console.error(`${{filePath}} is stale; run make generate`);
      process.exit(1);
    }}
  }}
  process.exit(0);
}}

for (const [filePath, contents] of files) {{
  fs.mkdirSync(path.dirname(filePath), {{ recursive: true }});
  fs.writeFileSync(filePath, contents);
}}
""",
        encoding="utf-8",
    )
    (integration / "README.md").write_text(WASM_TERLAN_README, encoding="utf-8")
    (integration / "Makefile").write_text(
        """TERLC ?= terlc
NODE ?= node
BUILD_DIR ?= build

.PHONY: all generate generate-check manifest-check namespace-manifest namespace-check reserved-build-check check clean

all: check

generate:
\t$(NODE) tool/generate_terlan_wasm_app.mjs

generate-check:
\t$(NODE) tool/generate_terlan_wasm_app.mjs --check

manifest-check:
\t$(NODE) tests/wasm_app_contract.test.mjs

namespace-manifest:
\tTERLAN_BINDGEN_VERSION="$$( $(TERLC) --version | tail -n 1 | awk '{print $$2}' )" $(NODE) tool/generate_ng_namespace_manifest.mjs

namespace-check: namespace-manifest
\trm -rf $(BUILD_DIR)/ng-namespace
\tcd ../../.. && $(TERLC) bind js-dom --manifest integrations/wasm/terlan/generated/ng_namespace_inputs.json --out integrations/wasm/terlan/$(BUILD_DIR)/ng-namespace
\t$(NODE) tool/check_ng_namespace_parity.mjs

reserved-build-check:
\tmkdir -p $(BUILD_DIR)
\t@if $(TERLC) build . --target wasm.browser > $(BUILD_DIR)/reserved-build.out 2>&1; then \\
\t\techo "expected wasm.browser build to remain reserved"; \\
\t\tcat $(BUILD_DIR)/reserved-build.out; \\
\t\texit 1; \\
\tfi
\tgrep -q "reserved for the Wasm target family" $(BUILD_DIR)/reserved-build.out

check: generate generate-check manifest-check namespace-check reserved-build-check

clean:
\trm -rf $(BUILD_DIR)
""",
        encoding="utf-8",
    )
    return integration


def ensure_namespace_fixture(root: Path) -> None:
    """Provide a minimal Angular namespace fixture for temporary roots."""

    namespace_file = root / "@types" / "namespace.d.ts"
    if namespace_file.exists():
        return
    namespace_file.parent.mkdir(parents=True, exist_ok=True)
    namespace_file.write_text(ANGULAR_NAMESPACE_FIXTURE, encoding="utf-8")


def materialize_integration(root: Path, *, force: bool = False) -> Path:
    """Write the Terlan integration package under an Angular.ts root."""

    integration = root / "integrations" / "terlan"
    wasm_integration = root / "integrations" / "wasm" / "terlan"
    if integration.exists() and any(integration.iterdir()) and not force:
        print(
            "angular-ts Terlan integration failed: integrations/terlan already exists; "
            "use --check-materialized to validate it or --force to overwrite it",
            file=sys.stderr,
        )
        sys.exit(1)
    if integration.exists() and force:
        shutil.rmtree(integration)
    if wasm_integration.exists() and force:
        shutil.rmtree(wasm_integration)
    integration = write_integration_layout(root)
    wasm_integration = write_wasm_integration_layout(root)
    validate_integration_layout(integration)
    validate_wasm_integration_layout(wasm_integration)
    return integration


def check_materialized_integration(root: Path) -> Path:
    """Validate an existing Terlan integration package under Angular.ts."""

    integration = root / "integrations" / "terlan"
    wasm_integration = root / "integrations" / "wasm" / "terlan"
    validate_integration_layout(integration)
    validate_wasm_integration_layout(wasm_integration)
    validate_root_makefile_wiring(root)
    run_checked(integration_make_command(integration, "check", browser=True), ROOT)
    run_checked(integration_make_command(wasm_integration, "check"), ROOT)
    validate_namespace_input_manifest(integration)
    validate_namespace_input_manifest(wasm_integration)
    validate_materialized_harness_paths(root)
    return integration


def check_namespace_generation(root: Path) -> None:
    """Validate Angular.ts namespace generation for JS and Wasm packages."""

    integration = materialize_integration(root)
    wasm_integration = root / "integrations" / "wasm" / "terlan"
    run_checked(integration_make_command(integration, "namespace-check"), ROOT)
    run_checked(integration_make_command(wasm_integration, "namespace-check"), ROOT)
    validate_namespace_input_manifest(integration)
    validate_namespace_input_manifest(wasm_integration)


def check_facade_parity(root: Path) -> None:
    """Validate generated Angular.ts Terlan facade parity."""

    check_namespace_generation(root)


def compiler_command() -> list[str]:
    """Return the current compiler as a subprocess argv prefix."""

    if terlc := os.environ.get("TERLC"):
        return shlex.split(terlc)
    return [
        "cargo",
        "run",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
        "-p",
        "terlan",
        "--bin",
        "terlc",
        "--",
    ]


def generated_hashes(root: Path) -> dict[str, str]:
    """Return the binding manifest's stable declared artifact hashes."""

    manifest_path = root / "std" / "js" / "manifests" / "std_js_bindings.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    hashes = {entry["path"]: entry["sha256"] for entry in manifest["generated_files"]}
    skipped_path = root / manifest["skipped_manifest"]
    hashes[manifest["skipped_manifest"]] = hashlib.sha256(skipped_path.read_bytes()).hexdigest()
    return hashes


def check_facade_parity_hardening(root: Path, *, compiler_adversarial: bool = True) -> None:
    """Run compile, drift, skip-policy, and adversarial facade checks."""

    check_namespace_generation(root)
    integration = root / "integrations" / "terlan"
    output = integration / "build" / "ng-namespace"
    namespace = root / "@types" / "namespace.d.ts"
    manifest = integration / "generated" / "ng_namespace_inputs.json"
    checker = integration / "tool" / "check_ng_namespace_bindings.mjs"
    binding_manifest_path = output / "std" / "js" / "manifests" / "std_js_bindings.json"

    def refresh_generated_hash(relative_path: str) -> None:
        binding_manifest = json.loads(binding_manifest_path.read_text(encoding="utf-8"))
        for entry in binding_manifest["generated_files"]:
            if entry["path"] == relative_path:
                entry["sha256"] = hashlib.sha256((output / relative_path).read_bytes()).hexdigest()
                binding_manifest_path.write_text(
                    json.dumps(binding_manifest, indent=2) + "\n", encoding="utf-8"
                )
                return
        print(
            f"angular-ts Terlan integration failed: generated hash entry missing: {relative_path}",
            file=sys.stderr,
        )
        sys.exit(1)

    compile_root = root / "facade-compile"
    facade_types = [
        "Angular", "NgModule", "Component", "Directive", "Scope",
        "TemplateCacheService", "HttpService", "HttpResponse", "Machine",
        "MachineConfig", "MachineSendResult", "MachineService", "MachineSnapshot",
        "Workflow", "WorkflowResult", "WorkflowService", "WorkflowSnapshot",
        "SseConfig", "SseConnection", "SseService", "WebSocketConfig",
        "WebSocketConnection", "WebSocketService", "WorkerConfig", "WorkerHandle",
        "WorkerService",
    ]
    for relative in ["terlan/angular/Ng.terl", "terlan/angular/NgTest.terl"] + [
        f"terlan/angular/ng/{name}.terl" for name in facade_types
    ]:
        destination = compile_root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(output / relative, destination)
    run_checked(compiler_command() + ["check", str(compile_root)], ROOT)
    if compiler_adversarial:
        adversarial_sources = {
            "MachineBad.terl": """module terlan.angular.adversarial.MachineBad.

pub reject(machine: terlan.angular.ng.Machine.Machine[Dynamic], payload: Dynamic): terlan.angular.ng.MachineSendResult.MachineSendResult[std.js.String.JsString] ->
    terlan.angular.Ng.machine_send(machine, 7, payload).
""",
            "WorkflowBad.terl": """module terlan.angular.adversarial.WorkflowBad.

pub reject(workflow: terlan.angular.ng.Workflow.Workflow[Dynamic], input: Dynamic): std.js.Promise[terlan.angular.ng.WorkflowResult.WorkflowResult[Dynamic]] ->
    terlan.angular.Ng.workflow_run(workflow, 7, input).
""",
            "SseBad.terl": """module terlan.angular.adversarial.SseBad.

pub reject(service: terlan.angular.ng.SseService.SseService): terlan.angular.ng.SseConnection.SseConnection ->
    terlan.angular.Ng.sse_connect(service, 7).
""",
            "WebSocketBad.terl": """module terlan.angular.adversarial.WebSocketBad.

pub reject(service: terlan.angular.ng.WebSocketService.WebSocketService): terlan.angular.ng.WebSocketConnection.WebSocketConnection ->
    terlan.angular.Ng.websocket_connect(service, 7).
""",
            "WorkerBad.terl": """module terlan.angular.adversarial.WorkerBad.

pub reject(service: terlan.angular.ng.WorkerService.WorkerService): terlan.angular.ng.WorkerHandle.WorkerHandle[Dynamic, Dynamic] ->
    terlan.angular.Ng.worker_start(service, 7).
""",
            "DirectiveBad.terl": """module terlan.angular.adversarial.DirectiveBad.

pub reject(link: Dynamic): terlan.angular.ng.Directive.Directive[Dynamic] ->
    terlan.angular.Ng.directive_with_link(7, link).
""",
            "TemplateCacheBad.terl": """module terlan.angular.adversarial.TemplateCacheBad.

pub reject(templates: terlan.angular.ng.TemplateCacheService.TemplateCacheService): Option[std.js.String.JsString] ->
    terlan.angular.Ng.template_get(templates, 7).
""",
        }
        adversarial_root = compile_root / "terlan" / "angular" / "adversarial"
        adversarial_root.mkdir(parents=True, exist_ok=True)
        for name, source in adversarial_sources.items():
            (adversarial_root / name).write_text(source, encoding="utf-8")
        result = subprocess.run(
            compiler_command() + ["check", str(compile_root)],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        diagnostics = f"{result.stdout}\n{result.stderr}"
        if result.returncode == 0:
            print(
                "angular-ts Terlan integration failed: adversarial facade calls silently compiled",
                file=sys.stderr,
            )
            sys.exit(1)
        missing_spans = [name for name in adversarial_sources if f"{name}:4:" not in diagnostics]
        if missing_spans:
            print(
                "angular-ts Terlan integration failed: adversarial facade diagnostics missing stable spans: "
                + ", ".join(missing_spans),
                file=sys.stderr,
            )
            print(diagnostics, file=sys.stderr)
            sys.exit(1)
    baseline = generated_hashes(output)

    original_namespace = namespace.read_text(encoding="utf-8")
    namespace.write_text(original_namespace + "\n// facade parity drift probe\n", encoding="utf-8")
    run_expected_failure(
        compiler_command()
        + [
            "bind",
            "js-dom",
            "--manifest",
            str(manifest),
            "--out",
            str(root / "drift-output"),
        ],
        root,
        "ts_bindgen.input_manifest_sha256_mismatch",
    )
    namespace.write_text(original_namespace, encoding="utf-8")
    run_checked(integration_make_command(integration, "namespace-check"), ROOT)
    if generated_hashes(output) != baseline:
        print(
            "angular-ts Terlan integration failed: namespace regeneration changed deterministic binding hashes",
            file=sys.stderr,
        )
        sys.exit(1)

    skipped_path = output / "std" / "js" / "manifests" / "std_js_skipped.json"
    skipped = json.loads(skipped_path.read_text(encoding="utf-8"))
    skipped["skipped"].append(
        {"source": "terlan.angular.ng.UnreasonedProbe", "reason": "", "detail": "probe"}
    )
    skipped_path.write_text(json.dumps(skipped, indent=2) + "\n", encoding="utf-8")
    run_expected_failure(["node", str(checker)], integration, "unstable reason")
    run_checked(integration_make_command(integration, "namespace-check"), ROOT)

    facade = output / "terlan" / "angular" / "Ng.terl"
    facade_source = facade.read_text(encoding="utf-8")
    binding_manifest_source = binding_manifest_path.read_text(encoding="utf-8")
    adversarial_markers = {
        "machine": "pub machine(",
        "workflow": "pub workflow(",
        "sse": "pub sse_connect(service:",
        "websocket": "pub websocket_connect(service:",
        "worker": "pub worker_start(service:",
        "directive": "pub directive_with_link(",
        "template-cache": "pub template_get(templates:",
    }
    for family, marker in adversarial_markers.items():
        if marker not in facade_source:
            print(
                f"angular-ts Terlan integration failed: missing positive {family} facade contract",
                file=sys.stderr,
            )
            sys.exit(1)
        facade.write_text(facade_source.replace(marker, f"pub missing_{family}(unused:", 1), encoding="utf-8")
        refresh_generated_hash("terlan/angular/Ng.terl")
        run_expected_failure(["node", str(checker)], integration, "missing executable marker")
        facade.write_text(facade_source, encoding="utf-8")
        binding_manifest_path.write_text(binding_manifest_source, encoding="utf-8")

    run_checked(["node", str(checker)], integration)


def validate_explicit_angular_root(root: Path) -> None:
    """Validate an explicit CLI root points at an Angular.ts checkout."""

    if (root / "integrations").is_dir():
        return
    print(
        f"angular-ts Terlan integration failed: explicit Angular.ts root must contain integrations/: {root}",
        file=sys.stderr,
    )
    sys.exit(1)


def patch_root_makefile(root: Path) -> None:
    """Insert Terlan integration hooks into the Angular.ts root Makefile."""

    makefile = root / "Makefile"
    if not makefile.is_file():
        print(f"angular-ts Terlan integration failed: root Makefile missing: {makefile}", file=sys.stderr)
        sys.exit(1)
    text = makefile.read_text(encoding="utf-8")
    makefile.write_text(apply_root_makefile_hooks(text), encoding="utf-8")


def apply_root_makefile_hooks(text: str) -> str:
    """Return root Makefile text with Terlan integration hooks inserted."""

    hooks = [
        ("generated-check:", "\t@$(MAKE) -C integrations/terlan generate-check"),
        ("generated-check:", "\t@$(MAKE) -C integrations/wasm/terlan generate-check"),
        ("test-integrations:", "\t@$(MAKE) -C integrations/terlan check"),
        ("test-integrations:", "\t@$(MAKE) -C integrations/wasm/terlan check"),
    ]
    for target, hook in hooks:
        if hook in text:
            continue
        lines = text.splitlines()
        for index, line in enumerate(lines):
            if line.startswith(target):
                lines.insert(index + 1, hook)
                text = "\n".join(lines) + "\n"
                break
        else:
            print(
                f"angular-ts Terlan integration failed: root Makefile missing target {target}",
                file=sys.stderr,
            )
            sys.exit(1)
    return text


def print_root_makefile_patch(root: Path) -> None:
    """Print a unified diff for the Terlan root Makefile hooks."""

    makefile = root / "Makefile"
    if not makefile.is_file():
        print(f"angular-ts Terlan integration failed: root Makefile missing: {makefile}", file=sys.stderr)
        sys.exit(1)
    current = makefile.read_text(encoding="utf-8")
    patched = apply_root_makefile_hooks(current)
    if current == patched:
        print("angular-ts root Makefile already contains Terlan integration hooks")
        return
    diff = difflib.unified_diff(
        current.splitlines(keepends=True),
        patched.splitlines(keepends=True),
        fromfile="a/Makefile",
        tofile="b/Makefile",
    )
    print("".join(diff), end="")


def print_application_patch(root: Path) -> None:
    """Print a reviewable patch for the full external Angular.ts integration."""

    patch_text = application_patch_text(root)
    validate_application_patch_portability(patch_text)
    print(patch_text, end="")


def application_patch_text(root: Path) -> str:
    """Return a reviewable patch for the full external Angular.ts integration."""

    validate_external_root_contract(root)
    output = io.StringIO()
    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-application-patch.") as tmp:
        temp_root = Path(tmp) / "angular.ts"
        (temp_root / "integrations").mkdir(parents=True)
        original_makefile = (root / "Makefile").read_text(encoding="utf-8")
        temp_makefile = temp_root / "Makefile"
        temp_makefile.write_text(original_makefile, encoding="utf-8")
        materialize_integration(temp_root)
        patch_root_makefile(temp_root)
        patched_makefile = temp_makefile.read_text(encoding="utf-8")
        print_unified_text_diff(
            original_makefile,
            patched_makefile,
            Path("a/Makefile"),
            Path("b/Makefile"),
            output=output,
        )
        integration = temp_root / "integrations" / "terlan"
        for path in sorted(file for file in integration.rglob("*") if file.is_file()):
            relative = path.relative_to(temp_root)
            print_unified_text_diff(
                "",
                path.read_text(encoding="utf-8"),
                Path("/dev/null"),
                Path("b") / relative,
                output=output,
            )
        wasm_integration = temp_root / "integrations" / "wasm" / "terlan"
        for path in sorted(file for file in wasm_integration.rglob("*") if file.is_file()):
            relative = path.relative_to(temp_root)
            print_unified_text_diff(
                "",
                path.read_text(encoding="utf-8"),
                Path("/dev/null"),
                Path("b") / relative,
                output=output,
            )
    return output.getvalue()


def print_unified_text_diff(
    before: str,
    after: str,
    before_path: Path,
    after_path: Path,
    *,
    output: io.StringIO | None = None,
) -> None:
    """Print one unified text diff when the file contents differ."""

    if before == after:
        return
    diff = difflib.unified_diff(
        before.splitlines(keepends=True),
        after.splitlines(keepends=True),
        fromfile=str(before_path),
        tofile=str(after_path),
    )
    rendered = "".join(diff)
    if not after.endswith("\n"):
        rendered += "\n"
    if output is None:
        print(rendered, end="")
    else:
        output.write(rendered)


def validate_integration_layout(integration: Path) -> None:
    """Validate the generated integration package boundary files."""

    required = [
        integration / "README.md",
        integration / "Makefile",
        integration / "ROOT_MAKEFILE_HOOKS.md",
        integration / "terlan.integration.json",
        integration / "src" / "terlan" / "angular" / "Todo.terl",
        integration / "src" / "terlan" / "angular" / "TodoSummary.terl.html",
        integration / "tests" / "todo_boundary.test.mjs",
        integration / "tests" / "todo_harness_contract.test.mjs",
        integration / "playwright.config.ts",
        integration / "terlan.test.ts",
        integration / "examples" / "todo" / "index.html",
        integration / "examples" / "todo" / "todo.js",
        integration / "examples" / "todo" / "angular-ts.json",
        integration / "tool" / "generate_terlan_todo.mjs",
        integration / "tool" / "generate_ng_namespace_manifest.mjs",
        integration / "tool" / "check_ng_namespace_bindings.mjs",
    ]
    missing = [path for path in required if not path.is_file()]
    if missing:
        rendered = ", ".join(str(path) for path in missing)
        print(f"angular-ts Terlan integration failed: missing generated files: {rendered}", file=sys.stderr)
        sys.exit(1)
    makefile = (integration / "Makefile").read_text(encoding="utf-8")
    readme = (integration / "README.md").read_text(encoding="utf-8")
    required_readme = [
        "--print-root-makefile-patch /path/to/angular.ts",
        "--print-application-patch /path/to/angular.ts",
        "--materialize /path/to/angular.ts --patch-root-makefile",
        "--check-materialized /path/to/angular.ts",
        "angular-wave/angular.ts",
        "$sse",
        "createSseService",
        "EventSource",
        "make browser-test",
        "TodoSummary.terl.html",
    ]
    missing_readme = [marker for marker in required_readme if marker not in readme]
    if missing_readme:
        rendered = ", ".join(missing_readme)
        print(f"angular-ts Terlan integration failed: README missing usage markers: {rendered}", file=sys.stderr)
        sys.exit(1)
    required_targets = [
        "all: check",
        "build:",
        "generate:",
        "generate-check:",
        "artifact-check:",
        "namespace-manifest:",
        "namespace-check:",
        "test:",
        "harness-check:",
        "browser-test:",
        "app-ownership-check:",
        "check:",
        "run:",
        "clean:",
    ]
    missing_targets = [target for target in required_targets if target not in makefile]
    if missing_targets:
        rendered = ", ".join(missing_targets)
        print(f"angular-ts Terlan integration failed: missing generated Make targets: {rendered}", file=sys.stderr)
        sys.exit(1)
    validate_integration_manifest(integration)
    validate_root_makefile_hooks(integration)


def validate_wasm_integration_layout(integration: Path) -> None:
    """Validate the generated Terlan Wasm integration boundary files."""

    required = [
        integration / "README.md",
        integration / "Makefile",
        integration / "terlan.toml",
        integration / "terlan.wasm.integration.json",
        integration / "src" / "terlan" / "angular" / "wasm" / "App.terl",
        integration / "examples" / "basic_app" / "index.html",
        integration / "examples" / "basic_app" / "angular-ts.json",
        integration / "tests" / "wasm_app_contract.test.mjs",
        integration / "tool" / "generate_terlan_wasm_app.mjs",
        integration / "tool" / "generate_ng_namespace_manifest.mjs",
        integration / "tool" / "check_ng_namespace_parity.mjs",
    ]
    missing = [path for path in required if not path.is_file()]
    if missing:
        rendered = ", ".join(str(path) for path in missing)
        print(f"angular-ts Terlan integration failed: missing generated Wasm files: {rendered}", file=sys.stderr)
        sys.exit(1)
    makefile = (integration / "Makefile").read_text(encoding="utf-8")
    readme = (integration / "README.md").read_text(encoding="utf-8")
    required_make_targets = [
        "all: check",
        "generate:",
        "generate-check:",
        "manifest-check:",
        "namespace-manifest:",
        "namespace-check:",
        "reserved-build-check:",
        "check:",
        "clean:",
    ]
    missing_targets = [target for target in required_make_targets if target not in makefile]
    if missing_targets:
        rendered = ", ".join(missing_targets)
        print(f"angular-ts Terlan integration failed: missing generated Wasm Make targets: {rendered}", file=sys.stderr)
        sys.exit(1)
    required_readme = [
        "Rust integration remains the reference implementation",
        "terlc build --target wasm.browser",
        "reserved backend diagnostics",
    ]
    missing_readme = [marker for marker in required_readme if marker not in readme]
    if missing_readme:
        rendered = ", ".join(missing_readme)
        print(f"angular-ts Terlan integration failed: Wasm README missing markers: {rendered}", file=sys.stderr)
        sys.exit(1)
    validate_wasm_integration_manifest(integration)


def validate_root_makefile_hooks(integration: Path) -> None:
    """Validate the generated root Makefile hook documentation."""

    hooks = (integration / "ROOT_MAKEFILE_HOOKS.md").read_text(encoding="utf-8")
    required = [
        "@$(MAKE) -C integrations/terlan generate-check",
        "@$(MAKE) -C integrations/terlan check",
        "@$(MAKE) -C integrations/wasm/terlan generate-check",
        "@$(MAKE) -C integrations/wasm/terlan check",
    ]
    missing = [marker for marker in required if marker not in hooks]
    if missing:
        rendered = ", ".join(missing)
        print(f"angular-ts Terlan integration failed: root Makefile hooks missing: {rendered}", file=sys.stderr)
        sys.exit(1)


def validate_wasm_integration_manifest(integration: Path) -> None:
    """Validate the generated Terlan Wasm integration manifest."""

    manifest_path = integration / "terlan.wasm.integration.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except OSError as err:
        print(f"angular-ts Terlan integration failed: cannot read {manifest_path}: {err}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as err:
        print(f"angular-ts Terlan integration failed: invalid Wasm integration manifest: {err}", file=sys.stderr)
        sys.exit(1)

    if manifest != WASM_TERLAN_MANIFEST:
        print("angular-ts Terlan integration failed: Wasm integration manifest drifted", file=sys.stderr)
        sys.exit(1)


def validate_root_makefile_wiring(root: Path) -> None:
    """Validate root Angular.ts Makefile wiring when a Makefile is present."""

    makefile = root / "Makefile"
    if not makefile.exists():
        return
    text = makefile.read_text(encoding="utf-8")
    required = [
        "$(MAKE) -C integrations/terlan generate-check",
        "$(MAKE) -C integrations/terlan check",
        "$(MAKE) -C integrations/wasm/terlan generate-check",
        "$(MAKE) -C integrations/wasm/terlan check",
    ]
    missing = [marker for marker in required if marker not in text]
    if missing:
        rendered = ", ".join(missing)
        print(f"angular-ts Terlan integration failed: root Makefile is not wired for Terlan: {rendered}", file=sys.stderr)
        sys.exit(1)


def validate_materialized_harness_paths(root: Path) -> None:
    """Validate that todo harness imports resolve to materialized files."""

    harness = root / "integrations" / "terlan" / "examples" / "todo" / "todo.js"
    imports = [
        "../../../../dist/index.js",
        "../../build/js/modules/terlan/angular/Todo.js",
    ]
    text = harness.read_text(encoding="utf-8")
    for specifier in imports:
        if specifier not in text:
            print(
                f"angular-ts Terlan integration failed: todo harness missing import {specifier}",
                file=sys.stderr,
            )
            sys.exit(1)
        target = (harness.parent / specifier).resolve()
        if not target.is_file():
            print(
                f"angular-ts Terlan integration failed: todo harness import does not resolve: {specifier} -> {target}",
                file=sys.stderr,
            )
            sys.exit(1)


def validate_namespace_input_manifest(integration: Path) -> None:
    """Validate the generated Angular.ts namespace input manifest contract."""

    manifest_path = integration / "generated" / "ng_namespace_inputs.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except OSError as err:
        print(f"angular-ts Terlan integration failed: cannot read {manifest_path}: {err}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as err:
        print(f"angular-ts Terlan integration failed: invalid namespace input manifest: {err}", file=sys.stderr)
        sys.exit(1)

    source_package = manifest.get("source_package")
    expected_source_package = {
        "name": "typescript",
        "version": "local",
        "resolution": "@types/namespace.d.ts",
    }
    if source_package != expected_source_package:
        print(
            "angular-ts Terlan integration failed: namespace input manifest source_package drifted",
            file=sys.stderr,
        )
        sys.exit(1)

    inputs = manifest.get("inputs")
    if not isinstance(inputs, list) or len(inputs) != 1:
        print("angular-ts Terlan integration failed: namespace input manifest must pin one input", file=sys.stderr)
        sys.exit(1)
    input_manifest = inputs[0]
    expected_input = {
        "path": "@types/namespace.d.ts",
        "kind": "typescript-declaration",
        "namespace": "terlan.angular",
    }
    for key, expected in expected_input.items():
        if input_manifest.get(key) != expected:
            print(
                f"angular-ts Terlan integration failed: namespace input manifest {key} drifted",
                file=sys.stderr,
            )
            sys.exit(1)
    sha256 = input_manifest.get("sha256")
    if not isinstance(sha256, str) or len(sha256) != 64:
        print("angular-ts Terlan integration failed: namespace input manifest sha256 is invalid", file=sys.stderr)
        sys.exit(1)


def validate_root_makefile_patch_idempotence(workspace: Path) -> None:
    """Prove root Makefile patching does not duplicate Terlan hooks."""

    root = workspace / "root-makefile-patch"
    root.mkdir()
    makefile = root / "Makefile"
    makefile.write_text(
        "generated-check: types\n\t@true\n\ntest-integrations: ensure-deps\n\t@true\n",
        encoding="utf-8",
    )
    materialize_integration(root)
    patch_root_makefile(root)
    patch_root_makefile(root)
    text = makefile.read_text(encoding="utf-8")
    hooks = [
        "$(MAKE) -C integrations/terlan generate-check",
        "$(MAKE) -C integrations/terlan check",
        "$(MAKE) -C integrations/wasm/terlan generate-check",
        "$(MAKE) -C integrations/wasm/terlan check",
    ]
    duplicated = [hook for hook in hooks if text.count(hook) != 1]
    if duplicated:
        rendered = ", ".join(duplicated)
        print(f"angular-ts Terlan integration failed: root Makefile hook was duplicated: {rendered}", file=sys.stderr)
        sys.exit(1)
    validate_root_makefile_wiring(root)


def validate_root_makefile_patch_print_is_portable(workspace: Path) -> None:
    """Prove root Makefile patch printing emits a portable review diff."""

    root = workspace / "root-makefile-print"
    root.mkdir()
    (root / "integrations").mkdir()
    (root / "Makefile").write_text(
        "generated-check: types\n\t@true\n\ntest-integrations: ensure-deps\n\t@true\n",
        encoding="utf-8",
    )
    result = subprocess.run(
        [
            sys.executable,
            "-B",
            str(Path(__file__).resolve()),
            "--print-root-makefile-patch",
            str(root),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        print("angular-ts Terlan integration failed: root Makefile patch print failed", file=sys.stderr)
        if result.stdout:
            print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
        if result.stderr:
            print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
        sys.exit(result.returncode)
    patch_text = result.stdout
    forbidden = [str(root), "/home/"]
    found = [marker for marker in forbidden if marker in patch_text]
    if found:
        rendered = ", ".join(found)
        print(
            f"angular-ts Terlan integration failed: root Makefile patch is not portable: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)
    required = [
        "--- a/Makefile",
        "+++ b/Makefile",
        "@$(MAKE) -C integrations/terlan generate-check",
        "@$(MAKE) -C integrations/terlan check",
        "@$(MAKE) -C integrations/wasm/terlan generate-check",
        "@$(MAKE) -C integrations/wasm/terlan check",
    ]
    missing = [marker for marker in required if marker not in patch_text]
    if missing:
        rendered = ", ".join(missing)
        print(
            f"angular-ts Terlan integration failed: root Makefile patch missing expected content: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)


def validate_integration_manifest(integration: Path) -> None:
    """Validate the generated integration manifest is explicit and complete."""

    manifest_path = integration / "terlan.integration.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except OSError as err:
        print(f"angular-ts Terlan integration failed: cannot read {manifest_path}: {err}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as err:
        print(f"angular-ts Terlan integration failed: invalid integration manifest: {err}", file=sys.stderr)
        sys.exit(1)

    expected = INTEGRATION_MANIFEST
    if manifest != expected:
        print("angular-ts Terlan integration failed: integration manifest drifted", file=sys.stderr)
        sys.exit(1)


def validate_generator_freshness_gate(integration: Path) -> None:
    """Prove generated Terlan source freshness is enforced."""

    source = integration / "src" / "terlan" / "angular" / "Todo.terl"
    source.write_text("module stale.\n", encoding="utf-8")
    run_expected_failure(["make", "-C", str(integration), "generate-check"], ROOT, "is stale")


def validate_app_ownership(integration: Path) -> None:
    """Prove Terlan source owns app metadata and Todo behavior."""

    source = (integration / "src" / "terlan" / "angular" / "Todo.terl").read_text(encoding="utf-8")
    bootstrap = (integration / "examples" / "todo" / "todo.js").read_text(encoding="utf-8")
    manifest_path = integration / "examples" / "todo" / "angular-ts.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    source_markers = [
        "pub struct TodoItem",
        "pub struct TodoState",
        "pub app_module(): String ->",
        "pub controller_name(): String ->",
        "pub initial_state(): TodoState ->",
        "pub summary(title: String, state: String): Html ->",
        "pub can_create(text: String): Bool ->",
        "pub create_item(id: Int, text: String): TodoItem ->",
        "pub toggle_item(item: TodoItem): TodoItem ->",
        "pub keep_item(item: TodoItem, removed_id: Int): Bool ->",
        "pub visible(item: TodoItem, filter: String): Bool ->",
        "pub render_row(item: TodoItem): String ->",
    ]
    bootstrap_markers = [
        "angular.module(Todo.app_module(), [])",
        "app.controller(Todo.controller_name()",
        "Todo.initial_state()",
        "Todo.can_create(text)",
        "Todo.create_item(todoModel.nextId, text)",
        "Todo.toggle_item(item)",
        "Todo.edit_item(item)",
        "Todo.keep_item(candidate, item.id)",
        "Todo.select_filter(filter)",
        "Todo.visible(item, todoModel.filter)",
        "Todo.render_row(item)",
        "angular.bootstrap(document.body, [app.name])",
    ]
    forbidden_bootstrap = [
        'angular.module("terlanTodo", [])',
        "item.completed = !item.completed",
        "candidate !== item",
        'this.filter === "active"',
    ]
    missing = [marker for marker in source_markers if marker not in source]
    missing.extend(marker for marker in bootstrap_markers if marker not in bootstrap)
    forbidden = [marker for marker in forbidden_bootstrap if marker in bootstrap]
    if missing or forbidden or manifest != TODO_APP_MANIFEST:
        problems = [*(f"missing {marker}" for marker in missing), *(f"forbidden {marker}" for marker in forbidden)]
        details = ", ".join(problems)
        if manifest != TODO_APP_MANIFEST:
            details = f"{details}, app manifest drifted" if details else "app manifest drifted"
        print(f"angular-ts Terlan app ownership failed: {details}", file=sys.stderr)
        sys.exit(1)


def validate_app_ownership_freshness_gate(integration: Path) -> None:
    """Prove every generated app asset rejects drift and regenerates."""

    generated = [
        integration / "src" / "terlan" / "angular" / "Todo.terl",
        integration / "src" / "terlan" / "angular" / "TodoSummary.terl.html",
        integration / "examples" / "todo" / "index.html",
        integration / "examples" / "todo" / "todo.js",
        integration / "examples" / "todo" / "angular-ts.json",
        integration / "playwright.config.ts",
        integration / "terlan.test.ts",
    ]
    for path in generated:
        path.write_text("stale\n", encoding="utf-8")
        run_expected_failure(
            ["make", "-C", str(integration), "generate-check"],
            ROOT,
            f"{path.relative_to(integration)} is stale",
        )
        run_checked(["make", "-C", str(integration), "generate"], ROOT)
    validate_app_ownership(integration)


def check_app_ownership(root: Path, *, browser: bool = False) -> None:
    """Validate the generated Terlan-owned Angular.ts app boundary."""

    integration = materialize_integration(root)
    validate_app_ownership(integration)
    validate_app_ownership_freshness_gate(integration)
    run_checked(
        integration_make_command(integration, "app-ownership-check", browser=browser),
        ROOT,
    )


def validate_force_materialization_prunes_stale_files(workspace: Path) -> None:
    """Prove forced materialization replaces stale integration files."""

    root = workspace / "force-materialize"
    (root / "integrations").mkdir(parents=True)
    integration = materialize_integration(root)
    stale = integration / "obsolete.local"
    stale.write_text("stale\n", encoding="utf-8")
    materialize_integration(root, force=True)
    if stale.exists():
        print(
            "angular-ts Terlan integration failed: --force left stale integration files behind",
            file=sys.stderr,
        )
        sys.exit(1)
    validate_integration_layout(root / "integrations" / "terlan")
    validate_wasm_integration_layout(root / "integrations" / "wasm" / "terlan")


def validate_explicit_root_commands_reject_invalid_root(workspace: Path) -> None:
    """Prove explicit root commands refuse non-Angular roots."""

    invalid_root = workspace / "not-angular"
    invalid_root.mkdir()
    for command in ["--materialize", "--check-materialized"]:
        run_expected_failure(
            [
                sys.executable,
                "-B",
                str(Path(__file__).resolve()),
                command,
                str(invalid_root),
            ],
            ROOT,
            "explicit Angular.ts root must contain integrations/",
        )


def validate_manifest(build_dir: Path) -> Path:
    """Validate JS build metadata and return the emitted todo module path."""

    js_root = build_dir / "js"
    manifest_path = js_root / "manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except OSError as err:
        print(f"angular-ts Terlan integration failed: cannot read {manifest_path}: {err}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as err:
        print(f"angular-ts Terlan integration failed: invalid JSON manifest: {err}", file=sys.stderr)
        sys.exit(1)

    if manifest.get("target_profile") != "js.shared":
        print(
            "angular-ts Terlan integration failed: expected js.shared manifest target",
            file=sys.stderr,
        )
        sys.exit(1)
    modules = manifest.get("modules")
    if not isinstance(modules, list):
        print("angular-ts Terlan integration failed: manifest modules must be a list", file=sys.stderr)
        sys.exit(1)
    expected_artifact = "modules/terlan/angular/Todo.js"
    if not any(module.get("relative_path") == expected_artifact for module in modules if isinstance(module, dict)):
        print(
            f"angular-ts Terlan integration failed: missing manifest artifact {expected_artifact}",
            file=sys.stderr,
        )
        sys.exit(1)
    artifact = js_root / expected_artifact
    if not artifact.is_file():
        print(f"angular-ts Terlan integration failed: missing emitted module {artifact}", file=sys.stderr)
        sys.exit(1)
    return artifact


def write_node_check(workspace: Path, artifact: Path) -> Path:
    """Write a Node ES-module smoke test for the emitted Terlan JS module."""

    relative_artifact = artifact.relative_to(workspace).as_posix()
    check = workspace / "check.mjs"
    check.write_text(
        f"""import * as Todo from './{relative_artifact}';
{TODO_BOUNDARY_TEST.split(';', 1)[1]}""",
        encoding="utf-8",
    )
    return check


def find_external_root() -> Path | None:
    """Return an environment-selected or sibling Angular.ts checkout."""

    if env_root := os.environ.get("ANGULAR_TS_ROOT"):
        candidate = Path(env_root)
        if (candidate / "integrations").is_dir():
            return candidate
        print(
            f"angular-ts Terlan integration failed: ANGULAR_TS_ROOT is not an Angular.ts checkout: {candidate}",
            file=sys.stderr,
        )
        sys.exit(1)
    for candidate in [
        ROOT.parent / "angular.ts",
        ROOT.parent.parent / "angular.ts",
        ROOT.parent.parent / "ng" / "angular.ts",
    ]:
        if (candidate / "integrations").is_dir() and (candidate / "@types" / "namespace.d.ts").is_file():
            return candidate
    return None


def validate_external_root_contract(root: Path) -> None:
    """Validate the Angular.ts integration conventions Terlan must fit."""

    makefile = root / "Makefile"
    try:
        text = makefile.read_text(encoding="utf-8")
    except OSError as err:
        print(f"angular-ts Terlan integration failed: cannot read {makefile}: {err}", file=sys.stderr)
        sys.exit(1)

    required_root_markers = [
        "test-integrations:",
        "generated-check:",
        "integrations/closure/Makefile",
        "integrations/kotlin",
        "integrations/wasm/rust",
    ]
    missing_markers = [marker for marker in required_root_markers if marker not in text]
    if missing_markers:
        rendered = ", ".join(missing_markers)
        print(
            f"angular-ts Terlan integration failed: external Makefile missing markers: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)

    integration_root = root / "integrations"
    exemplar_integrations = [
        integration_root / "closure",
        integration_root / "kotlin",
        integration_root / "gleam",
    ]
    missing_files = [
        path
        for integration in exemplar_integrations
        for path in [integration / "README.md", integration / "Makefile"]
        if not path.is_file()
    ]
    if missing_files:
        rendered = ", ".join(str(path) for path in missing_files)
        print(
            f"angular-ts Terlan integration failed: external integration convention missing files: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)
    validate_external_sse_runtime_contract(root)


def validate_external_sse_runtime_contract(root: Path) -> None:
    """Validate Angular.ts owns the browser SSE runtime Terlan will target."""

    candidates = [
        root / "src" / "services" / "sse" / "sse.ts",
        root / "dist" / "services" / "sse" / "sse.js",
    ]
    sse_source = next((path for path in candidates if path.is_file()), None)
    if sse_source is None:
        rendered = ", ".join(str(path) for path in candidates)
        print(
            f"angular-ts Terlan integration failed: missing Angular.ts SSE runtime source: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)
    text = sse_source.read_text(encoding="utf-8")
    required = ["createSseService", "EventSource", "ConnectionManager"]
    missing = [marker for marker in required if marker not in text]
    if missing:
        rendered = ", ".join(missing)
        print(
            f"angular-ts Terlan integration failed: Angular.ts SSE runtime missing markers: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)
    validate_external_sse_declaration_contract(root)


def validate_external_sse_declaration_contract(root: Path) -> None:
    """Validate real Angular.ts declarations expose the SSE surface Terlan binds."""

    required_files = {
        root / "@types" / "services" / "sse" / "sse.d.ts": [
            "export interface SseConfig",
            "extends ConnectionConfig",
            "export interface SseConnection",
            "close(): void;",
            "reconnect(): void;",
            "export type SseService",
        ],
        root / "@types" / "namespace.d.ts": [
            "type SseService = TSseService;",
            "type SseConfig = TSseConfig;",
            "type SseConnection = TSseConnection;",
            "type RealtimeProtocolEventDetail",
            "type RealtimeProtocolMessage",
        ],
        root / "@types" / "ng.d.ts": [
            "ngSseDirective",
            "$sse: RuntimeRegistrationRecipe;",
        ],
        root / "@types" / "interface.d.ts": [
            "$sse: ng.SseService;",
            'readonly $sse: "$sse";',
        ],
        root / "@types" / "directive" / "http" / "http.d.ts": [
            "ngSseDirective",
        ],
    }
    for path, markers in required_files.items():
        if not path.is_file():
            print(
                f"angular-ts Terlan integration failed: missing Angular.ts declaration file: {path}",
                file=sys.stderr,
            )
            sys.exit(1)
        text = path.read_text(encoding="utf-8")
        missing = [marker for marker in markers if marker not in text]
        if missing:
            rendered = ", ".join(missing)
            print(
                f"angular-ts Terlan integration failed: Angular.ts declaration {path} missing markers: {rendered}",
                file=sys.stderr,
            )
            sys.exit(1)


def report_external_root_status() -> None:
    """Report and validate whether the external Angular.ts checkout is visible."""

    root = find_external_root()
    if root is not None:
        validate_external_root_contract(root)
        validate_external_root_temp_materialization(root)
        validate_application_patch_apply(root)
        print(f"angular-ts external root detected and validated: {root}")
        materialized = root / "integrations" / "terlan"
        if materialized.is_dir():
            print(
                "angular-ts Terlan integration materialized package exists; validate it explicitly with "
                "`python3 -B tools/check_angular_ts_terlan_integration.py "
                f"--check-materialized {root}`"
            )
            return
        print(
            "angular-ts Terlan integration not materialized; review root Makefile patch with "
            "`python3 -B tools/check_angular_ts_terlan_integration.py "
            f"--print-root-makefile-patch {root}`"
        )
        print(
            "angular-ts Terlan integration not materialized; review full application patch with "
            "`python3 -B tools/check_angular_ts_terlan_integration.py "
            f"--print-application-patch {root} > /tmp/angular-terlan.patch`"
        )
        print(
            "angular-ts Terlan integration not materialized; apply with "
            "`python3 -B tools/check_angular_ts_terlan_integration.py "
            f"--materialize {root} --patch-root-makefile`"
        )
        return
    print("angular-ts external root not detected; validated temp Terlan integration layout only")


def validate_external_root_temp_materialization(root: Path) -> None:
    """Validate Terlan materialization against the real Angular.ts root Makefile."""

    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-real-root.") as tmp:
        temp_root = Path(tmp) / "angular.ts"
        (temp_root / "integrations").mkdir(parents=True)
        (temp_root / "Makefile").write_text(
            (root / "Makefile").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        link_external_runtime_fixtures(root, temp_root)
        materialize_integration(temp_root)
        patch_root_makefile(temp_root)
        check_materialized_integration(temp_root)
    print("angular-ts external root temp materialization passed")


def validate_external_root_namespace_generation(root: Path) -> None:
    """Validate namespace generation against the real Angular.ts declarations."""

    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-namespace.") as tmp:
        temp_root = Path(tmp) / "angular.ts"
        (temp_root / "integrations").mkdir(parents=True)
        (temp_root / "Makefile").write_text(
            (root / "Makefile").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        link_external_runtime_fixtures(root, temp_root)
        check_namespace_generation(temp_root)
    print("angular-ts external root namespace generation passed")


def validate_external_root_app_ownership(root: Path) -> None:
    """Validate app ownership against the selected Angular.ts runtime."""

    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-app-ownership.") as tmp:
        temp_root = Path(tmp) / "angular.ts"
        (temp_root / "integrations").mkdir(parents=True)
        (temp_root / "Makefile").write_text(
            (root / "Makefile").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        link_external_runtime_fixtures(root, temp_root)
        check_app_ownership(temp_root, browser=True)
    print("angular-ts external root app ownership passed")


def validate_application_patch_apply(root: Path) -> None:
    """Validate the dry-run application patch applies and passes package checks."""

    patch_text = application_patch_text(root)
    validate_application_patch_portability(patch_text)
    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-patch-apply.") as tmp:
        temp_root = Path(tmp) / "angular.ts"
        (temp_root / "integrations").mkdir(parents=True)
        (temp_root / "Makefile").write_text(
            (root / "Makefile").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        result = subprocess.run(
            ["patch", "-p1"],
            cwd=temp_root,
            input=patch_text,
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            print("angular-ts Terlan integration failed: application patch did not apply", file=sys.stderr)
            if result.stdout:
                print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
            if result.stderr:
                print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
            sys.exit(result.returncode)
        link_external_runtime_fixtures(root, temp_root)
        check_materialized_integration(temp_root)
        validate_application_patch_duplicate_rejected(temp_root, patch_text)
    print("angular-ts external root application patch apply passed")


def link_external_runtime_fixtures(root: Path, temp_root: Path) -> None:
    """Link external Angular.ts runtime inputs required by materialized checks."""

    for name in ["dist", "@types", "node_modules"]:
        source = root / name
        target = temp_root / name
        if source.is_dir() and not target.exists():
            try:
                target.symlink_to(source, target_is_directory=True)
            except OSError:
                shutil.copytree(source, target, symlinks=True)
    required = [
        temp_root / "dist" / "index.js",
        temp_root / "@types" / "namespace.d.ts",
    ]
    missing = [path for path in required if not path.exists()]
    if missing:
        for target in [temp_root / "dist", temp_root / "@types", temp_root / "node_modules"]:
            if target.is_symlink():
                target.unlink()
        for name in ["dist", "@types", "node_modules"]:
            source = root / name
            target = temp_root / name
            if source.is_dir() and not target.exists():
                shutil.copytree(source, target, symlinks=True)


def validate_application_patch_portability(patch_text: str) -> None:
    """Validate that the reviewable application patch is portable."""

    forbidden = [
        str(ROOT),
        "/home/",
        "cargo run",
    ]
    found = [marker for marker in forbidden if marker in patch_text]
    if found:
        rendered = ", ".join(found)
        print(
            f"angular-ts Terlan integration failed: application patch is not portable: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)
    required = [
        "b/integrations/terlan/Makefile",
        "b/integrations/wasm/terlan/Makefile",
        "TERLC ?= terlc",
        "namespace-check:",
        "b/integrations/terlan/tool/check_ng_namespace_bindings.mjs",
        "b/integrations/wasm/terlan/tool/check_ng_namespace_parity.mjs",
        "b/integrations/terlan/examples/todo/todo.js",
        "b/integrations/wasm/terlan/src/terlan/angular/wasm/App.terl",
    ]
    missing = [marker for marker in required if marker not in patch_text]
    if missing:
        rendered = ", ".join(missing)
        print(
            f"angular-ts Terlan integration failed: application patch missing expected portable content: {rendered}",
            file=sys.stderr,
        )
        sys.exit(1)


def validate_application_patch_duplicate_rejected(root: Path, patch_text: str) -> None:
    """Prove the generated application patch cannot be applied twice."""

    result = subprocess.run(
        ["patch", "--batch", "--forward", "-p1"],
        cwd=root,
        input=patch_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        return
    print(
        "angular-ts Terlan integration failed: application patch applied twice without conflict",
        file=sys.stderr,
    )
    sys.exit(1)


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse the integration gate CLI arguments."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--materialize",
        type=Path,
        metavar="ANGULAR_TS_ROOT",
        help="write integrations/terlan under the supplied Angular.ts root and exit",
    )
    parser.add_argument(
        "--check-materialized",
        type=Path,
        metavar="ANGULAR_TS_ROOT",
        help="validate an existing integrations/terlan package under the supplied Angular.ts root and exit",
    )
    parser.add_argument(
        "--namespace-generation-check",
        action="store_true",
        help="validate generated Angular.ts namespace manifests and Terlan binding outputs and exit",
    )
    parser.add_argument(
        "--facade-parity-check",
        action="store_true",
        help="validate generated Angular.ts Terlan facade modules, wrappers, and skip manifests and exit",
    )
    parser.add_argument(
        "--facade-parity-hardening-check",
        action="store_true",
        help="compile and adversarially validate expanded Angular.ts facade families",
    )
    parser.add_argument(
        "--app-ownership-check",
        action="store_true",
        help="validate Terlan-owned Angular.ts app metadata, behavior, generated adapter, and freshness",
    )
    parser.add_argument(
        "--patch-root-makefile",
        action="store_true",
        help="insert Terlan generated-check and test-integrations hooks into the supplied Angular.ts root Makefile",
    )
    parser.add_argument(
        "--print-root-makefile-patch",
        type=Path,
        metavar="ANGULAR_TS_ROOT",
        help="print a unified diff for the required root Makefile hooks and exit without writing files",
    )
    parser.add_argument(
        "--print-application-patch",
        type=Path,
        metavar="ANGULAR_TS_ROOT",
        help="print a unified diff for the root Makefile hooks and generated Terlan integration files",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="allow --materialize to overwrite an existing integrations/terlan package",
    )
    return parser.parse_args(argv)


def main() -> int:
    """Run the Angular.ts Terlan integration gate."""

    args = parse_args(sys.argv[1:])
    if args.print_application_patch is not None:
        print_application_patch(args.print_application_patch)
        return 0
    if args.print_root_makefile_patch is not None:
        print_root_makefile_patch(args.print_root_makefile_patch)
        return 0
    if args.patch_root_makefile and args.materialize is None and args.check_materialized is None:
        print(
            "angular-ts Terlan integration failed: --patch-root-makefile requires --materialize or --check-materialized",
            file=sys.stderr,
        )
        return 2
    if args.force and args.materialize is None:
        print(
            "angular-ts Terlan integration failed: --force requires --materialize",
            file=sys.stderr,
        )
        return 2
    if args.materialize is not None:
        validate_explicit_angular_root(args.materialize)
        integration = materialize_integration(args.materialize, force=args.force)
        if args.patch_root_makefile:
            patch_root_makefile(args.materialize)
            validate_root_makefile_wiring(args.materialize)
        print(f"angular-ts Terlan integration materialized: {integration}")
        return 0
    if args.check_materialized is not None:
        validate_explicit_angular_root(args.check_materialized)
        if args.patch_root_makefile:
            patch_root_makefile(args.check_materialized)
        integration = check_materialized_integration(args.check_materialized)
        print(f"angular-ts Terlan integration materialized package passed: {integration}")
        return 0
    if args.namespace_generation_check:
        with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-namespace.") as tmp:
            check_namespace_generation(Path(tmp))
        root = find_external_root()
        if root is not None:
            validate_external_root_contract(root)
            validate_external_root_namespace_generation(root)
            print(f"angular-ts namespace generation validated against external root: {root}")
        else:
            print("angular-ts external root not detected; validated temp namespace generation only")
        print("angular-ts namespace generation boundary passed")
        return 0
    if args.facade_parity_check:
        with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-facade.") as tmp:
            check_facade_parity(Path(tmp))
        root = find_external_root()
        if root is not None:
            validate_external_root_contract(root)
            validate_external_root_namespace_generation(root)
            print(f"angular-ts facade parity validated against external root: {root}")
        else:
            print("angular-ts external root not detected; validated temp facade parity only")
        print("angular-ts facade parity boundary passed")
        return 0
    if args.facade_parity_hardening_check:
        with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-facade-hardening.") as tmp:
            check_facade_parity_hardening(Path(tmp))
        root = find_external_root()
        if root is not None:
            validate_external_root_contract(root)
            with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-facade-hardening-external.") as tmp:
                temp_root = Path(tmp) / "angular.ts"
                (temp_root / "integrations").mkdir(parents=True)
                (temp_root / "Makefile").write_text(
                    (root / "Makefile").read_text(encoding="utf-8"), encoding="utf-8"
                )
                link_external_runtime_fixtures(root, temp_root)
                check_facade_parity_hardening(temp_root, compiler_adversarial=False)
            print(f"angular-ts facade hardening validated against external root: {root}")
        else:
            print("angular-ts external root not detected; validated hermetic facade hardening only")
        print("angular-ts facade parity hardening boundary passed")
        return 0
    if args.app_ownership_check:
        with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-app-ownership.") as tmp:
            check_app_ownership(Path(tmp))
        root = find_external_root()
        if root is not None:
            validate_external_root_app_ownership(root)
            print(f"angular-ts app ownership validated against external root: {root}")
        else:
            print("angular-ts external root not selected; validated hermetic app ownership only")
        print("angular-ts Terlan app ownership boundary passed")
        return 0

    with tempfile.TemporaryDirectory(prefix="terlan-angular-ts-integration.") as tmp:
        workspace = Path(tmp)
        integration = materialize_integration(workspace)
        validate_generator_freshness_gate(integration)
        validate_force_materialization_prunes_stale_files(workspace)
        validate_explicit_root_commands_reject_invalid_root(workspace)
        validate_root_makefile_patch_idempotence(workspace)
        validate_root_makefile_patch_print_is_portable(workspace)
        run_checked(integration_make_command(integration, "check"), ROOT)
        validate_namespace_input_manifest(integration)
        build_dir = integration / "build"
        artifact = validate_manifest(build_dir)
        node_check = write_node_check(integration, artifact)
        run_checked(["node", str(node_check)], workspace)
    report_external_root_status()
    print("angular-ts Terlan integration boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
