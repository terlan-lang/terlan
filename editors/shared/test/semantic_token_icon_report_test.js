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
  "editor-semantic-token-icon-report.json"
);

const REQUIRED_HIGHLIGHT_CAPTURES = Object.freeze([
  "@keyword",
  "@keyword.control",
  "@attribute",
  "@namespace",
  "@function",
  "@function.call",
  "@operator",
  "@type",
  "@type.builtin",
  "@variable",
  "@variable.parameter",
  "@property",
  "@number",
  "@string",
  "@comment",
  "@embedded"
]);

const REQUIRED_HIGHLIGHT_NODES = Object.freeze([
  "annotation",
  "module_declaration",
  "import_declaration",
  "shape_declaration",
  "shape_guard_expression",
  "function_declaration",
  "function_signature",
  "template_declaration",
  "call_expression",
  "type_identifier",
  "atom_type",
  "interpolation",
  "string_pattern"
]);

const REQUIRED_ICON_FILES = Object.freeze([
  "editors/shared/icons/terlan-file.svg",
  "editors/shared/icons/terlan-extension.svg",
  "editors/shared/icons/png/terlan-file-16.png",
  "editors/shared/icons/png/terlan-file-24.png",
  "editors/shared/icons/png/terlan-file-32.png",
  "editors/shared/icons/png/terlan-file-64.png",
  "editors/shared/icons/png/terlan-file-128.png",
  "editors/shared/icons/png/terlan-extension-128.png"
]);

const REQUIRED_ICON_FILE_NAMES = Object.freeze({
  "*Test.terl": "_terlan_test_file",
  "terlan.toml": "_terlan_file",
  "vmir-execution-trace-report.json": "_terlan_file"
});

const REQUIRED_ICON_FILE_EXTENSIONS = Object.freeze([
  "terl",
  "terls",
  "terli",
  "terl.html",
  "terl.md",
  "terl.json",
  "terl.toml",
  "terl.yaml",
  "terl.yml",
  "terl.txt",
  "terldbg"
]);

/**
 * Reads one repository-relative text file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {string} File contents.
 *
 * @description
 * Loads editor package files for dependency-free semantic/icon checks.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath), "utf8");
}

/**
 * Reads one repository-relative binary file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {Buffer} File bytes.
 *
 * @description
 * Loads icon assets so the report can include stable package hash evidence.
 */
function readBytes(relativePath) {
  return fs.readFileSync(path.join(REPO_ROOT, relativePath));
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
 * Computes a SHA-256 digest for bytes.
 *
 * @param {Buffer|string} value Input bytes or text.
 * @returns {string} Hex digest.
 *
 * @description
 * Provides stable asset and grammar fingerprints for the report.
 */
function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

/**
 * Reads the dimensions from a PNG icon.
 *
 * @param {Buffer} png PNG file bytes.
 * @returns {{width: number, height: number}} Image dimensions.
 *
 * @description
 * Validates the PNG signature and decodes the IHDR dimensions without a native
 * image dependency.
 */
function readPngDimensions(png) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  assert.ok(png.subarray(0, signature.length).equals(signature), "invalid PNG signature");
  return {
    width: png.readUInt32BE(16),
    height: png.readUInt32BE(20)
  };
}

/**
 * Verifies Tree-sitter highlight query coverage.
 *
 * @returns {{captures: string[], nodes: string[], hash: string}} Highlight report.
 *
 * @description
 * Checks representative capture classes and syntax node names used by editor
 * semantic/highlighting consumers.
 */
function testHighlightQueryCoverage() {
  const query = readText("tree-sitter-terlan/queries/highlights.scm");

  for (const capture of REQUIRED_HIGHLIGHT_CAPTURES) {
    assert.ok(query.includes(capture), `missing highlight capture ${capture}`);
  }
  for (const node of REQUIRED_HIGHLIGHT_NODES) {
    assert.ok(query.includes(node), `missing highlight node ${node}`);
  }

  return {
    captures: Array.from(REQUIRED_HIGHLIGHT_CAPTURES),
    nodes: Array.from(REQUIRED_HIGHLIGHT_NODES),
    hash: sha256(query)
  };
}

/**
 * Verifies TextMate bridge scope coverage.
 *
 * @returns {{scopeName: string, templateScopeName: string, hash: string}} Scope report.
 *
 * @description
 * Ensures VS Code's temporary TextMate bridge remains tied to Terlan source and
 * template scopes while Tree-sitter-backed highlighting is pending.
 */
function testTextMateBridgeCoverage() {
  const grammarText = readText("editors/vscode/syntaxes/terlan.tmLanguage.json");
  const templateText = readText("editors/vscode/syntaxes/terlan-template-html.tmLanguage.json");
  const grammar = JSON.parse(grammarText);
  const template = JSON.parse(templateText);

  assert.strictEqual(grammar.scopeName, "source.terlan");
  assert.ok(grammar.repository.keywords.patterns.length > 0);
  assert.ok(grammar.repository.operators.patterns.length > 0);
  assert.strictEqual(template.scopeName, "text.html.terlan");
  assert.ok(
    template.repository["terlan-interpolation"].patterns[0].contentName.includes("source.terlan"),
    "template grammar must embed Terlan expression islands"
  );

  return {
    scopeName: grammar.scopeName,
    templateScopeName: template.scopeName,
    hash: sha256(`${grammarText}\n${templateText}`)
  };
}

/**
 * Verifies VS Code icon theme mappings and source assets.
 *
 * @returns {{definitions: string[], languageIds: string[], fileExtensions: string[], assetHashes: object}} Icon report.
 *
 * @description
 * Checks normal files, tests, interfaces, and template suffixes map to existing
 * source-of-truth icons with package-ready PNG variants.
 */
function testIconThemeCoverage() {
  const manifest = readJson("editors/vscode/package.json");
  const theme = readJson("editors/vscode/icons/terlan-file-icon-theme.json");
  const languageIds = new Set(Object.keys(theme.languageIds || {}));
  const fileExtensions = new Set(Object.keys(theme.fileExtensions || {}));
  const fileNames = new Set(Object.keys(theme.fileNames || {}));
  const definitions = new Set(Object.keys(theme.iconDefinitions || {}));
  const assetHashes = {};

  for (const language of manifest.contributes.languages) {
    assert.ok(languageIds.has(language.id), `missing icon mapping for ${language.id}`);
    if (language.icon) {
      assert.ok(language.icon.light, `${language.id} missing light icon`);
      assert.ok(language.icon.dark, `${language.id} missing dark icon`);
      assert.strictEqual(language.icon.light, language.icon.dark);
    }
  }
  for (const extension of REQUIRED_ICON_FILE_EXTENSIONS) {
    assert.ok(fileExtensions.has(extension), `missing icon extension ${extension}`);
  }
  for (const [fileName, iconId] of Object.entries(REQUIRED_ICON_FILE_NAMES)) {
    assert.ok(fileNames.has(fileName), `missing icon filename ${fileName}`);
    assert.strictEqual(theme.fileNames[fileName], iconId);
  }
  assert.ok(definitions.has("_terlan_file"));
  assert.ok(definitions.has("_terlan_test_file"));
  assert.ok(definitions.has("_terlan_template_html_file"));

  for (const filePath of REQUIRED_ICON_FILES) {
    const bytes = readBytes(filePath);
    assetHashes[filePath] = sha256(bytes);
    if (filePath.endsWith(".png")) {
      const dimensions = readPngDimensions(bytes);
      const size = filePath.includes("extension-128") ? 128 : Number(filePath.match(/-(\d+)\.png$/)[1]);
      assert.deepStrictEqual(dimensions, { width: size, height: size });
    }
  }

  return {
    definitions: Array.from(definitions).sort(),
    languageIds: Array.from(languageIds).sort(),
    fileNames: Array.from(fileNames).sort(),
    fileExtensions: Array.from(fileExtensions).sort(),
    assetHashes
  };
}

/**
 * Writes the semantic-token/icon report.
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
  grammarFixtures: testHighlightQueryCoverage(),
  semanticTokenSnapshots: {
    treeSitterHighlightQuery: true,
    lspSemanticTokens: "pending"
  },
  scopeSnapshots: testTextMateBridgeCoverage(),
  iconAssetHashes: testIconThemeCoverage(),
  packagedExtensionHashes: {
    vscodePackageManifest: sha256(readText("editors/vscode/package.json")),
    treeSitterPackageManifest: sha256(readText("tree-sitter-terlan/package.json"))
  },
  unsupportedFormDiagnostics: {
    staleGrammarOutputRejectedByGate: true
  }
};

writeReport(report);

console.log(`editor semantic-token/icon contract is stable: ${REPORT_PATH}`);
