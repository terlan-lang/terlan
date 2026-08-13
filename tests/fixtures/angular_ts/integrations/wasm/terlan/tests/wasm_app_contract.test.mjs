import fs from "node:fs";

const source = fs.readFileSync("src/terlan/angular/wasm/App.terl", "utf8");
const project = fs.readFileSync("terlan.toml", "utf8");
const manifest = JSON.parse(fs.readFileSync("examples/basic_app/angular-ts.json", "utf8"));
const packageManifest = JSON.parse(fs.readFileSync("terlan.wasm.integration.json", "utf8"));

const sourceMarkers = [
  "module terlan.angular.wasm.App.",
  "pub title(): String ->",
  "pub remaining(open: Int, completed: Int): Int ->",
  "pub status(open: Int, completed: Int): String ->",
];

const projectMarkers = [
  'artifact = "wasm-browser"',
  '[target.wasm]',
  'profile = "browser"',
  'bridge = "generated-js"',
  'browser.scope',
];

for (const marker of sourceMarkers) {
  if (!source.includes(marker)) {
    throw new Error(`missing Terlan Wasm app source marker: ${marker}`);
  }
}

for (const marker of projectMarkers) {
  if (!project.includes(marker)) {
    throw new Error(`missing Terlan Wasm project marker: ${marker}`);
  }
}

if (manifest.module !== "terlanWasmDemo") {
  throw new Error("unexpected Terlan Wasm AngularTS module name");
}
if (!manifest.registrations.some((entry) => entry.kind === "component" && entry.name === "terlanWasmApp")) {
  throw new Error("missing Terlan Wasm component registration");
}
if (packageManifest.status !== "reserved-backend-integration") {
  throw new Error("Terlan Wasm integration status must remain explicit");
}
