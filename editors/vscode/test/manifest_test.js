"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EXTENSION_ROOT = path.resolve(__dirname, "..");
const CANONICAL_ICON = path.resolve(EXTENSION_ROOT, "..", "shared", "icons", "terlan-file.svg");

/**
 * Reads and parses a JSON file from the extension package.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {*} Parsed JSON value.
 *
 * @description
 * Converts checked-in JSON files into ordinary JavaScript values so manifest
 * contract tests can validate extension metadata without VS Code tooling.
 */
function readJson(relativePath) {
  const filePath = path.join(EXTENSION_ROOT, relativePath);
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

/**
 * Resolves an extension-relative path and asserts it exists.
 *
 * @param {string} relativePath Path relative to `editors/vscode`.
 * @returns {string} Absolute path for the checked file.
 *
 * @description
 * Normalizes VS Code manifest paths before checking the local filesystem so
 * missing language configuration, grammar, or entrypoint files fail in CI.
 */
function assertPackagePathExists(relativePath) {
  const filePath = path.join(EXTENSION_ROOT, relativePath);
  assert.ok(fs.existsSync(filePath), `expected ${relativePath} to exist`);
  return filePath;
}

/**
 * Returns contributed language identifiers from the extension manifest.
 *
 * @param {*} manifest Parsed `package.json` manifest.
 * @returns {string[]} Language ids declared under `contributes.languages`.
 *
 * @description
 * Extracts the language ids that must also be represented by activation events
 * and grammar contributions.
 */
function languageIds(manifest) {
  return manifest.contributes.languages.map((language) => language.id);
}

/**
 * Returns grammar language identifiers from the extension manifest.
 *
 * @param {*} manifest Parsed `package.json` manifest.
 * @returns {string[]} Language ids declared under `contributes.grammars`.
 *
 * @description
 * Extracts grammar coverage so the test can detect language associations that
 * would activate without syntax highlighting.
 */
function grammarLanguageIds(manifest) {
  return manifest.contributes.grammars.map((grammar) => grammar.language);
}

/**
 * Verifies the extension entrypoint and runtime dependency declaration.
 *
 * @returns {void}
 *
 * @description
 * Ensures VS Code can load the declared extension entrypoint and that the LSP
 * client dependency needed by that entrypoint remains declared.
 */
function testEntrypointAndDependencies() {
  const manifest = readJson("package.json");
  assertPackagePathExists(manifest.main);
  assert.strictEqual(
    manifest.dependencies["vscode-languageclient"],
    "^9.0.1"
  );
}

/**
 * Verifies the extension exposes the Terlan run command.
 *
 * @returns {void}
 *
 * @description
 * Checks command, activation, configuration, and editor menu contributions so
 * the visible run button stays connected to `terlc run`.
 */
function testRunCommandContribution() {
  const manifest = readJson("package.json");
  const commands = manifest.contributes.commands || [];
  const runCommand = commands.find((command) => command.command === "terlan.runMain");
  const runTestFileCommand = commands.find(
    (command) => command.command === "terlan.runTestFile"
  );
  const runTestAtCursorCommand = commands.find(
    (command) => command.command === "terlan.runTestAtCursor"
  );
  const editorTitleMenus = manifest.contributes.menus["editor/title"] || [];

  assert.ok(runCommand, "missing Terlan run command");
  assert.ok(runTestFileCommand, "missing Terlan test-file command");
  assert.ok(runTestAtCursorCommand, "missing Terlan test-at-cursor command");
  assert.strictEqual(runCommand.title, "Run Terlan Main");
  assert.strictEqual(runTestFileCommand.title, "Run Terlan Test File");
  assert.strictEqual(runTestAtCursorCommand.title, "Run Terlan Test at Cursor");
  assert.ok(
    manifest.activationEvents.includes("onStartupFinished"),
    "missing startup activation for CodeLens provider"
  );
  assert.ok(
    manifest.activationEvents.includes("onCommand:terlan.runMain"),
    "missing run command activation"
  );
  assert.ok(
    manifest.activationEvents.includes("onCommand:terlan.runTestFile"),
    "missing test-file command activation"
  );
  assert.ok(
    manifest.activationEvents.includes("onCommand:terlan.runTestAtCursor"),
    "missing test-at-cursor command activation"
  );
  assert.strictEqual(
    manifest.contributes.configuration.properties["terlan.run.command"].default,
    "terlc"
  );
  assert.ok(
    editorTitleMenus.some((menu) => menu.command === "terlan.runMain"),
    "missing editor title run button"
  );
  assert.ok(
    editorTitleMenus.some((menu) => menu.command === "terlan.runTestFile"),
    "missing editor title test-file button"
  );
  assert.ok(
    editorTitleMenus.some((menu) => menu.command === "terlan.runTestAtCursor"),
    "missing editor title test-at-cursor button"
  );
}

/**
 * Verifies every language has activation and language configuration.
 *
 * @returns {void}
 *
 * @description
 * Checks that each contributed Terlan language id wakes the extension and has a
 * local `language-configuration.json` file for editor behavior.
 */
function testLanguageContributions() {
  const manifest = readJson("package.json");
  const activationEvents = new Set(manifest.activationEvents);

  for (const language of manifest.contributes.languages) {
    assert.ok(
      activationEvents.has(`onLanguage:${language.id}`),
      `missing activation event for ${language.id}`
    );
    assertPackagePathExists(language.configuration);
    assert.ok(
      (language.extensions || language.filenamePatterns || []).length > 0,
      `${language.id} needs extension or filename pattern`
    );
  }

  const templateHtml = manifest.contributes.languages.find(
    (language) => language.id === "terlan-template-html"
  );
  assert.ok(templateHtml, "missing Terlan HTML template language");
  assert.ok(
    templateHtml.aliases.includes("HTML"),
    "Terlan HTML templates should be discoverable as HTML"
  );
}

/**
 * Verifies grammar contributions cover every language and point at valid JSON.
 *
 * @returns {void}
 *
 * @description
 * Ensures syntax highlighting remains attached to all Terlan source/template
 * language ids while the checked-in TextMate bridge is still in use. Multiple
 * VS Code grammar contributions may intentionally reuse the same physical
 * TextMate grammar until generated per-target grammars exist.
 */
function testGrammarContributions() {
  const manifest = readJson("package.json");
  const grammarIds = new Set(grammarLanguageIds(manifest));

  for (const languageId of languageIds(manifest)) {
    assert.ok(grammarIds.has(languageId), `missing grammar for ${languageId}`);
  }

  for (const grammar of manifest.contributes.grammars) {
    const grammarPath = assertPackagePathExists(grammar.path);
    const grammarJson = JSON.parse(fs.readFileSync(grammarPath, "utf8"));
    assert.strictEqual(typeof grammarJson.scopeName, "string");
    assert.strictEqual(typeof grammar.scopeName, "string");
  }

  const htmlGrammar = manifest.contributes.grammars.find(
    (grammar) => grammar.language === "terlan-template-html"
  );
  assert.ok(htmlGrammar, "missing Terlan HTML template grammar");
  assert.strictEqual(
    htmlGrammar.path,
    "./syntaxes/terlan-template-html.tmLanguage.json"
  );
  assert.deepStrictEqual(htmlGrammar.embeddedLanguages, {
    "text.html.basic": "html",
    "source.terlan.embedded": "terlan"
  });
}

/**
 * Verifies the checked-in TextMate grammar covers 0.0.5 template declarations.
 *
 * @returns {void}
 *
 * @description
 * Reads the conservative TextMate bridge and checks that the template
 * declaration keyword remains highlighted while Tree-sitter-backed highlighting
 * is still pending.
 */
function testTextMateTemplateKeywordCoverage() {
  const grammar = readJson("syntaxes/terlan.tmLanguage.json");
  const keywordPattern = grammar.repository.keywords.patterns.find(
    (pattern) => pattern.name === "keyword.declaration.terlan"
  );

  assert.ok(keywordPattern, "missing declaration keyword pattern");
  assert.ok(
    keywordPattern.match.includes("template"),
    "TextMate grammar should highlight template declarations"
  );
}

/**
 * Verifies the VS Code file icon theme covers Terlan suffixes.
 *
 * @returns {void}
 *
 * @description
 * Ensures VS Code exposes a Terlan file icon theme and maps every source,
 * interface, and template suffix to the canonical Terlan file icon definition.
 */
function testIconThemeContributions() {
  const manifest = readJson("package.json");
  const iconThemes = manifest.contributes.iconThemes || [];
  const terlanIconTheme = iconThemes.find(
    (theme) => theme.id === "terlan-file-icons"
  );

  assert.ok(terlanIconTheme, "missing Terlan file icon theme");
  const themePath = assertPackagePathExists(terlanIconTheme.path);
  const theme = JSON.parse(fs.readFileSync(themePath, "utf8"));
  const suffixes = [
    "terl",
    "terli",
    "terl.html",
    "terl.md",
    "terl.json",
    "terl.toml",
    "terl.yaml",
    "terl.yml",
    "terl.txt",
  ];

  assert.strictEqual(
    theme.iconDefinitions._terlan_file.iconPath,
    "./terlan-file.svg"
  );
  assert.strictEqual(
    theme.iconDefinitions._terlan_test_file.iconPath,
    "./terlan-test-file.svg"
  );
  assert.strictEqual(
    theme.iconDefinitions._terlan_template_html_file.iconPath,
    "./terlan-template-html-file.svg"
  );
  assert.strictEqual(theme.languageIds["terlan-test"], "_terlan_test_file");
  assert.strictEqual(
    theme.languageIds["terlan-template-html"],
    "_terlan_template_html_file"
  );
  assertPackagePathExists("icons/terlan-file.svg");
  assertPackagePathExists("icons/terlan-template-html-file.svg");
  assertPackagePathExists("icons/terlan-test-file.svg");
  assert.ok(fs.existsSync(CANONICAL_ICON), "missing canonical shared icon");

  for (const suffix of suffixes) {
    const expectedIcon = suffix === "terl.html"
      ? "_terlan_template_html_file"
      : "_terlan_file";
    assert.strictEqual(theme.fileExtensions[suffix], expectedIcon, `missing icon mapping for ${suffix}`);
  }
}

testEntrypointAndDependencies();
testRunCommandContribution();
testLanguageContributions();
testGrammarContributions();
testTextMateTemplateKeywordCoverage();
testIconThemeContributions();

console.log("terlan vscode manifest tests passed");
