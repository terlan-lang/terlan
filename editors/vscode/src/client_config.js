"use strict";

const DOCUMENT_SELECTOR = Object.freeze([
  Object.freeze({ scheme: "file", language: "terlan" }),
  Object.freeze({ scheme: "file", language: "terlan-test" }),
  Object.freeze({ scheme: "file", language: "terlan-interface" }),
  Object.freeze({ scheme: "file", language: "terlan-template-html" }),
  Object.freeze({ scheme: "file", language: "terlan-template-markdown" }),
  Object.freeze({ scheme: "file", language: "terlan-template-json" }),
  Object.freeze({ scheme: "file", language: "terlan-template-toml" }),
  Object.freeze({ scheme: "file", language: "terlan-template-yaml" }),
  Object.freeze({ scheme: "file", language: "terlan-template-text" })
]);

/**
 * Reads one VS Code configuration value with a default fallback.
 *
 * @param {{get?: Function}|undefined} configuration VS Code configuration
 * object returned by `workspace.getConfiguration("terlan")`.
 * @param {string} key Terlan extension configuration key without the `terlan.`
 * prefix.
 * @param {*} fallback Default value used when VS Code returns `undefined`.
 * @returns {*} The configured value or fallback.
 *
 * @description
 * Transforms VS Code's optional configuration lookup into a deterministic value
 * used by the language-client bootstrap path.
 */
function readConfigurationValue(configuration, key, fallback) {
  if (!configuration || typeof configuration.get !== "function") {
    return fallback;
  }
  const value = configuration.get(key, fallback);
  return value === undefined ? fallback : value;
}

/**
 * Returns the first workspace folder path for the LSP process cwd.
 *
 * @param {{workspaceFolders?: Array<{uri?: {fsPath?: string}}>}|undefined}
 * workspace VS Code workspace API object.
 * @returns {string|undefined} First workspace folder filesystem path when one
 * is available.
 *
 * @description
 * Extracts only the stable filesystem path needed by `vscode-languageclient`
 * while leaving single-file editor sessions without a forced cwd.
 */
function firstWorkspaceFolderPath(workspace) {
  return workspace?.workspaceFolders?.[0]?.uri?.fsPath;
}

/**
 * Builds language-server process options for the Terlan extension.
 *
 * @param {{get?: Function}|undefined} configuration Terlan extension
 * configuration object.
 * @param {{workspaceFolders?: Array<{uri?: {fsPath?: string}}>}|undefined}
 * workspace VS Code workspace API object.
 * @returns {{command: string, args: string[], options: {cwd?: string}}}
 * Server options passed to `LanguageClient`.
 *
 * @description
 * Converts user settings into a concrete stdio command. The default is
 * `terlc lsp --stdio`, and the cwd follows the first workspace folder when a
 * folder is open.
 */
function createServerOptions(configuration, workspace) {
  const command = readConfigurationValue(configuration, "lsp.command", "terlc");
  const args = readConfigurationValue(configuration, "lsp.args", [
    "lsp",
    "--stdio"
  ]);

  return {
    command,
    args,
    options: {
      cwd: firstWorkspaceFolderPath(workspace)
    }
  };
}

/**
 * Builds language-client document matching options.
 *
 * @returns {{documentSelector: Array<{scheme: string, language: string}>}}
 * Client options passed to `LanguageClient`.
 *
 * @description
 * Clones the checked-in language selector list so tests and callers cannot
 * mutate the shared selector contract by accident.
 */
function createClientOptions() {
  return {
    documentSelector: DOCUMENT_SELECTOR.map((selector) => ({ ...selector }))
  };
}

module.exports = {
  createClientOptions,
  createServerOptions,
  firstWorkspaceFolderPath,
  readConfigurationValue
};
