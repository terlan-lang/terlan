"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const WORKSPACE_ROOT = path.resolve(PACKAGE_ROOT, "..");

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

function testSharedInterpolationFixtureCoverage() {
  const fixtureTable = fs.readFileSync(
    path.join(WORKSPACE_ROOT, "tests/template/INTERPOLATION_TOOLING_FIXTURES.tsv"),
    "utf8"
  );
  const corpus = readText("test/corpus/basic.txt");
  for (const line of fixtureTable.trim().split("\n")) {
    const [name] = line.split("\t");
    assert.ok(
      corpus.includes(`[fixture:${name}]`),
      `Tree-sitter corpus missing shared interpolation fixture ${name}`
    );
  }
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
    "@operator",
    "@type",
    "@type.builtin",
    "@variable",
    "@variable.parameter",
    "@property",
    "@number",
    "@string",
    "@string.special",
    "@comment",
    "@embedded"
  ];
  const requiredNodes = [
    "annotation",
    "module_declaration",
    "import_declaration",
    "shape_declaration",
    "shape_guard_expression",
    "function_declaration",
    "function_signature",
    "template_declaration",
    "template_parameter",
    "call_expression",
    "type_identifier",
    "atom_type",
    "field_declaration",
    "field_identifier",
    "private_field_selector",
    "private_field_identifier",
    "parameter",
    "receiver",
    "line_comment",
    "block_comment",
    "interpolation",
    "interpolation_start",
    "template_interpolation_start",
    "interpolation_end",
    "template_text_interpolation",
    "template_attribute_interpolation",
    "string_pattern"
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
  const binaryExpression = grammar.match(/\n\s{4}binary_expression:[\s\S]*?\n\s{4}raw_macro_expression:/);

  assert.ok(grammar.includes('"|>"'), "grammar should recognize canonical |>");
  assert.ok(grammar.includes("list_comprehension_body"), "grammar should parse list comprehensions");
  assert.ok(grammar.includes('"|"'), "grammar should recognize the list-comprehension separator");
  assert.ok(binaryExpression, "grammar should expose binary expression production");
  assert.ok(
    !binaryExpression[0].includes('"|"'),
    "grammar must not introduce single-bar expression pipes"
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

/**
 * Verifies every long-tail string-capture context has a Tree-sitter anchor.
 *
 * @returns {void}
 */
function testStringPatternLongTailCorpusCoverage() {
  const corpus = readText("test/corpus/basic.txt");
  const requiredSnippets = [
    "string capture patterns",
    '"users/${id: Int}/${name}.json" where id > 0',
    'let "users/${id: Int}/${name}.json" = path',
    'pub named("users/${id: Int}/${name}.json")',
    "shape synonym declaration",
    'shape UserAsset(id, file) = "users/${id: Int}/assets/${file}".',
    "long-tail string capture contexts",
    'let render = ("docs/${section}/${slug}.html")',
    '<p>${case route {',
    "(lambda_expression",
    "(template_text_interpolation",
    "(string_pattern)"
  ];

  for (const snippet of requiredSnippets) {
    assert.ok(corpus.includes(snippet), `missing long-tail corpus snippet ${snippet}`);
  }
}

/**
 * Verifies shape declarations and their ordinary pattern uses stay aligned
 * with the compiler grammar.
 *
 * @returns {void}
 */
function testShapeSynonymCorpusCoverage() {
  const grammar = readText("grammar.js");
  const corpus = readText("test/corpus/shape_synonyms.txt");
  const shapeGuard = grammar.match(
    /\n\s{4}shape_guard_clause:[\s\S]*?\n\s{4}shape_guard_expression:/
  );
  const requiredSnippets = [
    "shape synonym declarations and ordinary pattern uses",
    "pub shape OkResponse(body)",
    'shape UserAsset(id) = "users/${id: Int}.json".',
    "pub status(OkResponse(body): Dynamic): Int -> body.",
    "UserAsset(id) -> id;",
    "(shape_declaration",
    "(constructor_pattern",
    "(case_expression"
  ];

  assert.ok(shapeGuard, "grammar should expose the shape guard production");
  assert.ok(shapeGuard[0].includes('seq("where"'), "shape guards must use canonical where");
  assert.ok(!shapeGuard[0].includes('"when"'), "shape guards must reject legacy when");
  for (const snippet of requiredSnippets) {
    assert.ok(corpus.includes(snippet), `missing shape corpus snippet ${snippet}`);
  }
}

/**
 * Verifies binary constructors and every shared pattern position are owned by
 * the editor grammar and highlight query.
 *
 * @returns {void}
 */
function testBinaryLayoutToolingCoverage() {
  const grammar = readText("grammar.js");
  const query = readText("queries/highlights.scm");
  const corpus = readText("test/corpus/binary_layouts.txt");
  const requiredGrammarNodes = [
    "binary_layout_expression",
    "binary_layout_pattern",
    "binary_layout_descriptor",
    "binary_layout_endian"
  ];
  const requiredCorpusSnippets = [
    "binary layout constructors and patterns",
    "Binary[big] { port: UInt[16], scalar: Utf8, body: Rest }",
    "Binary[little] { port: UInt[16], delta: IntBits[8] }",
    "pub decode_head(Binary[big]",
    "let decode = ((Binary[big]",
    "let Binary[big] { prefix: Bytes[2], flags: Bits[3], body: Rest }"
  ];

  for (const node of requiredGrammarNodes) {
    assert.ok(grammar.includes(node), `missing binary grammar node ${node}`);
    assert.ok(query.includes(node), `missing binary highlight node ${node}`);
  }
  for (const snippet of requiredCorpusSnippets) {
    assert.ok(corpus.includes(snippet), `missing binary corpus snippet ${snippet}`);
  }
  assert.ok(query.includes('@constant.builtin'), "binary endian should be highlighted as a constant");
}

testTreeSitterScripts();
testTreeSitterFileTypes();
testPackageFileSelection();
testHighlightQueryCoverage();
testInjectionQueryCoverage();
testSharedInterpolationFixtureCoverage();
testCanonicalPipeOperatorSpelling();
testCorpusCoverage();
testStringPatternLongTailCorpusCoverage();
testShapeSynonymCorpusCoverage();
testBinaryLayoutToolingCoverage();

console.log("terlan tree-sitter package smoke tests passed");
