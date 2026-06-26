"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");

/**
 * Reads and parses a JSON file from the Tree-sitter package.
 *
 * @param {string} relativePath Path relative to `tree-sitter-terlan`.
 * @returns {*} Parsed JSON value.
 *
 * @description
 * Converts checked-in package metadata into a JavaScript value so package
 * contract tests can validate script wiring and selected release files.
 */
function readJson(relativePath) {
  return JSON.parse(
    fs.readFileSync(path.join(PACKAGE_ROOT, relativePath), "utf8")
  );
}

/**
 * Reads one Tree-sitter package text file.
 *
 * @param {string} relativePath Path relative to `tree-sitter-terlan`.
 * @returns {string} UTF-8 file text.
 *
 * @description
 * Loads grammar/query/corpus text for dependency-free package smoke checks.
 */
function readText(relativePath) {
  return fs.readFileSync(path.join(PACKAGE_ROOT, relativePath), "utf8");
}

/**
 * Recursively collects files under a package-relative path.
 *
 * @param {string} relativePath File or directory path from `package.json`.
 * @returns {string[]} Package-relative file paths.
 *
 * @description
 * Expands package `files` entries into concrete paths so the smoke can verify
 * grammar, query, and corpus assets are selected for publication.
 */
function collectPackageFiles(relativePath) {
  const absolutePath = path.join(PACKAGE_ROOT, relativePath);
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
 * Returns the selected Tree-sitter package files.
 *
 * @returns {Set<string>} Package-relative files selected by `package.json`.
 *
 * @description
 * Applies the package `files` list and adds `package.json`, which npm includes
 * by default, to model the intended release surface without writing an archive.
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
 * Verifies Tree-sitter CLI command wiring.
 *
 * @returns {void}
 *
 * @description
 * Checks that package scripts expose generation, parser tests, and the
 * dependency-free metadata check while depending on the Tree-sitter CLI package
 * for local grammar generation.
 */
function testTreeSitterScripts() {
  const manifest = readJson("package.json");
  assert.strictEqual(manifest.scripts.generate, "tree-sitter generate --no-bindings");
  assert.strictEqual(manifest.scripts.test, "tree-sitter test");
  assert.strictEqual(manifest.scripts["check:cli"], "npm run generate && npm test");
  assert.strictEqual(
    manifest.scripts["pack:dry-run"],
    "npm pack --dry-run --json"
  );
  assert.ok(
    manifest.scripts.check.includes("test/package_smoke_test.js"),
    "check script should run package smoke"
  );
  assert.strictEqual(manifest.devDependencies["tree-sitter-cli"], "^0.22.6");
}

/**
 * Verifies Tree-sitter package file-type coverage.
 *
 * @returns {void}
 *
 * @description
 * Ensures source, interface, and template suffixes are attached to the grammar
 * package so generated editor integrations can consume one metadata source.
 */
function testTreeSitterFileTypes() {
  const manifest = readJson("package.json");
  const grammar = manifest["tree-sitter"][0];
  const fileTypes = new Set(grammar["file-types"]);
  const expected = [
    "terl",
    "terli",
    "terl.html",
    "terl.md",
    "terl.json",
    "terl.toml",
    "terl.yaml",
    "terl.yml",
    "terl.txt"
  ];

  assert.strictEqual(grammar.highlights, "queries/highlights.scm");
  assert.strictEqual(grammar.injections, "queries/injections.scm");
  for (const fileType of expected) {
    assert.ok(fileTypes.has(fileType), `missing Tree-sitter file type ${fileType}`);
  }
}

/**
 * Verifies grammar release files are selected and test helpers are excluded.
 *
 * @returns {void}
 *
 * @description
 * Ensures the package includes grammar, highlight query, and parser corpus
 * inputs while excluding dependency-free smoke tests and generated parser
 * outputs until those outputs become intentional release artifacts.
 */
function testPackageFileSelection() {
  const fileSet = selectedPackageFileSet();
  const requiredFiles = [
    "package.json",
    "README.md",
    "grammar.js",
    "queries/injections.scm",
    "queries/highlights.scm",
    "test/corpus/basic.txt"
  ];

  for (const filePath of requiredFiles) {
    assert.ok(fileSet.has(filePath), `missing package file ${filePath}`);
  }

  for (const filePath of fileSet) {
    assert.ok(
      !filePath.endsWith("package_smoke_test.js"),
      `smoke test packaged: ${filePath}`
    );
    assert.ok(!filePath.startsWith("src/"), `generated parser packaged: ${filePath}`);
    assert.ok(!filePath.startsWith("bindings/"), `generated binding packaged: ${filePath}`);
  }
}

/**
 * Verifies highlight query coverage for current editor regions.
 *
 * @returns {void}
 *
 * @description
 * Checks the query file names every capture class required by the 0.0.5
 * editor scaffold before generated Tree-sitter tests are available locally.
 */
function testHighlightQueryCoverage() {
  const query = readText("queries/highlights.scm");
  const requiredCaptures = [
    "@keyword",
    "@keyword.control",
    "@punctuation.special",
    "@attribute",
    "@namespace",
    "@function",
    "@function.call",
    "@type",
    "@type.builtin",
    "@variable",
    "@variable.parameter",
    "@property",
    "@number",
    "@string",
    "@comment",
    "@embedded"
  ];
  const requiredNodes = [
    "annotation",
    "module_declaration",
    "import_declaration",
    "function_declaration",
    "function_signature",
    "template_declaration",
    "template_parameter",
    "call_expression",
    "type_identifier",
    "atom_type",
    "field_declaration",
    "private_field_identifier",
    "parameter",
    "receiver",
    "line_comment",
    "block_comment",
    "interpolation"
  ];

  for (const capture of requiredCaptures) {
    assert.ok(query.includes(capture), `missing highlight capture ${capture}`);
  }
  for (const node of requiredNodes) {
    assert.ok(query.includes(node), `missing highlight node ${node}`);
  }
}

/**
 * Verifies interpolation expression injection query coverage.
 *
 * @returns {void}
 *
 * @description
 * Ensures mixed `.terl.*` template files can reuse Terlan highlighting inside
 * `${...}` expression islands once editor hosts consume Tree-sitter queries.
 */
function testInjectionQueryCoverage() {
  const query = readText("queries/injections.scm");

  assert.ok(query.includes("interpolation"), "injection query should target interpolation");
  assert.ok(
    query.includes("@injection.content"),
    "injection query should mark interpolation expression content"
  );
  assert.ok(
    query.includes('injection.language "terlan"'),
    "injection query should inject Terlan expression highlighting"
  );
}

/**
 * Verifies the editor grammar keeps the canonical pipe spelling.
 *
 * @returns {void}
 *
 * @description
 * Locks the 0.0.5 template-expression rule that pipes inside `${...}` are
 * ordinary Terlan `|>` expressions, not template-only `{ value | filter }`
 * filters.
 */
function testCanonicalPipeOperatorSpelling() {
  const grammar = readText("grammar.js");

  assert.ok(grammar.includes('"|>"'), "grammar should recognize canonical |>");
  assert.ok(
    !grammar.includes('"|"'),
    "grammar must not introduce single-bar template filter pipes"
  );
}

/**
 * Verifies the checked-in parser corpus covers current 0.0.5 syntax examples.
 *
 * @returns {void}
 *
 * @description
 * Locks representative source snippets and expected node names without running
 * the optional Tree-sitter CLI. The CLI corpus path remains available through
 * `npm test` when local package dependencies are installed.
 */
function testCorpusCoverage() {
  const corpus = readText("test/corpus/basic.txt");
  const requiredSnippets = [
    "pub main(): Unit ->",
    "pub struct User implements Show[User]",
    "#email: String",
    "template Page from \"../../templates/page.terl.html\"",
    "Page(title = \"Hello\").",
    "@route {",
    "Response.text(\"ok\", status = 200).",
    "pub (user: User) display_name(): String ->",
    "case user.name {",
    "user.#email.",
    "if {",
    "user.display_name()",
    "sql {",
    "${count.to_string()}",
    "(interpolation",
    "(method_call_expression"
  ];
  const requiredNodes = [
    "(module_declaration",
    "(import_declaration",
    "(trait_declaration",
    "(struct_declaration",
    "(template_declaration",
    "(function_declaration"
  ];

  for (const snippet of requiredSnippets) {
    assert.ok(corpus.includes(snippet), `missing corpus snippet ${snippet}`);
  }
  for (const node of requiredNodes) {
    assert.ok(corpus.includes(node), `missing corpus expected node ${node}`);
  }
  assert.ok(
    !corpus.includes("children: Template.Html"),
    "template corpus must not model reserved children as a normal prop"
  );
}

testTreeSitterScripts();
testTreeSitterFileTypes();
testPackageFileSelection();
testHighlightQueryCoverage();
testInjectionQueryCoverage();
testCanonicalPipeOperatorSpelling();
testCorpusCoverage();

console.log("terlan tree-sitter package smoke tests passed");
