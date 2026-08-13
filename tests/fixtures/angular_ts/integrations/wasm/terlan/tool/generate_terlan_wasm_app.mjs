import fs from "node:fs";
import path from "node:path";

const files = new Map([
  [path.join("src", "terlan", "angular", "wasm", "App.terl"), "module terlan.angular.wasm.App.\n\npub title(): String ->\n    \"Terlan Wasm Angular App\".\n\npub remaining(open: Int, completed: Int): Int ->\n    open - completed.\n\npub status(open: Int, completed: Int): String ->\n    if {\n        remaining(open, completed) == 0 -> \"complete\";\n        true -> \"active\"\n    }.\n"],
  ["terlan.toml", "[package]\nname = \"angular-ts-terlan-wasm\"\nversion = \"0.1.0\"\nnamespace = \"terlan.angular.wasm\"\n\n[build]\nsource_roots = [\"src\"]\nartifact = \"wasm-browser\"\n\n[target.wasm]\nprofile = \"browser\"\nexports = [\"terlan.angular.wasm.App.title\", \"terlan.angular.wasm.App.remaining\", \"terlan.angular.wasm.App.status\"]\nbridge = \"generated-js\"\ncapabilities = [\"browser.console\", \"browser.scope\"]\nvalidation_engine = \"browser-playwright\"\n"],
  [path.join("examples", "basic_app", "angular-ts.json"), "{\n  \"module\": \"terlanWasmDemo\",\n  \"package\": \"../pkg/terlan_angular_wasm_app.js\",\n  \"appTemplatePath\": \"index.html\",\n  \"registrations\": [\n    {\n      \"kind\": \"component\",\n      \"name\": \"terlanWasmApp\",\n      \"templatePath\": \"templates/terlan-wasm-app.html\"\n    }\n  ]\n}\n"],
  [path.join("examples", "basic_app", "index.html"), "<!doctype html>\n<html lang=\"en\" ng-app=\"terlanWasmDemo\">\n  <head>\n    <meta charset=\"utf-8\">\n    <title>Terlan Wasm Angular Demo</title>\n  </head>\n  <body>\n    <main>\n      <h3>Terlan-authored AngularTS Wasm App</h3>\n      <p id=\"terlan-wasm-status\">reserved backend integration</p>\n    </main>\n  </body>\n</html>\n"],
]);
const check = process.argv.includes("--check");

if (check) {
  for (const [filePath, expected] of files) {
    const actual = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : "";
    if (actual !== expected) {
      console.error(`${filePath} is stale; run make generate`);
      process.exit(1);
    }
  }
  process.exit(0);
}

for (const [filePath, contents] of files) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents);
}
