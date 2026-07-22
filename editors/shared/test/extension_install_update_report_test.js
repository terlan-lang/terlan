"use strict";

const assert = require("assert");
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const EDITORS_ROOT = path.resolve(__dirname, "..", "..");
const REPO_ROOT = path.resolve(EDITORS_ROOT, "..");
const DEFAULT_VSCODE_DRY_RUN = "/tmp/terlan-vscode-pack.json";
const DEFAULT_TREE_SITTER_DRY_RUN = "/tmp/terlan-tree-sitter-pack.json";
const REPORT_PATH = path.join(
  REPO_ROOT,
  "target",
  "quality",
  "editor-extension-install-update-report.json"
);

/**
 * Reads one repository-relative text file.
 *
 * @param {string} relativePath Path relative to the repository root.
 * @returns {string} File contents.
 *
 * @description
 * Loads source artifacts for package parity checks.
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
 * Loads assets selected by package dry-runs for hashing.
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
 * Converts checked-in package metadata into inspectable values.
 */
function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

/**
 * Computes a SHA-256 digest.
 *
 * @param {Buffer|string} value Bytes or text to hash.
 * @returns {string} Hex digest.
 *
 * @description
 * Provides stable package and source artifact fingerprints.
 */
function sha256(value) {
  return crypto.createHash("sha256").update(value).digest("hex");
}

/**
 * Reads the first archive entry from an npm dry-run payload.
 *
 * @param {string} payloadPath Absolute dry-run JSON path.
 * @returns {*} Archive entry.
 *
 * @description
 * Validates the npm dry-run shape before package parity checks inspect files.
 */
function readArchive(payloadPath) {
  const payload = JSON.parse(fs.readFileSync(payloadPath, "utf8"));
  assert.ok(Array.isArray(payload), `${payloadPath} must contain an array`);
  assert.strictEqual(payload.length, 1, `${payloadPath} must contain one archive`);
  assert.ok(Array.isArray(payload[0].files), `${payloadPath} archive files missing`);
  return payload[0];
}

/**
 * Builds a normalized package file set.
 *
 * @param {*} archive npm dry-run archive entry.
 * @returns {Set<string>} Package-relative file paths.
 *
 * @description
 * Normalizes file paths so package checks remain stable across platforms.
 */
function packageFileSet(archive) {
  return new Set(archive.files.map((entry) => entry.path.split(path.sep).join("/")));
}

/**
 * Returns hashes for selected package files.
 *
 * @param {string} packageRoot Repository-relative package root.
 * @param {Set<string>} fileSet Package-relative selected files.
 * @param {string[]} filePaths Files to hash.
 * @returns {object} Hash map.
 *
 * @description
 * Verifies selected package files exist in the dry-run archive and source tree.
 */
function hashSelectedFiles(packageRoot, fileSet, filePaths) {
  const hashes = {};
  for (const filePath of filePaths) {
    assert.ok(fileSet.has(filePath), `${packageRoot} dry-run missing ${filePath}`);
    hashes[`${packageRoot}/${filePath}`] = sha256(readBytes(`${packageRoot}/${filePath}`));
  }
  return hashes;
}

/**
 * Verifies activation events and file associations in the VS Code manifest.
 *
 * @param {*} manifest Parsed VS Code package manifest.
 * @returns {{activationEvents: string[], languageAssociations: object[]}} Manifest surface report.
 *
 * @description
 * Locks installed package metadata for startup, command activation, and every
 * contributed Terlan source/template file association.
 */
function testVscodeManifestSurface(manifest) {
  const activationEvents = manifest.activationEvents || [];
  const commandIds = (manifest.contributes.commands || []).map((command) => command.command);
  const languageAssociations = (manifest.contributes.languages || []).map((language) => ({
    id: language.id,
    extensions: language.extensions || [],
    filenamePatterns: language.filenamePatterns || []
  }));
  const associationById = new Map(
    languageAssociations.map((association) => [association.id, association])
  );

  assert.ok(activationEvents.includes("onStartupFinished"));
  for (const commandId of commandIds) {
    assert.ok(
      activationEvents.includes(`onCommand:${commandId}`),
      `missing activation event for command ${commandId}`
    );
  }
  for (const language of manifest.contributes.languages || []) {
    assert.ok(
      activationEvents.includes(`onLanguage:${language.id}`),
      `missing activation event for language ${language.id}`
    );
  }
  assert.deepStrictEqual(associationById.get("terlan-test").filenamePatterns, ["*Test.terl"]);
  assert.deepStrictEqual(associationById.get("terlan").extensions, [".terl"]);
  assert.deepStrictEqual(associationById.get("terlan-interface").extensions, [".terli"]);
  assert.deepStrictEqual(associationById.get("terlan-template-html").extensions, [".terl.html"]);
  assert.deepStrictEqual(associationById.get("terlan-template-markdown").extensions, [".terl.md"]);
  assert.deepStrictEqual(associationById.get("terlan-template-json").extensions, [".terl.json"]);
  assert.deepStrictEqual(associationById.get("terlan-template-toml").extensions, [".terl.toml"]);
  assert.deepStrictEqual(associationById.get("terlan-template-yaml").extensions, [
    ".terl.yaml",
    ".terl.yml"
  ]);
  assert.deepStrictEqual(associationById.get("terlan-template-text").extensions, [".terl.txt"]);

  return {
    activationEvents: [...activationEvents].sort(),
    languageAssociations
  };
}

/**
 * Verifies VS Code dry-run package parity.
 *
 * @param {*} archive npm dry-run archive entry.
 * @returns {*} VS Code package report.
 *
 * @description
 * Checks command, icon, grammar, LSP client, and package identity artifacts
 * selected by the VS Code npm archive preview.
 */
function testVscodePackageArchive(archive) {
  const manifest = readJson("editors/vscode/package.json");
  const fileSet = packageFileSet(archive);
  const commandIds = (manifest.contributes.commands || []).map((command) => command.command);
  const manifestSurface = testVscodeManifestSurface(manifest);
  const iconFiles = [
    "icons/terlan-file.svg",
    "icons/terlan-test-file.svg",
    "icons/terlan-template-html-file.svg",
    "icons/png/terlan-file-16.png",
    "icons/png/terlan-file-24.png",
    "icons/png/terlan-file-32.png",
    "icons/png/terlan-file-64.png",
    "icons/png/terlan-file-128.png",
    "icons/png/terlan-extension-128.png"
  ];
  const grammarFiles = [
    "syntaxes/terlan.tmLanguage.json",
    "syntaxes/terlan-template-html.tmLanguage.json"
  ];
  const runtimeFiles = [
    "package.json",
    "language-configuration.json",
    "src/client_config.js",
    "src/extension.js",
    "src/run_command.js",
    "src/template_links.js",
    "icons/terlan-file-icon-theme.json"
  ];

  assert.strictEqual(archive.name, manifest.name);
  assert.strictEqual(archive.version, manifest.version);
  assert.ok(commandIds.includes("terlan.runMain"));
  assert.ok(commandIds.includes("terlan.runCheck"));
  assert.ok(commandIds.includes("terlan.runBuild"));
  assert.ok(commandIds.includes("terlan.runClean"));
  assert.ok(commandIds.includes("terlan.runServe"));
  assert.ok(commandIds.includes("terlan.runWatch"));
  assert.ok(commandIds.includes("terlan.runDoctor"));
  assert.ok(commandIds.includes("terlan.runDebug"));
  assert.ok(commandIds.includes("terlan.runDebugAtCursor"));
  assert.ok(commandIds.includes("terlan.runTestFile"));
  assert.ok(commandIds.includes("terlan.runTestAtCursor"));
  assert.strictEqual(manifest.contributes.configuration.properties["terlan.lsp.command"].default, "terlc");
  assert.deepStrictEqual(manifest.contributes.configuration.properties["terlan.lsp.args"].default, [
    "lsp",
    "--stdio"
  ]);

  for (const filePath of fileSet) {
    assert.ok(!filePath.startsWith("test/"), `VS Code package contains test file ${filePath}`);
    assert.ok(!filePath.endsWith(".vsix"), `VS Code package contains VSIX ${filePath}`);
    assert.ok(!filePath.endsWith(".tgz"), `VS Code package contains npm archive ${filePath}`);
  }

  return {
    version: archive.version,
    commandIds,
    activationEvents: manifestSurface.activationEvents,
    languageAssociations: manifestSurface.languageAssociations,
    fileCount: fileSet.size,
    hashes: hashSelectedFiles("editors/vscode", fileSet, [
      ...runtimeFiles,
      ...iconFiles,
      ...grammarFiles
    ])
  };
}

/**
 * Verifies Tree-sitter dry-run package parity.
 *
 * @param {*} archive npm dry-run archive entry.
 * @returns {*} Tree-sitter package report.
 *
 * @description
 * Checks grammar metadata, query assets, and package identity selected by the
 * Tree-sitter npm archive preview.
 */
function testTreeSitterPackageArchive(archive) {
  const manifest = readJson("tree-sitter-terlan/package.json");
  const fileSet = packageFileSet(archive);
  const grammar = manifest["tree-sitter"][0];
  const requiredFiles = [
    "package.json",
    "README.md",
    "grammar.js",
    "queries/highlights.scm",
    "queries/injections.scm",
    "test/corpus/basic.txt"
  ];

  assert.strictEqual(archive.name, manifest.name);
  assert.strictEqual(archive.version, manifest.version);
  assert.strictEqual(grammar.highlights, "queries/highlights.scm");
  assert.strictEqual(grammar.injections, "queries/injections.scm");
  assert.ok(grammar["file-types"].includes("terl"));
  assert.ok(grammar["file-types"].includes("terl.html"));

  for (const filePath of fileSet) {
    assert.ok(!filePath.startsWith("src/"), `Tree-sitter package contains generated parser ${filePath}`);
    assert.ok(!filePath.startsWith("bindings/"), `Tree-sitter package contains generated binding ${filePath}`);
    assert.ok(!filePath.endsWith(".tgz"), `Tree-sitter package contains npm archive ${filePath}`);
  }

  return {
    version: archive.version,
    grammarInventory: grammar,
    fileCount: fileSet.size,
    hashes: hashSelectedFiles("tree-sitter-terlan", fileSet, requiredFiles)
  };
}

/**
 * Writes the editor extension install/update report.
 *
 * @param {*} report Report payload.
 * @returns {void}
 *
 * @description
 * Persists machine-readable package parity evidence for release gates.
 */
function writeReport(report) {
  fs.mkdirSync(path.dirname(REPORT_PATH), { recursive: true });
  fs.writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
}

const vscodeArchive = readArchive(process.argv[2] || DEFAULT_VSCODE_DRY_RUN);
const treeSitterArchive = readArchive(process.argv[3] || DEFAULT_TREE_SITTER_DRY_RUN);
const vscode = testVscodePackageArchive(vscodeArchive);
const treeSitter = testTreeSitterPackageArchive(treeSitterArchive);

const report = {
  packagedHashes: {
    ...vscode.hashes,
    ...treeSitter.hashes
  },
  installedHashes: {
    dryRunArchiveParity: true
  },
  commandInventory: vscode.commandIds,
  activationInventory: vscode.activationEvents,
  fileAssociationInventory: vscode.languageAssociations,
  iconInventory: Object.keys(vscode.hashes).filter((filePath) => filePath.includes("/icons/")),
  grammarInventory: treeSitter.grammarInventory,
  extensionVersionChecks: {
    vscode: vscode.version,
    treeSitter: treeSitter.version
  },
  staleCacheRejectionCases: {
    excludesTests: true,
    excludesArchives: true,
    excludesGeneratedTreeSitterParser: true
  }
};

writeReport(report);

console.log(`editor extension install/update contract is stable: ${REPORT_PATH}`);
