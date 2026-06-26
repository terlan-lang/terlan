"use strict";

const path = require("path");

const TERLAN_TEMPLATE_SUFFIXES = [".terl.html", ".terl.md"];
const HTML_VOID_OR_BUILTIN_TAGS = new Set([
  "a",
  "abbr",
  "address",
  "area",
  "article",
  "aside",
  "audio",
  "b",
  "base",
  "bdi",
  "bdo",
  "blockquote",
  "body",
  "br",
  "button",
  "canvas",
  "caption",
  "cite",
  "code",
  "col",
  "colgroup",
  "data",
  "datalist",
  "dd",
  "del",
  "details",
  "dfn",
  "dialog",
  "div",
  "dl",
  "dt",
  "em",
  "embed",
  "fieldset",
  "figcaption",
  "figure",
  "footer",
  "form",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "head",
  "header",
  "hr",
  "html",
  "i",
  "iframe",
  "img",
  "input",
  "ins",
  "kbd",
  "label",
  "legend",
  "li",
  "link",
  "main",
  "map",
  "mark",
  "menu",
  "meta",
  "meter",
  "nav",
  "noscript",
  "object",
  "ol",
  "optgroup",
  "option",
  "output",
  "p",
  "picture",
  "pre",
  "progress",
  "q",
  "rp",
  "rt",
  "ruby",
  "s",
  "samp",
  "script",
  "section",
  "select",
  "slot",
  "small",
  "source",
  "span",
  "strong",
  "style",
  "sub",
  "summary",
  "sup",
  "svg",
  "table",
  "tbody",
  "td",
  "template",
  "textarea",
  "tfoot",
  "th",
  "thead",
  "time",
  "title",
  "tr",
  "track",
  "u",
  "ul",
  "var",
  "video",
  "wbr"
]);

/**
 * Derives the compiler-visible component tag for a Terlan template path.
 *
 * @param {string} templatePath Source path ending in `.terl.html` or `.terl.md`.
 * @returns {string | undefined} Normalized component tag.
 *
 * @description
 * Mirrors `terlan_html::template_tag_from_path`: strips the Terlan template
 * suffix, lowercases ASCII uppercase letters, converts `_` to `-`, preserves
 * existing `-`, and rejects invalid or repeated separators.
 */
function templateTagFromPath(templatePath) {
  const fileName = path.basename(String(templatePath));
  const suffix = TERLAN_TEMPLATE_SUFFIXES.find((candidate) => fileName.endsWith(candidate));
  if (!suffix) {
    return undefined;
  }
  const stem = fileName.slice(0, fileName.length - suffix.length);
  if (!stem) {
    return undefined;
  }

  let tag = "";
  let previousWasDash = false;
  for (const char of stem) {
    if (/[a-z0-9]/.test(char)) {
      tag += char;
      previousWasDash = false;
    } else if (/[A-Z]/.test(char)) {
      tag += char.toLowerCase();
      previousWasDash = false;
    } else if (char === "_" || char === "-") {
      if (!tag || previousWasDash) {
        return undefined;
      }
      tag += "-";
      previousWasDash = true;
    } else {
      return undefined;
    }
  }
  if (tag.endsWith("-")) {
    return undefined;
  }
  return tag;
}

/**
 * Parses `template Name from "..."` declarations from one Terlan module.
 *
 * @param {string} text Terlan module source.
 * @param {string} filePath Filesystem path for the Terlan module.
 * @returns {Array<{name: string, tag: string, sourcePath: string, sourceFile: string, filePath: string, line: number, character: number}>}
 *
 * @description
 * Extracts the declaration data needed by the VS Code template document-link
 * provider without running the full compiler in the extension host.
 */
function parseTemplateDeclarations(text, filePath) {
  const source = String(text);
  const declarations = [];
  const pattern = /^\s*template\s+([A-Z][A-Za-z0-9_]*)\s+from\s+"([^"]+)"/gm;
  let match;

  while ((match = pattern.exec(source)) !== null) {
    const before = source.slice(0, match.index);
    const line = before.split(/\r?\n/).length - 1;
    const lineStart = Math.max(before.lastIndexOf("\n") + 1, 0);
    const character = match.index + match[0].indexOf(match[1]) - lineStart;
    const sourcePath = match[2];
    const tag = templateTagFromPath(sourcePath);
    if (!tag) {
      continue;
    }
    declarations.push({
      name: match[1],
      tag,
      sourcePath,
      sourceFile: path.resolve(path.dirname(filePath), sourcePath),
      filePath,
      line,
      character
    });
  }

  return declarations;
}

/**
 * Finds HTML component tag occurrences that have Terlan template declarations.
 *
 * @param {string} html Template HTML source.
 * @param {Map<string, *> | Record<string, *>} declarationsByTag Declaration lookup by component tag.
 * @returns {Array<{tag: string, start: number, end: number, declaration: *}>}
 *
 * @description
 * Scans opening HTML tags, skips built-in HTML tags, and returns ranges for
 * tags whose normalized component name is present in the declaration index.
 */
function findTemplateComponentTagLinks(html, declarationsByTag) {
  const source = String(html);
  const links = [];
  const tagPattern = /<\s*([a-z][a-z0-9-]*)\b/g;
  let match;

  while ((match = tagPattern.exec(source)) !== null) {
    const tag = match[1];
    const previous = source[match.index + 1];
    if (previous === "/" || HTML_VOID_OR_BUILTIN_TAGS.has(tag)) {
      continue;
    }
    const declaration = declarationsByTag instanceof Map
      ? declarationsByTag.get(tag)
      : declarationsByTag[tag];
    if (!declaration) {
      continue;
    }
    const start = match.index + match[0].indexOf(tag);
    links.push({
      tag,
      start,
      end: start + tag.length,
      declaration
    });
  }

  return links;
}

module.exports = {
  findTemplateComponentTagLinks,
  parseTemplateDeclarations,
  templateTagFromPath
};
