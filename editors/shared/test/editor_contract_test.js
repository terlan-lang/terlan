"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EDITORS_ROOT = path.resolve(__dirname, "..", "..");
const REPO_ROOT = path.resolve(EDITORS_ROOT, "..");

const CANONICAL_SUFFIXES = Object.freeze([
  ".terl",
  ".terli",
  ".terl.html",
  ".terl.md",
  ".terl.json",
  ".terl.toml",
  ".terl.yaml",
  ".terl.yml",
  ".terl.txt",
]);

const CANONICAL_LANGUAGE_IDS = Object.freeze([
  "terlan",
  "terlan-interface",
  "terlan-template-html",
  "terlan-template-markdown",
  "terlan-template-json",
  "terlan-template-toml",
  "terlan-template-yaml",
  "terlan-template-text",
]);

const VSCODE_LANGUAGE_IDS = Object.freeze([
  ...CANONICAL_LANGUAGE_IDS,
  "terlan-test",
]);

const CANONICAL_LSP_COMMAND = Object.freeze(["terlc", "lsp", "--stdio"]);

/**
 * Reads one repository-relative text file.
 *
 * @param {string} relativePath Path relative to the Terlan repository root.
 * @returns {string} UTF-8 file contents.
 *
 * @description
 * Loads editor package files for dependency-free contract checks without
 * invoking any editor runtime.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
}

/**
 * Reads and parses one repository-relative JSON file.
 *
 * @param {string} relativePath Path relative to the Terlan repository root.
 * @returns {*} Parsed JSON value.
 *
 * @description
 * Converts checked-in editor metadata into ordinary JavaScript values so the
 * shared contract can compare package surfaces directly.
 */
function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

/**
 * Returns a sorted copy of string values.
 *
 * @param {Iterable<string>} values Values to normalize.
 * @returns {string[]} Sorted unique string values.
 *
 * @description
 * Converts package-specific filetype lists into stable arrays before comparing
 * them with the canonical editor suffix set.
 */
function sortedUnique(values) {
  return Array.from(new Set(values)).sort();
}

/**
 * Asserts that two string collections contain the same members.
 *
 * @param {string} label Human-readable collection label for assertion output.
 * @param {Iterable<string>} actual Actual values collected from a package.
 * @param {Iterable<string>} expected Expected canonical values.
 * @returns {void}
 *
 * @description
 * Normalizes order and duplicates so package metadata can choose its own
 * ordering while still staying aligned with the shared editor contract.
 */
function assertSameSet(label, actual, expected) {
  assert.deepStrictEqual(
    sortedUnique(actual),
    sortedUnique(expected),
    `${label} drifted from shared editor contract`
  );
}

/**
 * Asserts editor startup code does not reference an alternate LSP binary.
 *
 * @param {string} label Human-readable package label for assertion output.
 * @param {string} source Editor startup source text.
 * @returns {void}
 *
 * @description
 * Keeps editor integrations on the compiler-owned `terlc lsp --stdio`
 * deployment path instead of introducing a second default `terlan-lsp` binary.
 */
function assertNoSeparateLspBinary(label, source) {
  assert.ok(
    !/(^|[\s"'`])terlan-lsp(?=$|[\s"'`])/.test(source),
    `${label} must not prefer terlan-lsp`
  );
}

/**
 * Extracts Kotlin string literal values from a list declaration.
 *
 * @param {string} source Kotlin source text.
 * @param {string} propertyName Kotlin property name to read.
 * @returns {string[]} String literal values inside the property's `listOf`.
 *
 * @description
 * Reads simple declarative Kotlin metadata without compiling the IntelliJ
 * plugin, keeping the shared editor contract dependency-free.
 */
function extractKotlinList(source, propertyName) {
  const listMatch = source.match(
    new RegExp(`${propertyName}:\\s*List<String>\\s*=\\s*listOf\\(([\\s\\S]*?)\\)`)
  );
  assert.ok(listMatch, `missing Kotlin list ${propertyName}`);
  return Array.from(listMatch[1].matchAll(/"([^"]+)"/g), (match) => match[1]);
}

/**
 * Verifies VS Code language and suffix metadata.
 *
 * @returns {void}
 *
 * @description
 * Checks that VS Code contributes exactly the shared Terlan language ids and
 * source/interface/template suffixes.
 */
function testVscodeLanguageContract() {
  const manifest = readJson("editors/vscode/package.json");
  const languages = manifest.contributes.languages;
  const languageIds = languages.map((language) => language.id);
  const suffixes = languages.flatMap((language) => language.extensions || []);
  const testLanguage = languages.find((language) => language.id === "terlan-test");

  assertSameSet("VS Code language ids", languageIds, VSCODE_LANGUAGE_IDS);
  assertSameSet("VS Code suffixes", suffixes, CANONICAL_SUFFIXES);
  assert.ok(testLanguage, "VS Code missing Terlan test language");
  assert.deepStrictEqual(testLanguage.filenamePatterns, ["*Test.terl"]);
}

/**
 * Verifies VS Code default LSP startup metadata.
 *
 * @returns {void}
 *
 * @description
 * Checks VS Code's package defaults and startup helper stay on the shared
 * compiler-owned `terlc lsp --stdio` command.
 */
function testVscodeLspContract() {
  const manifest = readJson("editors/vscode/package.json");
  const config = manifest.contributes.configuration.properties;
  const clientConfig = readText("editors/vscode/src/client_config.js");
  const extension = readText("editors/vscode/src/extension.js");

  assert.deepStrictEqual(
    [config["terlan.lsp.command"].default, ...config["terlan.lsp.args"].default],
    CANONICAL_LSP_COMMAND
  );
  assertNoSeparateLspBinary("VS Code client config", clientConfig);
  assertNoSeparateLspBinary("VS Code extension", extension);
}

/**
 * Verifies Tree-sitter package suffix metadata.
 *
 * @returns {void}
 *
 * @description
 * Checks that the parser package advertises the same suffix family consumed by
 * editor integrations.
 */
function testTreeSitterSuffixContract() {
  const manifest = readJson("tree-sitter-terlan/package.json");
  const grammar = manifest["tree-sitter"][0];
  const suffixes = grammar["file-types"].map((suffix) => `.${suffix}`);

  assertSameSet("Tree-sitter suffixes", suffixes, CANONICAL_SUFFIXES);
  assert.strictEqual(grammar.injections, "queries/injections.scm");
}

/**
 * Verifies Neovim language ids, suffixes, and LSP command metadata.
 *
 * @returns {void}
 *
 * @description
 * Locks the Neovim package to the shared suffix family and compiler-owned LSP
 * command without launching Neovim.
 */
function testNeovimEditorContract() {
  const ftdetect = readText("editors/neovim/ftdetect/terlan.lua");
  const helper = readText("editors/neovim/lua/terlan_lsp.lua");

  for (const suffix of CANONICAL_SUFFIXES) {
    assert.ok(ftdetect.includes(`*${suffix}`), `Neovim missing suffix ${suffix}`);
  }
  for (const languageId of CANONICAL_LANGUAGE_IDS) {
    assert.ok(helper.includes(`"${languageId}"`), `Neovim missing language id ${languageId}`);
  }
  assert.ok(helper.includes(`{ "${CANONICAL_LSP_COMMAND.join('", "')}" }`));
  assert.ok(helper.includes('{ "terlan.toml", ".git" }'));
  assertNoSeparateLspBinary("Neovim helper", helper);
}

/**
 * Verifies Emacs suffix and LSP command metadata.
 *
 * @returns {void}
 *
 * @description
 * Checks the Emacs package stays on the same compiler-owned LSP endpoint and
 * registers the complete Terlan suffix family.
 */
function testEmacsEditorContract() {
  const mode = readText("editors/emacs/terlan-mode.el");
  const suffixPatterns = [
    String.raw`\\.terl\\'`,
    String.raw`\\.terli\\'`,
    String.raw`\\.terl\\.html\\'`,
    String.raw`\\.terl\\.md\\'`,
    String.raw`\\.terl\\.json\\'`,
    String.raw`\\.terl\\.toml\\'`,
    String.raw`\\.terl\\.ya?ml\\'`,
    String.raw`\\.terl\\.txt\\'`,
  ];

  for (const pattern of suffixPatterns) {
    assert.ok(mode.includes(pattern), `Emacs missing suffix pattern ${pattern}`);
  }
  assert.ok(mode.includes(`("${CANONICAL_LSP_COMMAND.join('" "')}")`));
  assert.ok(mode.includes("'(\"terlan.toml\" \".git\")"));
  assertNoSeparateLspBinary("Emacs mode", mode);
}

/**
 * Verifies IntelliJ suffix and LSP command metadata.
 *
 * @returns {void}
 *
 * @description
 * Reads the declarative Kotlin package contract and checks it matches the
 * shared editor suffix and root-marker model.
 */
function testIntellijEditorContract() {
  const fileTypes = readText("editors/intellij/src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt");
  const descriptor = readText("editors/intellij/src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt");

  assertSameSet(
    "IntelliJ suffixes",
    extractKotlinList(fileTypes, "suffixes"),
    CANONICAL_SUFFIXES
  );
  assertSameSet(
    "IntelliJ root markers",
    extractKotlinList(descriptor, "rootMarkers"),
    ["terlan.toml", ".git"]
  );
  assert.deepStrictEqual(
    extractKotlinList(descriptor, "command"),
    CANONICAL_LSP_COMMAND
  );
  assertNoSeparateLspBinary("IntelliJ descriptor", descriptor);
}

testVscodeLanguageContract();
testVscodeLspContract();
testTreeSitterSuffixContract();
testNeovimEditorContract();
testEmacsEditorContract();
testIntellijEditorContract();

console.log("terlan shared editor contract tests passed");
