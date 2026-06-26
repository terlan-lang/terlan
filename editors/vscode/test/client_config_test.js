"use strict";

const assert = require("assert");
const {
  createClientOptions,
  createServerOptions,
  firstWorkspaceFolderPath,
  readConfigurationValue
} = require("../src/client_config");

/**
 * Builds a minimal VS Code configuration double for extension tests.
 *
 * @param {Record<string, *>} values Key/value settings returned by the double.
 * @returns {{get: function(string, *): *}} Configuration object with a VS
 * Code-compatible `get` method.
 *
 * @description
 * Transforms a plain object into the narrow configuration API needed by the
 * Terlan extension without loading VS Code.
 */
function configuration(values) {
  return {
    get(key, fallback) {
      return Object.prototype.hasOwnProperty.call(values, key)
        ? values[key]
        : fallback;
    }
  };
}

/**
 * Builds a minimal VS Code workspace double for extension tests.
 *
 * @param {string|undefined} path First workspace folder path.
 * @returns {{workspaceFolders?: Array<{uri: {fsPath: string}}>}|undefined}
 * Workspace object matching the extension's required shape.
 *
 * @description
 * Converts an optional filesystem path into the workspace metadata consumed by
 * language-server process configuration.
 */
function workspace(path) {
  if (!path) {
    return undefined;
  }
  return {
    workspaceFolders: [
      {
        uri: {
          fsPath: path
        }
      }
    ]
  };
}

/**
 * Verifies default server command construction.
 *
 * @returns {void}
 *
 * @description
 * Ensures an unconfigured extension starts `terlc lsp --stdio` and uses the
 * first workspace folder as the server cwd.
 */
function testDefaultServerOptions() {
  const options = createServerOptions(configuration({}), workspace("/repo"));
  assert.strictEqual(options.command, "terlc");
  assert.deepStrictEqual(options.args, ["lsp", "--stdio"]);
  assert.strictEqual(options.options.cwd, "/repo");
}

/**
 * Verifies configured server command construction.
 *
 * @returns {void}
 *
 * @description
 * Ensures user-provided LSP command settings override the default command and
 * argument list without changing workspace cwd behavior.
 */
function testConfiguredServerOptions() {
  const options = createServerOptions(
    configuration({
      "lsp.command": "/tmp/terlc",
      "lsp.args": ["lsp", "--stdio", "--trace"]
    }),
    workspace("/project")
  );
  assert.strictEqual(options.command, "/tmp/terlc");
  assert.deepStrictEqual(options.args, ["lsp", "--stdio", "--trace"]);
  assert.strictEqual(options.options.cwd, "/project");
}

/**
 * Verifies helper fallback behavior.
 *
 * @returns {void}
 *
 * @description
 * Confirms configuration and workspace helpers remain deterministic when VS
 * Code supplies no workspace folder or setting value.
 */
function testFallbackHelpers() {
  assert.strictEqual(
    readConfigurationValue(undefined, "missing", "fallback"),
    "fallback"
  );
  assert.strictEqual(firstWorkspaceFolderPath(undefined), undefined);
  assert.strictEqual(
    firstWorkspaceFolderPath({ workspaceFolders: [] }),
    undefined
  );
}

/**
 * Verifies document selector coverage and cloning.
 *
 * @returns {void}
 *
 * @description
 * Confirms the extension attaches the LSP client to source, interface, and
 * template language IDs, and that each call receives a fresh selector array.
 */
function testClientOptions() {
  const first = createClientOptions();
  const second = createClientOptions();
  const languages = first.documentSelector.map((selector) => selector.language);

  assert.ok(languages.includes("terlan"));
  assert.ok(languages.includes("terlan-interface"));
  assert.ok(languages.includes("terlan-template-html"));
  assert.ok(languages.includes("terlan-template-json"));
  assert.notStrictEqual(first.documentSelector, second.documentSelector);
}

testDefaultServerOptions();
testConfiguredServerOptions();
testFallbackHelpers();
testClientOptions();

console.log("terlan vscode client config tests passed");
