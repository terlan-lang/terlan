"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");

/**
 * Reads one Neovim package text file.
 *
 * @param {string} relativePath Path relative to `editors/neovim`.
 * @returns {string} UTF-8 file contents.
 *
 * @description
 * Loads Lua and README files so smoke checks can validate the package contract
 * without launching Neovim.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(PACKAGE_ROOT, relativePath), "utf8");
}

/**
 * Verifies the expected Neovim package files exist.
 *
 * @returns {void}
 *
 * @description
 * Locks the minimal package layout documented in the 0.0.5 roadmap.
 */
function testExpectedFilesExist() {
  const files = [
    "README.md",
    "ftdetect/terlan.lua",
    "ftplugin/terlan.lua",
    "lua/terlan_lsp.lua",
  ];

  for (const file of files) {
    assert.ok(fs.existsSync(path.join(PACKAGE_ROOT, file)), `missing ${file}`);
  }
}

/**
 * Verifies Neovim starts the compiler-owned LSP command.
 *
 * @returns {void}
 *
 * @description
 * Ensures the plugin uses `terlc lsp --stdio` and does not introduce a second
 * language-server binary or daemon command.
 */
function testLanguageServerCommand() {
  const helper = readText("lua/terlan_lsp.lua");

  assert.ok(
    helper.includes('{ "terlc", "lsp", "--stdio" }'),
    "Neovim helper must start terlc lsp --stdio"
  );
  assert.ok(!helper.includes("terlan-lsp"), "Neovim helper must not prefer terlan-lsp");
}

/**
 * Verifies project root markers are stable.
 *
 * @returns {void}
 *
 * @description
 * Locks `terlan.toml` as the primary project-root marker with `.git` fallback.
 */
function testRootMarkers() {
  const helper = readText("lua/terlan_lsp.lua");

  assert.ok(helper.includes('"terlan.toml"'), "missing terlan.toml root marker");
  assert.ok(helper.includes('".git"'), "missing .git root marker");
}

/**
 * Verifies Terlan suffixes are registered.
 *
 * @returns {void}
 *
 * @description
 * Checks source, interface, and template suffixes expected by the editor
 * roadmap are present in filetype detection.
 */
function testFiletypeSuffixes() {
  const ftdetect = readText("ftdetect/terlan.lua");
  const suffixes = [
    "*.terl",
    "*.terli",
    "*.terl.html",
    "*.terl.md",
    "*.terl.json",
    "*.terl.toml",
    "*.terl.yaml",
    "*.terl.yml",
    "*.terl.txt",
  ];

  for (const suffix of suffixes) {
    assert.ok(ftdetect.includes(suffix), `missing suffix ${suffix}`);
  }
}

/**
 * Verifies Neovim can reuse the shared Tree-sitter parser package.
 *
 * @returns {void}
 *
 * @description
 * Checks that the integration maps all Terlan source/interface/template
 * filetypes to the canonical `terlan` Tree-sitter language when the host API is
 * available, without replacing the compiler-owned LSP path.
 */
function testTreeSitterRegistration() {
  const helper = readText("lua/terlan_lsp.lua");

  assert.ok(
    helper.includes('M.tree_sitter_language = "terlan"'),
    "missing canonical Tree-sitter language name"
  );
  assert.ok(
    helper.includes("vim.treesitter.language.register"),
    "missing Tree-sitter language registration"
  );
  assert.ok(
    helper.includes("vim.treesitter.language.register(M.tree_sitter_language, M.filetypes)"),
    "Tree-sitter registration must cover every Terlan filetype"
  );
}

/**
 * Verifies generated Neovim plugin artifacts are not committed.
 *
 * @returns {void}
 *
 * @description
 * Keeps the Neovim package source-only until a later release deliberately
 * chooses a packaged artifact format.
 */
function testNoGeneratedPluginArtifacts() {
  const generatedPaths = ["node_modules", "dist", "target", ".luarocks"];

  for (const generatedPath of generatedPaths) {
    assert.ok(
      !fs.existsSync(path.join(PACKAGE_ROOT, generatedPath)),
      `generated Neovim artifact must not be committed: ${generatedPath}`
    );
  }
}

testExpectedFilesExist();
testLanguageServerCommand();
testRootMarkers();
testFiletypeSuffixes();
testTreeSitterRegistration();
testNoGeneratedPluginArtifacts();

console.log("terlan neovim package smoke tests passed");
