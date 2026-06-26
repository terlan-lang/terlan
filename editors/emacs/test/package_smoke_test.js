"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");

/**
 * Reads one Emacs package text file.
 *
 * @param {string} relativePath Path relative to `editors/emacs`.
 * @returns {string} UTF-8 file contents.
 *
 * @description
 * Loads package files for dependency-free contract checks without launching
 * Emacs or installing editor packages.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(PACKAGE_ROOT, relativePath), "utf8");
}

/**
 * Verifies the expected Emacs package files exist.
 *
 * @returns {void}
 *
 * @description
 * Locks the minimal package layout documented in the 0.0.5 roadmap.
 */
function testExpectedFilesExist() {
  const files = ["README.md", "terlan-mode.el"];

  for (const file of files) {
    assert.ok(fs.existsSync(path.join(PACKAGE_ROOT, file)), `missing ${file}`);
  }
}

/**
 * Verifies Emacs starts the compiler-owned LSP command.
 *
 * @returns {void}
 *
 * @description
 * Ensures both supported Emacs LSP client paths use `terlc lsp --stdio`.
 */
function testLanguageServerCommand() {
  const mode = readText("terlan-mode.el");

  assert.ok(
    mode.includes('("terlc" "lsp" "--stdio")'),
    "Emacs mode must start terlc lsp --stdio"
  );
  assert.ok(mode.includes("eglot-server-programs"), "missing eglot registration");
  assert.ok(mode.includes("lsp-register-client"), "missing lsp-mode registration");
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
  const mode = readText("terlan-mode.el");

  assert.ok(mode.includes('"terlan.toml"'), "missing terlan.toml root marker");
  assert.ok(mode.includes('".git"'), "missing .git root marker");
}

/**
 * Verifies Terlan suffixes are registered.
 *
 * @returns {void}
 *
 * @description
 * Checks source, interface, and template suffix patterns expected by the
 * editor roadmap are present in `auto-mode-alist` registration.
 */
function testFiletypeSuffixes() {
  const mode = readText("terlan-mode.el");
  const suffixes = [
    String.raw`\\.terl\\'`,
    String.raw`\\.terli\\'`,
    String.raw`\\.terl\\.html\\'`,
    String.raw`\\.terl\\.md\\'`,
    String.raw`\\.terl\\.json\\'`,
    String.raw`\\.terl\\.toml\\'`,
    String.raw`\\.terl\\.ya?ml\\'`,
    String.raw`\\.terl\\.txt\\'`,
  ];

  for (const suffix of suffixes) {
    assert.ok(mode.includes(suffix), `missing suffix ${suffix}`);
  }
}

/**
 * Verifies optional Tree-sitter integration stays thin and shared.
 *
 * @returns {void}
 *
 * @description
 * Checks that Emacs can remap to a Tree-sitter-backed mode only when the
 * installed `terlan` grammar is available, without replacing LSP diagnostics.
 */
function testTreeSitterRemap() {
  const mode = readText("terlan-mode.el");

  assert.ok(mode.includes("terlan-enable-treesit"), "missing Tree-sitter toggle");
  assert.ok(
    mode.includes("(defconst terlan-treesit-language 'terlan"),
    "missing canonical Tree-sitter language symbol"
  );
  assert.ok(mode.includes("define-derived-mode terlan-ts-mode"), "missing terlan-ts-mode");
  assert.ok(
    mode.includes("treesit-language-available-p terlan-treesit-language"),
    "Tree-sitter remap must check grammar availability"
  );
  assert.ok(
    mode.includes("(add-to-list 'major-mode-remap-alist '(terlan-mode . terlan-ts-mode))"),
    "missing guarded major-mode remap"
  );
}

/**
 * Verifies generated Emacs package artifacts are absent.
 *
 * @returns {void}
 *
 * @description
 * Keeps the Emacs package source-only until a later release deliberately
 * chooses a packaged artifact format.
 */
function testNoGeneratedPackageArtifacts() {
  const generatedPaths = ["terlan-mode.elc", "dist", "target", ".elpa"];

  for (const generatedPath of generatedPaths) {
    assert.ok(
      !fs.existsSync(path.join(PACKAGE_ROOT, generatedPath)),
      `generated Emacs artifact must not be committed: ${generatedPath}`
    );
  }
}

testExpectedFilesExist();
testLanguageServerCommand();
testRootMarkers();
testFiletypeSuffixes();
testTreeSitterRemap();
testNoGeneratedPackageArtifacts();

console.log("terlan emacs package smoke tests passed");
