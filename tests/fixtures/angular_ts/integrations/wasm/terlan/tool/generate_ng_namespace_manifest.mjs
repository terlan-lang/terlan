import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const integrationRoot = process.cwd();
const angularRoot = path.resolve(integrationRoot, "../../..");
const sourcePath = path.join(angularRoot, "@types", "namespace.d.ts");
const outputPath = path.join("generated", "ng_namespace_inputs.json");
const version = process.env.TERLAN_BINDGEN_VERSION || "";

if (!version) {
  console.error("TERLAN_BINDGEN_VERSION is required");
  process.exit(1);
}

if (!fs.existsSync(sourcePath)) {
  console.error(`missing AngularTS namespace declaration file: ${sourcePath}`);
  process.exit(1);
}

const source = fs.readFileSync(sourcePath);
const sha256 = crypto.createHash("sha256").update(source).digest("hex");
const manifest = {
  schema: "terlan.std.js.input-manifest.v1",
  generator: {
    name: "terlc",
    version,
    profile: "angular-ts-namespace",
    oxc_parser: true
  },
  target_profile: "js.browser",
  source_package: {
    name: "typescript",
    version: "local",
    resolution: "@types/namespace.d.ts"
  },
  inputs: [
    {
      path: "@types/namespace.d.ts",
      sha256,
      kind: "typescript-declaration",
      namespace: "terlan.angular"
    }
  ]
};

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
