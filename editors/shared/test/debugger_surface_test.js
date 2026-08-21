"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EDITORS_ROOT = path.resolve(__dirname, "..", "..");
const REPO_ROOT = path.resolve(EDITORS_ROOT, "..");

const DEBUGGER_DOC = "docs/runtime/EDITOR_DEBUGGER_SURFACE.md";
const VM_IMAGE_DOC = "docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md";
const VM_IMAGE_SOURCE = "crates/terlan/src/runtime/native_image/image.rs";
const BUILD_IMAGE_SOURCE = "crates/terlan/src/commands/build/vm_artifact/native_image.rs";
const NATIVE_DEBUG_SOURCE = "crates/terlan/src/runtime/native_image/debug.rs";
const CLI_DISPATCH_SOURCE = "crates/terlan/src/cli_dispatch.rs";
const CLI_USAGE_SOURCE = "crates/terlan/src/cli_usage.rs";
const CLI_DEBUG_SOURCE = "crates/terlan/src/commands/debug/mod.rs";
const CLI_DEBUG_SESSION_SOURCE = "crates/terlan/src/commands/debug/session.rs";
const VSCODE_MANIFEST = "editors/vscode/package.json";

const REQUIRED_DEBUGGER_TERMS = Object.freeze([
  "debug type `terlan-vm`",
  "terlan-vm debug-adapter --stdio",
  "terlc debug <image.tvm>",
  "actually admitted image",
  "terlc --diagnostic-format json debug build/app.tvm --script session.terldbg",
  "`launch`",
  "`attach`",
  "`setBreakpoints`",
  "`setFunctionBreakpoints`",
  "`continue`",
  "`next`",
  "`stepIn`",
  "`stepOut`",
  "`pause`",
  "`stackTrace`",
  "`scopes`",
  "`variables`",
  "`evaluate`",
  "mailbox state",
  "NativeBoundary resources",
  "trace ids",
  "process ids",
  "generation ids",
  "NativeBoundary call spans",
]);

const REQUIRED_CLI_DEBUG_TERMS = Object.freeze([
  "native_image_admitted",
  "--json-events",
  "--script",
  "--break",
]);

const REQUIRED_IMAGE_DOC_TERMS = Object.freeze([
  "TVMDBG05",
  ".debug_terlan",
  "__terlan",
  ".tdbg$D",
  ".tdbg",
  "UTF-8-safe declaration",
  "SHA-256 digest",
  "never embeds source text",
  "metadata, not executable IR",
]);

/**
 * Reads one repository-relative UTF-8 file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {string} File contents.
 *
 * @description
 * Keeps the debugger surface gate dependency-free and editor-runtime-free.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
}

/**
 * Reads one repository-relative JSON file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {*} Parsed JSON contents.
 *
 * @description
 * Allows the check to inspect editor metadata without launching VS Code.
 */
function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

/**
 * Asserts that text contains all required markers.
 *
 * @param {string} label Human-readable file label.
 * @param {string} text Text to inspect.
 * @param {readonly string[]} markers Required substrings.
 * @returns {void}
 *
 * @description
 * Produces precise assertion messages when a contract marker drifts.
 */
function assertContainsAll(label, text, markers) {
  for (const marker of markers) {
    assert.ok(text.includes(marker), `${label} missing marker ${marker}`);
  }
}

/**
 * Verifies the debugger contract document names the required surface.
 *
 * @returns {void}
 *
 * @description
 * Locks launch/attach, stepping, breakpoint, stack, variable, mailbox,
 * resource, trace, and NativeBoundary span vocabulary into a checked contract.
 */
function testDebuggerContractDocument() {
  assertContainsAll(DEBUGGER_DOC, readText(DEBUGGER_DOC), REQUIRED_DEBUGGER_TERMS);
}

/**
 * Verifies native TVM image docs and implementation expose debugger metadata.
 *
 * @returns {void}
 *
 * @description
 * Ensures editor debugger work remains grounded in compiler-owned native debug
 * sections without restoring serialized runtime IR.
 */
function testVmImageDebuggerMetadataContract() {
  const imageDoc = readText(VM_IMAGE_DOC);
  const imageSource = readText(VM_IMAGE_SOURCE);
  const buildSource = readText(BUILD_IMAGE_SOURCE);
  const debugSource = readText(NATIVE_DEBUG_SOURCE);

  assertContainsAll(VM_IMAGE_DOC, imageDoc, REQUIRED_IMAGE_DOC_TERMS);
  assert.ok(
    imageSource.includes("descriptor_object_for_native_with_debug"),
    "native image builder must attach debug metadata"
  );
  assert.ok(imageSource.includes(".debug_terlan"), "ELF images must use the native debug section");
  assert.ok(buildSource.includes("encode_native_debug"), "build path must encode native debug metadata");
  assert.ok(debugSource.includes("TVMDBG05"), "native debug records must carry a versioned magic");
}

/**
 * Verifies the editor contract names a shipped command-line debugger fallback.
 *
 * @returns {void}
 *
 * @description
 * Ensures editor debugger work is tied to a real `terlc debug` command while
 * the DAP adapter is still future work.
 */
function testCliDebuggerFallbackContract() {
  const dispatchSource = readText(CLI_DISPATCH_SOURCE);
  const usageSource = readText(CLI_USAGE_SOURCE);
  const debugSource = readText(CLI_DEBUG_SOURCE);
  const debugSessionSource = readText(CLI_DEBUG_SESSION_SOURCE);

  assertContainsAll(CLI_DISPATCH_SOURCE, dispatchSource, ["\"debug\" => commands::debug::run"]);
  assertContainsAll(CLI_USAGE_SOURCE, usageSource, ["terlc debug <image.tvm>"]);
  assertContainsAll(CLI_DEBUG_SOURCE, debugSource, REQUIRED_CLI_DEBUG_TERMS);
  assertContainsAll(CLI_DEBUG_SESSION_SOURCE, debugSessionSource, [
    "PureNativeExecutionShard::load_image",
    "inspect_tvm_native_debug",
    "debug_target_not_native_image",
  ]);
}

/**
 * Verifies VS Code keeps debugging separate from LSP startup until an adapter exists.
 *
 * @returns {void}
 *
 * @description
 * Prevents accidental publication of a half-wired VS Code debug contribution
 * while the checked debugger contract is still adapter-facing.
 */
function testVscodeDoesNotPublishPartialDebuggerContribution() {
  const manifest = readJson(VSCODE_MANIFEST);
  assert.strictEqual(
    manifest.contributes.debuggers,
    undefined,
    "VS Code must not publish a debugger contribution until the adapter exists"
  );
  assert.ok(
    manifest.activationEvents.includes("onLanguage:terlan"),
    "VS Code still needs Terlan language activation"
  );
}

testDebuggerContractDocument();
testVmImageDebuggerMetadataContract();
testCliDebuggerFallbackContract();
testVscodeDoesNotPublishPartialDebuggerContribution();

console.log("editor debugger surface contract is stable");
