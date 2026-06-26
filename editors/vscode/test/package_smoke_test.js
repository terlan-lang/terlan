"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EXTENSION_ROOT = path.resolve(__dirname, "..");
const CANONICAL_ICON = path.resolve(EXTENSION_ROOT, "..", "shared", "icons", "terlan-file.svg");
const CANONICAL_EXTENSION_ICON = path.resolve(EXTENSION_ROOT, "..", "shared", "icons", "terlan-extension.svg");
const CANONICAL_PNG_ICON_ROOT = path.resolve(EXTENSION_ROOT, "..", "shared", "icons", "png");
const PNG_ICON_SIZES = [16, 24, 32, 64, 128];

/**
 * Reads and parses a JSON file from the extension package.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {*} Parsed JSON value.
 *
 * @description
 * Converts checked-in package metadata into a JavaScript value so packaging
 * tests can validate the selected release surface without invoking VS Code.
 */
function readJson(relativePath) {
  return JSON.parse(
    fs.readFileSync(path.join(EXTENSION_ROOT, relativePath), "utf8")
  );
}

/**
 * Reads one extension package text file.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {string} UTF-8 file text.
 *
 * @description
 * Loads package documentation so smoke tests can lock deployment contracts
 * that users and maintainers rely on outside the JavaScript runtime files.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(EXTENSION_ROOT, relativePath), "utf8");
}

/**
 * Reads one extension package binary file.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {Buffer} Raw file bytes.
 *
 * @description
 * Loads package assets that must be compared byte-for-byte against shared
 * editor assets without text decoding.
 */
function readBinary(relativePath) {
  return fs.readFileSync(path.join(EXTENSION_ROOT, relativePath));
}

/**
 * Resolves an extension-relative path and asserts it exists.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {string} Absolute path for the checked file.
 *
 * @description
 * Gives package metadata checks a small local assertion helper without pulling
 * in VS Code APIs or duplicating package-selection expansion.
 */
function assertPackagePathExists(relativePath) {
  const filePath = path.join(EXTENSION_ROOT, relativePath);
  assert.ok(fs.existsSync(filePath), `expected ${relativePath} to exist`);
  return filePath;
}

/**
 * Recursively collects files under a package-relative path.
 *
 * @param {string} relativePath File or directory path from `package.json`.
 * @returns {string[]} Package-relative file paths.
 *
 * @description
 * Expands manifest `files` entries into concrete checked-in files so the smoke
 * can detect missing runtime assets before a package archive is produced.
 */
function collectPackageFiles(relativePath) {
  const absolutePath = path.join(EXTENSION_ROOT, relativePath);
  assert.ok(fs.existsSync(absolutePath), `missing package path ${relativePath}`);

  const stat = fs.statSync(absolutePath);
  if (stat.isFile()) {
    return [relativePath.split(path.sep).join("/")];
  }

  const files = [];
  for (const entry of fs.readdirSync(absolutePath)) {
    files.push(...collectPackageFiles(path.join(relativePath, entry)));
  }
  return files;
}

/**
 * Returns the selected extension package files from `package.json`.
 *
 * @returns {Set<string>} Package-relative runtime file paths.
 *
 * @description
 * Applies the extension's explicit package `files` list and adds the npm
 * package manifest, which npm includes by default, to model the intended
 * package surface without depending on npm command output.
 */
function selectedPackageFileSet() {
  const manifest = readJson("package.json");
  const fileSet = new Set(["package.json"]);

  for (const entry of manifest.files) {
    for (const filePath of collectPackageFiles(entry)) {
      fileSet.add(filePath);
    }
  }

  return fileSet;
}

/**
 * Verifies release package scripts include deterministic dry-run packaging.
 *
 * @returns {void}
 *
 * @description
 * Locks the local packaging validation command used by `make editor-check`
 * without requiring VS Code host tooling or publishing credentials.
 */
function testPackageScripts() {
  const manifest = readJson("package.json");

  assert.strictEqual(
    manifest.scripts["check:main"],
    "node -c src/client_config.js && node -c src/run_command.js && node -c src/template_links.js && node -c src/extension.js"
  );
  assert.strictEqual(
    manifest.scripts["pack:dry-run"],
    "npm pack --dry-run --json"
  );
  assert.strictEqual(
    manifest.scripts["test:all"],
    "npm run check && npm test"
  );
}

/**
 * Verifies package metadata required by local VSIX and marketplace packaging.
 *
 * @returns {void}
 *
 * @description
 * Locks repository, license, and extension icon metadata so local packaging
 * does not emit avoidable VSCE warnings and installed extensions display the
 * canonical Terlan visual identity.
 */
function testPackageMetadata() {
  const manifest = readJson("package.json");

  assert.strictEqual(manifest.license, "Apache-2.0");
  assert.strictEqual(manifest.icon, "icons/png/terlan-extension-128.png");
  assert.strictEqual(manifest.repository.type, "git");
  assert.strictEqual(
    manifest.repository.url,
    "https://github.com/terlan-lang/terlan.git"
  );
  assertPackagePathExists(manifest.icon);
}

/**
 * Verifies every Terlan language contribution carries a file icon.
 *
 * @returns {void}
 *
 * @description
 * Locks the extension contract that `.terl`, `.terli`, and template source
 * files expose the canonical Terlan icon to VS Code and compatible file-icon
 * themes.
 */
function testLanguageContributionsDeclareTerlanFileIcon() {
  const manifest = readJson("package.json");

  for (const language of manifest.contributes.languages) {
    const icon = language.id === "terlan-test"
      ? {
          light: "./icons/terlan-test-file.svg",
          dark: "./icons/terlan-test-file.svg"
        }
      : language.id === "terlan-template-html"
        ? {
            light: "./icons/terlan-template-html-file.svg",
            dark: "./icons/terlan-template-html-file.svg"
          }
        : {
            light: "./icons/terlan-file.svg",
            dark: "./icons/terlan-file.svg"
          };
    assert.deepStrictEqual(
      language.icon,
      icon,
      `${language.id} must declare Terlan file icons`
    );
  }
}

/**
 * Verifies the extension documents the compiler-owned LSP deployment path.
 *
 * @returns {void}
 *
 * @description
 * Locks the 0.0.5 decision that the VS Code package launches the installed
 * `terlc lsp --stdio` command instead of shipping or managing a separate
 * long-running language-server daemon.
 */
function testLanguageServerDeploymentDocs() {
  const readme = readText("README.md");

  assert.ok(
    readme.includes("terlc lsp --stdio"),
    "README must document the compiler-owned LSP command"
  );
  assert.ok(
    readme.includes("does not ship a separate language-server binary"),
    "README must state that the extension does not deploy a separate LSP binary"
  );
}

/**
 * Verifies runtime files are present and test files are excluded.
 *
 * @returns {void}
 *
 * @description
 * Checks the package surface selected by `package.json.files` against the
 * extension's intended release surface: manifest, README, language
 * configuration, syntax grammar, and runtime source files only.
 */
function testPackageFileSelection() {
  const fileSet = selectedPackageFileSet();
  const requiredFiles = [
    "LICENSE",
    "package.json",
    "README.md",
    "icons/terlan-extension.svg",
    "icons/png/terlan-extension-128.png",
    "icons/terlan-file.svg",
    "icons/terlan-template-html-file.svg",
    "icons/terlan-test-file.svg",
    "icons/png/terlan-file-16.png",
    "icons/png/terlan-file-24.png",
    "icons/png/terlan-file-32.png",
    "icons/png/terlan-file-64.png",
    "icons/png/terlan-file-128.png",
    "icons/terlan-file-icon-theme.json",
    "language-configuration.json",
    "syntaxes/terlan-template-html.tmLanguage.json",
    "syntaxes/terlan.tmLanguage.json",
    "src/client_config.js",
    "src/extension.js",
    "src/template_links.js"
  ];

  for (const filePath of requiredFiles) {
    assert.ok(fileSet.has(filePath), `missing package file ${filePath}`);
  }

  for (const filePath of fileSet) {
    assert.ok(!filePath.startsWith("test/"), `test file packaged: ${filePath}`);
    assert.ok(!filePath.endsWith(".vsix"), `vsix artifact packaged: ${filePath}`);
  }
}

/**
 * Verifies the packaged VS Code icon matches the shared canonical icon.
 *
 * @returns {void}
 *
 * @description
 * Allows the VS Code package to carry a local installable asset while keeping
 * the shared editor SVG as the source of truth.
 */
function testPackagedIconMatchesCanonicalIcon() {
  const packagedIcon = readText("icons/terlan-file.svg");
  const canonicalIcon = fs.readFileSync(CANONICAL_ICON, "utf8");

  assert.strictEqual(packagedIcon, canonicalIcon);
}

/**
 * Verifies the packaged extension icon matches the shared no-fold mark.
 *
 * @returns {void}
 *
 * @description
 * Keeps the extension/package icon distinct from folded file icons so the
 * marketplace/plugin surface uses a centered Terlan mark rather than a file
 * document metaphor.
 */
function testPackagedExtensionIconMatchesCanonicalIcon() {
  const packagedIcon = readText("icons/terlan-extension.svg");
  const canonicalIcon = fs.readFileSync(CANONICAL_EXTENSION_ICON, "utf8");

  assert.strictEqual(packagedIcon, canonicalIcon);
}

/**
 * Verifies packaged VS Code PNG icons match the shared canonical variants.
 *
 * @returns {void}
 *
 * @description
 * Keeps generated raster icon variants synchronized across editor packages and
 * catches accidental local edits before publishing.
 */
function testPackagedPngIconsMatchCanonicalIcons() {
  for (const size of PNG_ICON_SIZES) {
    const packagePath = `icons/png/terlan-file-${size}.png`;
    const canonicalPath = path.join(CANONICAL_PNG_ICON_ROOT, `terlan-file-${size}.png`);

    assert.ok(fs.existsSync(canonicalPath), `missing canonical PNG icon ${size}`);
    assert.ok(readBinary(packagePath).equals(fs.readFileSync(canonicalPath)), `PNG icon ${size} drifted`);
  }
}

testPackageScripts();
testPackageMetadata();
testLanguageContributionsDeclareTerlanFileIcon();
testLanguageServerDeploymentDocs();
testPackageFileSelection();
testPackagedIconMatchesCanonicalIcon();
testPackagedExtensionIconMatchesCanonicalIcon();
testPackagedPngIconsMatchCanonicalIcons();

console.log("terlan vscode package smoke tests passed");
