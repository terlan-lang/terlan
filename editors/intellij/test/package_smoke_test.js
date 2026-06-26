"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const EDITORS_ROOT = path.resolve(PACKAGE_ROOT, "..");
const CANONICAL_ICON = path.join(EDITORS_ROOT, "shared", "icons", "terlan-file.svg");
const CANONICAL_PNG_ICON_ROOT = path.join(EDITORS_ROOT, "shared", "icons", "png");
const PNG_ICON_SIZES = [16, 24, 32, 64, 128];

/**
 * Reads one IntelliJ package text file.
 *
 * @param {string} relativePath Path relative to `editors/intellij`.
 * @returns {string} UTF-8 file contents.
 *
 * @description
 * Loads package files for static contract validation without invoking Gradle or
 * a JetBrains IDE runtime.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(PACKAGE_ROOT, relativePath), "utf8");
}

/**
 * Reads one IntelliJ package binary file.
 *
 * @param {string} relativePath Path relative to `editors/intellij`.
 * @returns {Buffer} Raw file bytes.
 *
 * @description
 * Loads packaged binary assets for byte-for-byte comparison with shared editor
 * source assets.
 */
function readBinary(relativePath) {
  return fs.readFileSync(path.join(PACKAGE_ROOT, relativePath));
}

/**
 * Verifies the expected IntelliJ scaffold files exist.
 *
 * @returns {void}
 *
 * @description
 * Locks the minimal package layout documented in the 0.0.5 roadmap.
 */
function testExpectedFilesExist() {
  const files = [
    "README.md",
    "build.gradle.kts",
    "settings.gradle.kts",
    "src/main/resources/META-INF/plugin.xml",
    "src/main/resources/icons/terlan-file.svg",
    "src/main/resources/icons/png/terlan-file-16.png",
    "src/main/resources/icons/png/terlan-file-24.png",
    "src/main/resources/icons/png/terlan-file-32.png",
    "src/main/resources/icons/png/terlan-file-64.png",
    "src/main/resources/icons/png/terlan-file-128.png",
    "src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt",
    "src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt",
  ];

  for (const file of files) {
    assert.ok(fs.existsSync(path.join(PACKAGE_ROOT, file)), `missing ${file}`);
  }
}

/**
 * Verifies IntelliJ metadata starts the compiler-owned LSP command.
 *
 * @returns {void}
 *
 * @description
 * Ensures the plugin contract remains `terlc lsp --stdio` and does not drift
 * toward editor-specific compiler or daemon commands.
 */
function testLanguageServerCommand() {
  const descriptor = readText("src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt");
  const plugin = readText("src/main/resources/META-INF/plugin.xml");

  assert.ok(
    descriptor.includes('listOf("terlc", "lsp", "--stdio")'),
    "IntelliJ descriptor must start terlc lsp --stdio"
  );
  assert.ok(plugin.includes("terlc lsp --stdio"), "plugin docs must name terlc lsp --stdio");
  assert.ok(!descriptor.includes("terlan-lsp"), "descriptor must not prefer terlan-lsp");
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
  const descriptor = readText("src/main/kotlin/org/terlan/intellij/TerlanLspServerDescriptor.kt");

  assert.ok(descriptor.includes('"terlan.toml"'), "missing terlan.toml root marker");
  assert.ok(descriptor.includes('".git"'), "missing .git root marker");
}

/**
 * Verifies Terlan suffixes are registered.
 *
 * @returns {void}
 *
 * @description
 * Checks source, interface, and template suffixes expected by the editor
 * roadmap are present in both declarative metadata and Kotlin contract data.
 */
function testFiletypeSuffixes() {
  const fileTypes = readText("src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt");
  const plugin = readText("src/main/resources/META-INF/plugin.xml");
  const suffixes = [
    ".terl",
    ".terli",
    ".terl.html",
    ".terl.md",
    ".terl.json",
    ".terl.toml",
    ".terl.yaml",
    ".terl.yml",
    ".terl.txt",
  ];

  for (const suffix of suffixes) {
    assert.ok(fileTypes.includes(`"${suffix}"`), `missing Kotlin suffix ${suffix}`);
    assert.ok(plugin.includes(suffix.slice(1)), `missing plugin suffix ${suffix}`);
  }
}

/**
 * Verifies IntelliJ metadata points at the canonical editor icon.
 *
 * @returns {void}
 *
 * @description
 * Keeps file identity shared across editor packages instead of creating a
 * JetBrains-specific icon source.
 */
function testCanonicalIconMetadata() {
  const fileTypes = readText("src/main/kotlin/org/terlan/intellij/TerlanFileTypes.kt");
  const plugin = readText("src/main/resources/META-INF/plugin.xml");
  const packagedIcon = readText("src/main/resources/icons/terlan-file.svg");
  const canonicalIcon = fs.readFileSync(CANONICAL_ICON, "utf8");

  assert.ok(fs.existsSync(CANONICAL_ICON), "missing canonical shared icon");
  assert.ok(fileTypes.includes("/icons/terlan-file.svg"), "Kotlin icon path must use packaged icon");
  assert.ok(plugin.includes("/icons/terlan-file.svg"), "plugin icon path must use packaged icon");
  assert.strictEqual(packagedIcon, canonicalIcon);
}

/**
 * Verifies packaged IntelliJ PNG icons match the shared canonical variants.
 *
 * @returns {void}
 *
 * @description
 * Keeps JetBrains raster icon assets synchronized with the shared editor icon
 * source used by the rest of the editor packages.
 */
function testPackagedPngIconsMatchCanonicalIcons() {
  for (const size of PNG_ICON_SIZES) {
    const packagePath = `src/main/resources/icons/png/terlan-file-${size}.png`;
    const canonicalPath = path.join(CANONICAL_PNG_ICON_ROOT, `terlan-file-${size}.png`);

    assert.ok(fs.existsSync(canonicalPath), `missing canonical PNG icon ${size}`);
    assert.ok(readBinary(packagePath).equals(fs.readFileSync(canonicalPath)), `PNG icon ${size} drifted`);
  }
}

/**
 * Verifies generated IntelliJ artifacts are absent.
 *
 * @returns {void}
 *
 * @description
 * Prevents local Gradle output or plugin archives from entering the checked-in
 * editor package surface.
 */
function testNoGeneratedArtifacts() {
  const forbidden = ["build", "out", "terlan-intellij.zip"];

  for (const entry of forbidden) {
    assert.ok(!fs.existsSync(path.join(PACKAGE_ROOT, entry)), `generated artifact committed: ${entry}`);
  }
}

testExpectedFilesExist();
testLanguageServerCommand();
testRootMarkers();
testFiletypeSuffixes();
testCanonicalIconMetadata();
testPackagedPngIconsMatchCanonicalIcons();
testNoGeneratedArtifacts();

console.log("terlan intellij package smoke tests passed");
