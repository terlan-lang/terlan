"use strict";

const assert = require("assert");
const path = require("path");
const {
  findTemplateComponentTagLinks,
  parseTemplateDeclarations,
  templateTagFromPath
} = require("../src/template_links");

/**
 * Verifies template path names normalize to compiler component tags.
 *
 * @returns {void}
 *
 * @description
 * Mirrors the `terlan_html` filename-derived tag contract in the VS Code
 * helper so editor links target the same component names as the compiler.
 */
function testTemplateTagFromPath() {
  assert.strictEqual(
    templateTagFromPath("templates/page_shell.terl.html"),
    "page-shell"
  );
  assert.strictEqual(
    templateTagFromPath("templates/MainLayout.terl.html"),
    "mainlayout"
  );
  assert.strictEqual(
    templateTagFromPath("templates/welcome_content.terl.md"),
    "welcome-content"
  );
  assert.strictEqual(templateTagFromPath("templates/page.html"), undefined);
}

/**
 * Verifies Terlan template declarations are parsed with tag and location data.
 *
 * @returns {void}
 *
 * @description
 * Extracts enough declaration metadata for the extension's document-link
 * provider without requiring a VS Code host or compiler process.
 */
function testParseTemplateDeclarations() {
  const filePath = path.join("/repo", "src", "app", "Site.terl");
  const declarations = parseTemplateDeclarations(
    "module app.Site.\n\n" +
      "template PageShell from \"../../templates/page_shell.terl.html\" {\n" +
      "    title: Binary\n" +
      "}.\n",
    filePath
  );

  assert.strictEqual(declarations.length, 1);
  assert.strictEqual(declarations[0].name, "PageShell");
  assert.strictEqual(declarations[0].tag, "page-shell");
  assert.strictEqual(
    declarations[0].sourceFile,
    path.resolve("/repo/src/app", "../../templates/page_shell.terl.html")
  );
  assert.strictEqual(declarations[0].line, 1);
}

/**
 * Verifies component tag links are found only for declared template tags.
 *
 * @returns {void}
 *
 * @description
 * Scans a template body containing ordinary HTML and a custom component tag,
 * then confirms only the custom tag receives a link target.
 */
function testFindTemplateComponentTagLinks() {
  const declaration = { name: "PageShell" };
  const links = findTemplateComponentTagLinks(
    "<main><page-shell title=\"Home\"></page-shell><section></section></main>",
    new Map([["page-shell", declaration]])
  );

  assert.strictEqual(links.length, 1);
  assert.strictEqual(links[0].tag, "page-shell");
  assert.strictEqual(links[0].declaration, declaration);
}

testTemplateTagFromPath();
testParseTemplateDeclarations();
testFindTemplateComponentTagLinks();

console.log("terlan vscode template link tests passed");
