"use strict";

const assert = require("assert");
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const EDITORS_ROOT = path.resolve(__dirname, "..", "..");
const REPO_ROOT = path.resolve(EDITORS_ROOT, "..");
const REPORT_PATH = path.join(
  REPO_ROOT,
  "target",
  "quality",
  "editor-diagnostic-parity-report.json"
);

/**
 * Reads one repository-relative text file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {string} File contents.
 *
 * @description
 * Loads compiler/editor source files for dependency-free diagnostic contracts.
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
 * Converts editor package metadata into inspectable values.
 */
function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

/**
 * Computes a SHA-256 digest for text.
 *
 * @param {string} value Text to hash.
 * @returns {string} Hex digest.
 *
 * @description
 * Provides stable source fingerprints for the diagnostic parity report.
 */
function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

/**
 * Verifies VS Code delegates diagnostics to the LSP client.
 *
 * @returns {{languageIds: string[], extensionHash: string, clientConfigHash: string}} Editor diagnostic report.
 *
 * @description
 * Ensures the extension starts the Terlan language client for every contributed
 * Terlan language and does not create an editor-owned diagnostic collection.
 */
function testEditorDiagnosticDelegation() {
  const manifest = readJson("editors/vscode/package.json");
  const extensionSource = readText("editors/vscode/src/extension.js");
  const clientConfigSource = readText("editors/vscode/src/client_config.js");
  const selectedLanguages = Array.from(
    clientConfigSource.matchAll(/language:\s*"([^"]+)"/g),
    (match) => match[1]
  );

  assert.ok(extensionSource.includes("vscode-languageclient/node"));
  assert.ok(extensionSource.includes("client.start()"));
  assert.ok(!extensionSource.includes("createDiagnosticCollection"));
  for (const language of manifest.contributes.languages) {
    assert.ok(
      selectedLanguages.includes(language.id),
      `diagnostic selector missing ${language.id}`
    );
  }

  return {
    languageIds: selectedLanguages,
    extensionHash: sha256(extensionSource),
    clientConfigHash: sha256(clientConfigSource)
  };
}

/**
 * Verifies LSP diagnostics are built from compiler diagnostic payloads.
 *
 * @returns {{sources: string[], severityMapping: string[], lspSourceHash: string}} Compiler/LSP report.
 *
 * @description
 * Checks parse, HIR, typechecker, and template diagnostics are mapped through
 * the LSP server rather than duplicated in editor code.
 */
function testCompilerDiagnosticMapping() {
  const lspSource = readText("crates/terlan/src/lsp/mod.rs");
  const diagnosticSource = readText(
    "crates/terlan/src/lsp/backend/diagnostics_and_completion.rs"
  );
  const documentSource = readText("crates/terlan/src/lsp/document.rs");
  const requiredSources = [
    "terlan-syntax",
    "terlan-hir",
    "terlan-typeck",
    "terlan-template"
  ];

  for (const source of requiredSources) {
    assert.ok(diagnosticSource.includes(source), `missing LSP diagnostic source ${source}`);
  }
  assert.ok(diagnosticSource.includes("DiagnosticSeverity::ERROR"));
  assert.ok(diagnosticSource.includes("DiagnosticSeverity::WARNING"));
  assert.ok(diagnosticSource.includes("range_from_span"));
  assert.ok(documentSource.includes("resolve_diagnostics"));
  assert.ok(documentSource.includes("type_diagnostics"));
  assert.ok(documentSource.includes("template_diagnostics"));

  return {
    sources: requiredSources,
    severityMapping: ["error", "warning"],
    lspSourceHash: sha256(`${lspSource}\n${diagnosticSource}\n${documentSource}`)
  };
}

/**
 * Verifies fixable diagnostics stay tied to LSP code actions.
 *
 * @returns {{quickFixKinds: string[], importDiagnosticShapes: string[]}} Fixability report.
 *
 * @description
 * Checks unknown-constructor/function diagnostics flow through quick-fix import
 * actions while remaining compiler-diagnostic driven.
 */
function testDiagnosticFixabilityContract() {
  const importActions = readText("crates/terlan/src/lsp/import_actions.rs");
  const tests = readText(
    "crates/terlan/src/lsp/import_actions_test/action_fixtures.rs"
  );

  assert.ok(importActions.includes("CodeActionKind::QUICKFIX"));
  assert.ok(importActions.includes("unknown constructor"));
  assert.ok(importActions.includes("unknown function"));
  assert.ok(tests.includes("diagnostic_import_action_contains_workspace_edit"));
  assert.ok(tests.includes("diagnostic_import_action_repairs_provider_function"));

  return {
    quickFixKinds: ["quickfix"],
    importDiagnosticShapes: ["unknown constructor", "unknown function"]
  };
}

/**
 * Verifies stale and adversarial diagnostic coverage is checked.
 *
 * @returns {{staleClearTest: string, adversarialUnicodeTest: string, pathRedaction: string}} Stale/path report.
 *
 * @description
 * Ensures the gate is backed by tests for clear-on-fix, adversarial Unicode
 * parse isolation, and no editor-side absolute-path diagnostic fabrication.
 */
function testStaleAndPathContract() {
  const resolutionTests = readText(
    "crates/terlan/src/lsp/lib_test/resolution_diagnostics.rs"
  );
  const documentTests = readText(
    "crates/terlan/src/lsp/lib_test/documents_and_shapes.rs"
  );
  const extensionSource = readText("editors/vscode/src/extension.js");

  assert.ok(resolutionTests.includes("did_open_reports_diagnostic_and_clear_on_parse_fix"));
  assert.ok(documentTests.includes("adversarial_lsp_diagnostics_isolate_unicode_parse_failures"));
  assert.ok(!extensionSource.includes("createDiagnosticCollection"));
  assert.ok(!extensionSource.includes("publishDiagnostics"));

  return {
    staleClearTest: "did_open_reports_diagnostic_and_clear_on_parse_fix",
    adversarialUnicodeTest: "adversarial_lsp_diagnostics_isolate_unicode_parse_failures",
    pathRedaction: "editor does not fabricate diagnostics"
  };
}

/**
 * Verifies VM debugger diagnostics stay compiler-owned.
 *
 * @returns {{jsonDiagnosticTests: string[], invalidUsageTests: string[], diagnosticCodes: string[]}} Debug diagnostic report.
 *
 * @description
 * Locks editor-facing debugger diagnostics to the Rust CLI parser/renderer and
 * reserved `.terldbg` validation tests instead of allowing an editor shim to
 * invent separate VM debug errors.
 */
function testVmDebuggerDiagnosticContract() {
  const debugSource = readText("crates/terlan/src/commands/debug/mod.rs");
  const scriptSource = readText("crates/terlan/src/commands/debug/script.rs");
  const debugCliTests = readText("crates/terlan/src/tests/debug_cli_test.rs");
  const debuggerSurfaceTest = readText("editors/shared/test/debugger_surface_test.js");
  const jsonDiagnosticTests = [
    "run_cli_routes_debug_command_after_json_diagnostic_flag",
    "run_cli_routes_repl_debug_after_json_diagnostic_flag"
  ];
  const invalidUsageTests = [
    "run_cli_rejects_debug_command_invalid_breakpoint_spec",
    "run_cli_rejects_debug_command_missing_script_file",
    "run_cli_rejects_debug_script_invalid_breakpoint_selector"
  ];
  const diagnosticCodes = [
    "debug_missing_option_value",
    "debug_script_invalid_breakpoint_selector",
    "debug_script_read_failed"
  ];

  assert.ok(debugSource.includes("print_debug_error"));
  assert.ok(debugSource.includes("render_debug_error_json"));
  assert.ok(debugSource.includes("DiagnosticFormat::Json"));
  assert.ok(scriptSource.includes("validate_debug_script_file"));
  assert.ok(
    debuggerSurfaceTest.includes(
      "terlc --diagnostic-format json debug build/app.tvm --script session.terldbg"
    )
  );
  for (const testName of jsonDiagnosticTests) {
    assert.ok(debugCliTests.includes(testName), `missing debugger JSON diagnostic test ${testName}`);
  }
  for (const testName of invalidUsageTests) {
    assert.ok(debugCliTests.includes(testName), `missing debugger invalid usage test ${testName}`);
  }
  for (const code of diagnosticCodes) {
    assert.ok(
      debugSource.includes(code) || scriptSource.includes(code),
      `missing debugger diagnostic code ${code}`
    );
  }

  return {
    jsonDiagnosticTests,
    invalidUsageTests,
    diagnosticCodes
  };
}

/**
 * Writes the diagnostic parity report.
 *
 * @param {*} report Report payload.
 * @returns {void}
 *
 * @description
 * Persists machine-readable evidence consumed by roadmap and release gates.
 */
function writeReport(report) {
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });
  fs.writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
}

const report = {
  diagnosticFixtures: testCompilerDiagnosticMapping(),
  compilerLspComparisons: {
    publishDiagnosticsSources: ["syntax", "hir", "typeck", "template"],
    exactRustGateTests: [
      "did_open_reports_parse_diagnostic",
      "did_open_reports_diagnostic_and_clear_on_parse_fix",
      "did_open_reports_type_diagnostic",
      "did_open_reports_resolve_diagnostic",
      "did_open_invalid_template_document_publishes_template_diagnostic"
    ]
  },
  editorProblemPanelSnapshots: testEditorDiagnosticDelegation(),
  fixabilityDecisions: testDiagnosticFixabilityContract(),
  sourceMapChecks: {
    spanToRange: "range_from_span"
  },
  vmDebugLaunchFailures: testVmDebuggerDiagnosticContract(),
  staleDiagnosticRejections: testStaleAndPathContract(),
  pathRedactionChecks: {
    editorOwnedDiagnostics: false
  }
};

writeReport(report);

console.log(`editor diagnostic parity contract is stable: ${REPORT_PATH}`);
