import crypto from "node:crypto";
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
  const unresolvedAliasPattern = new RegExp(`pub type ${typeName}(?:\\[[^\\]]+\\])? =\\s*\\n\\s*T[A-Z]`);
  if (unresolvedAliasPattern.test(source)) {
    throw new Error(`generated core facade leaks unresolved TypeScript alias: ${sourcePath}`);
  }
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
  const lines = fs.readFileSync(filePath, "utf8").split(/\r?\n/);
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
    const match = /^\s*type\s+([A-Za-z_$][A-Za-z0-9_$]*)\b/.exec(line);
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
