"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EXTENSION_ROOT = path.resolve(__dirname, "..");

/**
 * Reads one extension source file.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {string} UTF-8 source text.
 *
 * @description
 * Loads extension runtime source for dependency-free smoke checks without
 * importing VS Code or `vscode-languageclient` packages.
 */
function readSource(relativePath) {
  return fs.readFileSync(path.join(EXTENSION_ROOT, relativePath), "utf8");
}

/**
 * Verifies diagnostics are delegated to the Terlan LSP client.
 *
 * @returns {void}
 *
 * @description
 * Checks the extension entrypoint starts `vscode-languageclient` with the
 * shared client/server options and does not create an independent VS Code
 * diagnostic collection that would duplicate compiler diagnostics.
 */
function testDiagnosticsFlowUsesLanguageClient() {
  const source = readSource("src/extension.js");

  assert.ok(
    source.includes("vscode-languageclient/node"),
    "extension should use vscode-languageclient"
  );
  assert.ok(
    source.includes("createServerOptions"),
    "extension should use shared server options"
  );
  assert.ok(
    source.includes("createClientOptions"),
    "extension should use shared client options"
  );
  assert.ok(source.includes("client.start()"), "extension should start LSP client");
  assert.ok(
    !source.includes("createDiagnosticCollection"),
    "extension must not own a duplicate diagnostics collection"
  );
}

/**
 * Verifies client options keep diagnostics attached to all Terlan language ids.
 *
 * @returns {void}
 *
 * @description
 * Loads the pure client configuration helper and checks the LSP document
 * selector covers every source/template language id contributed by the
 * extension manifest.
 */
function testDiagnosticsDocumentSelectorMatchesManifestLanguages() {
  const manifest = JSON.parse(readSource("package.json"));
  const { createClientOptions } = require("../src/client_config");
  const selectedLanguages = new Set(
    createClientOptions().documentSelector.map((selector) => selector.language)
  );

  for (const language of manifest.contributes.languages) {
    assert.ok(
      selectedLanguages.has(language.id),
      `diagnostics selector missing ${language.id}`
    );
  }
}

testDiagnosticsFlowUsesLanguageClient();
testDiagnosticsDocumentSelectorMatchesManifestLanguages();

console.log("terlan vscode diagnostics smoke tests passed");
