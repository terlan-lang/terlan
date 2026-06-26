"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EXTENSION_ROOT = path.resolve(__dirname, "..");
const DEFAULT_DRY_RUN_PATH = "/tmp/terlan-vscode-pack.json";

/**
 * Reads and parses the npm dry-run archive manifest.
 *
 * @param {string[]} argv Process arguments passed to this script.
 * @returns {*} Parsed npm dry-run JSON payload.
 *
 * @description
 * Converts `npm pack --dry-run --json` output into data that can be checked
 * deterministically without publishing or writing a `.vsix` artifact.
 */
function readDryRunPayload(argv) {
  const payloadPath = argv[2] || DEFAULT_DRY_RUN_PATH;
  return JSON.parse(fs.readFileSync(payloadPath, "utf8"));
}

/**
 * Returns the first archive object from the npm dry-run payload.
 *
 * @param {*} payload Parsed `npm pack --dry-run --json` payload.
 * @returns {*} Archive metadata emitted by npm.
 *
 * @description
 * Validates the expected npm output shape before later checks inspect package
 * file paths. This keeps packaging failures readable when npm changes shape or
 * the dry-run command fails before producing archive metadata.
 */
function archiveFromPayload(payload) {
  assert.ok(Array.isArray(payload), "npm dry-run payload must be an array");
  assert.strictEqual(payload.length, 1, "expected one dry-run archive");

  const archive = payload[0];
  assert.ok(archive && typeof archive === "object", "archive must be an object");
  assert.ok(Array.isArray(archive.files), "archive files must be an array");
  return archive;
}

/**
 * Builds a set of packaged paths from npm dry-run archive metadata.
 *
 * @param {*} archive Archive metadata emitted by npm.
 * @returns {Set<string>} Package-relative file paths selected by npm.
 *
 * @description
 * Normalizes npm file entries to forward-slash paths so package surface checks
 * behave the same on Unix, macOS, and Windows hosts.
 */
function packagedPathSet(archive) {
  return new Set(
    archive.files.map((entry) => {
      assert.ok(entry && typeof entry.path === "string", "archive file path missing");
      return entry.path.split(path.sep).join("/");
    })
  );
}

/**
 * Verifies required runtime files are present in the dry-run archive.
 *
 * @param {Set<string>} fileSet Package-relative paths selected by npm.
 * @returns {void}
 *
 * @description
 * Locks the release-facing VS Code package surface against the actual npm
 * archive model so missing runtime assets are caught before a release.
 */
function assertRequiredRuntimeFiles(fileSet) {
  const requiredFiles = [
    "package.json",
    "README.md",
    "icons/terlan-file.svg",
    "icons/png/terlan-file-16.png",
    "icons/png/terlan-file-24.png",
    "icons/png/terlan-file-32.png",
    "icons/png/terlan-file-64.png",
    "icons/png/terlan-file-128.png",
    "icons/terlan-file-icon-theme.json",
    "language-configuration.json",
    "syntaxes/terlan.tmLanguage.json",
    "src/client_config.js",
    "src/extension.js"
  ];

  for (const filePath of requiredFiles) {
    assert.ok(fileSet.has(filePath), `archive missing ${filePath}`);
  }
}

/**
 * Verifies development-only files are absent from the dry-run archive.
 *
 * @param {Set<string>} fileSet Package-relative paths selected by npm.
 * @returns {void}
 *
 * @description
 * Keeps local tests, generated archives, and scratch files out of the VS Code
 * extension package while allowing the source tree to keep focused smoke tests.
 */
function assertNoDevelopmentFiles(fileSet) {
  for (const filePath of fileSet) {
    assert.ok(!filePath.startsWith("test/"), `archive includes test file ${filePath}`);
    assert.ok(!filePath.endsWith(".vsix"), `archive includes VSIX artifact ${filePath}`);
    assert.ok(!filePath.endsWith(".tgz"), `archive includes npm package artifact ${filePath}`);
  }
}

/**
 * Verifies the archive metadata matches the extension package identity.
 *
 * @param {*} archive Archive metadata emitted by npm.
 * @returns {void}
 *
 * @description
 * Reads checked-in package metadata and compares it with npm's dry-run archive
 * identity so package publication cannot silently drift from the manifest.
 */
function assertArchiveIdentity(archive) {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(EXTENSION_ROOT, "package.json"), "utf8")
  );

  assert.strictEqual(archive.name, manifest.name);
  assert.strictEqual(archive.version, manifest.version);
}

const archive = archiveFromPayload(readDryRunPayload(process.argv));
const fileSet = packagedPathSet(archive);

assertArchiveIdentity(archive);
assertRequiredRuntimeFiles(fileSet);
assertNoDevelopmentFiles(fileSet);

console.log("terlan vscode npm dry-run archive tests passed");
