"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const EXTENSION_ROOT = path.resolve(__dirname, "..");
const TREE_SITTER_ROOT = path.resolve(EXTENSION_ROOT, "..", "..", "tree-sitter-terlan");

/**
 * Reads and parses the VS Code TextMate grammar.
 *
 * @returns {*} Parsed TextMate grammar JSON.
 *
 * @description
 * Loads the temporary TextMate bridge so tests can compare it with the
 * Tree-sitter highlight contract until generated highlighting replaces it.
 */
function readTextMateGrammar() {
  return JSON.parse(
    fs.readFileSync(
      path.join(EXTENSION_ROOT, "syntaxes", "terlan.tmLanguage.json"),
      "utf8"
    )
  );
}

/**
 * Reads the Terlan HTML template TextMate grammar.
 *
 * @returns {*} Parsed TextMate grammar JSON.
 *
 * @description
 * Loads the HTML-backed template grammar so tests can ensure `.terl.html`
 * files keep normal HTML highlighting plus Terlan expression islands.
 */
function readTemplateHtmlGrammar() {
  return JSON.parse(
    fs.readFileSync(
      path.join(EXTENSION_ROOT, "syntaxes", "terlan-template-html.tmLanguage.json"),
      "utf8"
    )
  );
}

/**
 * Reads the Tree-sitter highlight query file.
 *
 * @returns {string} Highlight query source.
 *
 * @description
 * Loads the checked-in Tree-sitter query that acts as the editor highlighting
 * contract across VS Code and future Tree-sitter-capable editor hosts.
 */
function readTreeSitterHighlights() {
  return fs.readFileSync(
    path.join(TREE_SITTER_ROOT, "queries", "highlights.scm"),
    "utf8"
  );
}

/**
 * Extracts literal keywords from one Tree-sitter query capture group.
 *
 * @param {string} query Tree-sitter highlight query source.
 * @param {string} capture Capture name such as `@keyword.control`.
 * @returns {string[]} Literal words listed immediately before the capture.
 *
 * @description
 * Parses only the simple bracketed keyword groups used by the Terlan highlight
 * query. This keeps the bridge test explicit without implementing a general
 * Tree-sitter query parser.
 */
function keywordsForCapture(query, capture) {
  const escapedCapture = capture.replace(".", "\\.");
  const pattern = new RegExp(`\\[\\s*((?:"[^"]+"\\s*)+)\\]\\s*${escapedCapture}(?![.A-Za-z0-9_-])`);
  const match = query.match(pattern);
  assert.ok(match, `missing Tree-sitter capture group ${capture}`);

  return Array.from(match[1].matchAll(/"([^"]+)"/g)).map((keyword) => keyword[1]);
}

/**
 * Returns the TextMate keyword pattern with the requested scope name.
 *
 * @param {*} grammar Parsed TextMate grammar JSON.
 * @param {string} scope TextMate scope name.
 * @returns {*} TextMate pattern object.
 *
 * @description
 * Finds one keyword pattern from the grammar repository and fails with a stable
 * assertion if the temporary bridge stops exposing the expected scope.
 */
function keywordPattern(grammar, scope) {
  const pattern = grammar.repository.keywords.patterns.find(
    (candidate) => candidate.name === scope
  );
  assert.ok(pattern, `missing TextMate keyword scope ${scope}`);
  return pattern;
}

/**
 * Verifies TextMate keyword coverage follows Tree-sitter keywords.
 *
 * @returns {void}
 *
 * @description
 * Compares declaration/control keyword literals from Tree-sitter highlight
 * queries with the temporary TextMate bridge so VS Code does not lag behind the
 * canonical editor grammar while Tree-sitter-backed highlighting is pending.
 */
function testKeywordBridgeCoverage() {
  const grammar = readTextMateGrammar();
  const query = readTreeSitterHighlights();
  const controlPattern = keywordPattern(grammar, "keyword.control.terlan");
  const declarationPattern = keywordPattern(grammar, "keyword.declaration.terlan");

  for (const keyword of keywordsForCapture(query, "@keyword.control")) {
    assert.ok(
      controlPattern.match.includes(keyword),
      `TextMate control keywords missing ${keyword}`
    );
  }

  for (const keyword of keywordsForCapture(query, "@keyword")) {
    assert.ok(
      declarationPattern.match.includes(keyword),
      `TextMate declaration keywords missing ${keyword}`
    );
  }
}

/**
 * Verifies TextMate interpolation coverage follows Tree-sitter interpolation.
 *
 * @returns {void}
 *
 * @description
 * Locks `${...}` expression-island highlighting in the temporary TextMate
 * bridge against the Tree-sitter query contract used by multi-editor support.
 */
function testInterpolationBridgeCoverage() {
  const grammar = readTextMateGrammar();
  const query = readTreeSitterHighlights();
  const interpolation = grammar.repository.interpolation.patterns[0];

  assert.ok(query.includes("interpolation"), "Tree-sitter query must cover interpolation");
  assert.strictEqual(interpolation.begin, "\\$\\{");
  assert.strictEqual(interpolation.end, "\\}");
  assert.deepStrictEqual(interpolation.patterns, [{ include: "$self" }]);
}

/**
 * Verifies `.terl.html` highlighting embeds HTML and Terlan expressions.
 *
 * @returns {void}
 *
 * @description
 * Locks the VS Code grammar choice that treats template HTML as HTML first,
 * while still highlighting `${...}` as embedded Terlan source.
 */
function testTemplateHtmlGrammarEmbedsHtmlAndTerlan() {
  const grammar = readTemplateHtmlGrammar();
  const includes = grammar.patterns.map((pattern) => pattern.include);
  const interpolation = grammar.repository["terlan-interpolation"].patterns[0];

  assert.strictEqual(grammar.scopeName, "text.html.terlan");
  assert.ok(includes.includes("text.html.basic"));
  assert.strictEqual(interpolation.begin, "\\$\\{");
  assert.strictEqual(interpolation.contentName, "source.terlan.embedded");
  assert.deepStrictEqual(interpolation.patterns, [{ include: "source.terlan" }]);
}

testKeywordBridgeCoverage();
testInterpolationBridgeCoverage();
testTemplateHtmlGrammarEmbedsHtmlAndTerlan();

console.log("terlan vscode TextMate bridge tests passed");
