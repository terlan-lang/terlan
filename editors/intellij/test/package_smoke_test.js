"use strict";

const assert = require("assert");
const childProcess = require("child_process");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const EDITORS_ROOT = path.resolve(PACKAGE_ROOT, "..");
const CANONICAL_ICON = path.join(EDITORS_ROOT, "shared", "icons", "terlan-file.svg");

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
 * Verifies the expected buildable IntelliJ plugin files exist.
 *
 * @returns {void}
 *
 * @description
 * Locks the runtime, build-wrapper, and validation inputs required by the
 * executable JetBrains LSP integration.
 */
function testExpectedFilesExist() {
  const files = [
    "README.md",
    "build.gradle.kts",
    "gradlew",
    "gradlew.bat",
    "gradle/wrapper/gradle-wrapper.jar",
    "gradle/wrapper/gradle-wrapper.properties",
    "gradle.properties",
    "settings.gradle.kts",
    "src/main/resources/META-INF/plugin.xml",
    "src/main/resources/icons/terlan-file.svg",
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
  assert.ok(
    descriptor.includes("LspServerSupportProvider"),
    "IntelliJ must register an executable LSP support provider"
  );
  assert.ok(
    descriptor.includes("ProjectWideLspServerDescriptor"),
    "IntelliJ must use one project-wide compiler LSP process"
  );
  assert.ok(
    descriptor.includes("ensureServerStarted"),
    "opening a Terlan file must start the compiler LSP"
  );
  assert.ok(
    plugin.includes("platform.lsp.serverSupportProvider"),
    "plugin metadata must register the LSP support provider"
  );
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
  assert.ok(descriptor.includes("rootMarkers.any"), "root markers must drive root discovery");
  assert.ok(descriptor.includes("withWorkDirectory"), "LSP process must start in the discovered root");
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
    ".terls",
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
 * Verifies generated IntelliJ artifacts are not tracked.
 *
 * @returns {void}
 *
 * @description
 * Prevents local Gradle output or plugin archives from entering the checked-in
 * editor package surface.
 */
function testNoGeneratedArtifactsAreTracked() {
  const repositoryRoot = path.resolve(PACKAGE_ROOT, "..", "..");
  const tracked = childProcess.execFileSync(
    "git",
    [
      "ls-files",
      "--",
      "editors/intellij/bin",
      "editors/intellij/build",
      "editors/intellij/out",
      "editors/intellij/*.zip",
    ],
    { cwd: repositoryRoot, encoding: "utf8" }
  ).trim();
  assert.strictEqual(tracked, "", `generated IntelliJ artifacts are tracked:\n${tracked}`);
}

/** Ensures every authored IntelliJ file is a build input or an explicit gate. */
function testFilesParticipateInBuildOrValidation() {
  function collect(relativePath = ".") {
    const generatedDirectories = new Set([
      ".gradle",
      ".intellijPlatform",
      ".kotlin",
      "bin",
      "build",
      "out",
    ]);
    const files = [];
    for (const entry of fs.readdirSync(path.join(PACKAGE_ROOT, relativePath), { withFileTypes: true })) {
      if (entry.isDirectory() && generatedDirectories.has(entry.name)) {
        continue;
      }
      const child = relativePath === "." ? entry.name : path.join(relativePath, entry.name);
      if (entry.isDirectory()) {
        files.push(...collect(child));
      } else {
        files.push(child.split(path.sep).join("/"));
      }
    }
    return files;
  }
  const explicitInputs = new Set([
    "README.md",
    "build.gradle.kts",
    "gradlew",
    "gradlew.bat",
    "gradle/wrapper/gradle-wrapper.jar",
    "gradle/wrapper/gradle-wrapper.properties",
    "gradle.properties",
    "settings.gradle.kts",
    "test/package_smoke_test.js",
  ]);
  const dormant = collect().filter((file) =>
    !explicitInputs.has(file)
      && !file.startsWith("src/main/kotlin/")
      && !file.startsWith("src/main/resources/")
  );
  assert.deepStrictEqual(dormant, [], `dormant IntelliJ package files: ${dormant.join(", ")}`);
}

testExpectedFilesExist();
testLanguageServerCommand();
testRootMarkers();
testFiletypeSuffixes();
testCanonicalIconMetadata();
testNoGeneratedArtifactsAreTracked();
testFilesParticipateInBuildOrValidation();

console.log("terlan intellij package smoke tests passed");
