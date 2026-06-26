"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const DEFAULT_DRY_RUN_PATH = "/tmp/terlan-tree-sitter-pack.json";

/**
 * Reads and parses npm dry-run archive metadata.
 *
 * @param {string[]} argv Process arguments passed to this script.
 * @returns {*} Parsed `npm pack --dry-run --json` payload.
 *
 * @description
 * Loads the package archive preview produced by npm so release checks validate
 * the actual publishable file list rather than only the hand-modeled manifest.
 */
function readDryRunPayload(argv) {
  const payloadPath = argv[2] || DEFAULT_DRY_RUN_PATH;
  return JSON.parse(fs.readFileSync(payloadPath, "utf8"));
}

/**
 * Returns the single archive entry from npm dry-run output.
 *
 * @param {*} payload Parsed npm dry-run JSON payload.
 * @returns {*} Archive metadata emitted by npm.
 *
 * @description
 * Validates npm's output shape before file-level checks run so failures point
 * to the package command rather than producing obscure path assertions.
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
 * Builds a normalized set of packaged paths.
 *
 * @param {*} archive Archive metadata emitted by npm.
 * @returns {Set<string>} Package-relative file paths selected by npm.
 *
 * @description
 * Converts npm file entries into forward-slash paths so checks are stable
 * across local development platforms and CI runners.
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
 * Verifies required grammar package files are present.
 *
 * @param {Set<string>} fileSet Package-relative paths selected by npm.
 * @returns {void}
 *
 * @description
 * Locks the Tree-sitter package surface needed by editor hosts: package
 * metadata, grammar source, highlight and injection queries, README, and corpus
 * fixtures.
 */
function assertRequiredRuntimeFiles(fileSet) {
  const requiredFiles = [
    "package.json",
    "README.md",
    "grammar.js",
    "queries/highlights.scm",
    "queries/injections.scm",
    "test/corpus/basic.txt"
  ];

  for (const filePath of requiredFiles) {
    assert.ok(fileSet.has(filePath), `archive missing ${filePath}`);
  }
}

/**
 * Verifies generated and smoke-test files are excluded.
 *
 * @param {Set<string>} fileSet Package-relative paths selected by npm.
 * @returns {void}
 *
 * @description
 * Keeps dependency-free test scripts, generated parser outputs, bindings, and
 * package archives out of the Tree-sitter npm package until those files become
 * intentional release artifacts.
 */
function assertNoDevelopmentFiles(fileSet) {
  for (const filePath of fileSet) {
    assert.ok(
      !filePath.endsWith("package_smoke_test.js"),
      `archive includes smoke test ${filePath}`
    );
    assert.ok(
      !filePath.endsWith("pack_dry_run_test.js"),
      `archive includes archive smoke test ${filePath}`
    );
    assert.ok(!filePath.startsWith("src/"), `archive includes generated parser ${filePath}`);
    assert.ok(!filePath.startsWith("bindings/"), `archive includes generated binding ${filePath}`);
    assert.ok(!filePath.endsWith(".tgz"), `archive includes npm package artifact ${filePath}`);
  }
}

/**
 * Verifies archive identity matches package metadata.
 *
 * @param {*} archive Archive metadata emitted by npm.
 * @returns {void}
 *
 * @description
 * Compares npm's archive identity with checked-in package metadata so the
 * grammar package cannot silently publish under a stale name or version.
 */
function assertArchiveIdentity(archive) {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(PACKAGE_ROOT, "package.json"), "utf8")
  );

  assert.strictEqual(archive.name, manifest.name);
  assert.strictEqual(archive.version, manifest.version);
}

const archive = archiveFromPayload(readDryRunPayload(process.argv));
const fileSet = packagedPathSet(archive);

assertArchiveIdentity(archive);
assertRequiredRuntimeFiles(fileSet);
assertNoDevelopmentFiles(fileSet);

console.log("terlan tree-sitter npm dry-run archive tests passed");
