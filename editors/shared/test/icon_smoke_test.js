"use strict";

const assert = require("assert");
const fs = require("fs");
const path = require("path");

const SHARED_ROOT = path.resolve(__dirname, "..");
const ICON_PATH = path.join(SHARED_ROOT, "icons", "terlan-file.svg");
const EXTENSION_ICON_PATH = path.join(SHARED_ROOT, "icons", "terlan-extension.svg");
const PNG_ICON_ROOT = path.join(SHARED_ROOT, "icons", "png");
const PNG_ICON_SIZES = [16, 24, 32, 64, 128];

/**
 * Reads the canonical Terlan editor icon.
 *
 * @returns {string} UTF-8 SVG source.
 *
 * @description
 * Loads the shared icon source so package smoke tests can validate the editor
 * visual identity without invoking any editor-specific packaging tool.
 */
function readCanonicalIcon() {
  return fs.readFileSync(ICON_PATH, "utf8");
}

/**
 * Reads the PNG image dimensions from a Terlan editor icon variant.
 *
 * @param {Buffer} png PNG file bytes.
 * @returns {{width: number, height: number}} Parsed image dimensions.
 *
 * @description
 * Validates the PNG signature and decodes the IHDR width and height fields so
 * icon size checks stay dependency-free and focused on packaging integrity.
 */
function readPngDimensions(png) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  assert.ok(png.subarray(0, signature.length).equals(signature), "invalid PNG signature");

  return {
    width: png.readUInt32BE(16),
    height: png.readUInt32BE(20),
  };
}

/**
 * Verifies the canonical icon file exists.
 *
 * @returns {void}
 *
 * @description
 * Fails fast when editor packages would otherwise reference a missing shared
 * icon asset.
 */
function testIconExists() {
  assert.ok(fs.existsSync(ICON_PATH), "missing canonical Terlan editor icon");
  assert.ok(fs.existsSync(EXTENSION_ICON_PATH), "missing canonical Terlan extension icon");
}

/**
 * Verifies the canonical icon is a simple SVG.
 *
 * @returns {void}
 *
 * @description
 * Locks the icon as text source rather than generated binary package output.
 */
function testIconIsSvgSource() {
  const icon = readCanonicalIcon();

  assert.ok(icon.startsWith("<svg "), "icon must start with an SVG root");
  assert.ok(icon.includes('viewBox="0 0 128 128"'), "icon must define viewBox");
  assert.ok(icon.includes("Terlan file"), "icon must carry an accessible label");
  assert.ok(!icon.includes("terlan-file-fold"), "file icon must not use folded-corner styling");
  assert.ok(!icon.includes("h22L84"), "file icon must not include the folded-corner path");
  assert.ok(!icon.includes("<script"), "icon must not contain script content");
}

/**
 * Verifies the extension icon is a no-fold Terlan mark.
 *
 * @returns {void}
 *
 * @description
 * Keeps plugin/package identity separate from file-document identity by
 * checking that the extension icon has no folded-corner path or file label.
 */
function testExtensionIconIsNoFoldMark() {
  const icon = fs.readFileSync(EXTENSION_ICON_PATH, "utf8");

  assert.ok(icon.startsWith("<svg "), "extension icon must start with an SVG root");
  assert.ok(icon.includes('viewBox="0 0 128 128"'), "extension icon must define viewBox");
  assert.ok(icon.includes('aria-label="Terlan"'), "extension icon must label the language");
  assert.ok(!icon.includes("Terlan file"), "extension icon must not use file icon label");
  assert.ok(!icon.includes("terlan-file-fold"), "extension icon must not use folded-corner styling");
  assert.ok(!icon.includes("h22L84"), "extension icon must not include the folded-corner path");
  assert.ok(!icon.includes("<script"), "extension icon must not contain script content");
}

/**
 * Verifies all canonical PNG icon variants exist with matching dimensions.
 *
 * @returns {void}
 *
 * @description
 * Locks the package-ready raster icon assets used by editor ecosystems that
 * require fixed-size PNG files in addition to the shared SVG source.
 */
function testPngIconVariants() {
  for (const size of PNG_ICON_SIZES) {
    const iconPath = path.join(PNG_ICON_ROOT, `terlan-file-${size}.png`);

    assert.ok(fs.existsSync(iconPath), `missing PNG icon variant ${size}`);

    const dimensions = readPngDimensions(fs.readFileSync(iconPath));
    assert.deepStrictEqual(
      dimensions,
      { width: size, height: size },
      `PNG icon variant ${size} must be ${size}x${size}`
    );
  }

  const extensionIconPath = path.join(PNG_ICON_ROOT, "terlan-extension-128.png");
  assert.ok(fs.existsSync(extensionIconPath), "missing extension PNG icon variant");
  assert.deepStrictEqual(
    readPngDimensions(fs.readFileSync(extensionIconPath)),
    { width: 128, height: 128 },
    "extension PNG icon must be 128x128"
  );
}

testIconExists();
testIconIsSvgSource();
testExtensionIconIsNoFoldMark();
testPngIconVariants();

console.log("terlan shared editor icon smoke tests passed");
