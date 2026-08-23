#!/usr/bin/env node
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const outDir = resolve(root, "out", "mmd-oracle-dumper-package");
const artifacts = [
  {
    name: "mmd_oracle_dumper.dll",
    source: resolve(root, "out", "native-dll-smoke", "mmd_oracle_dumper.dll"),
    required: true,
  },
  {
    name: "MSIMG32.dll",
    source: resolve(root, "out", "native-proxy-smoke", "MSIMG32.dll"),
    required: true,
  },
  {
    name: "d3d9.dll",
    source: resolve(root, "out", "native-d3d9-smoke", "d3d9.dll"),
    required: false,
  },
  {
    name: "Plugin/mmd_oracle_plugin.dll",
    source: resolve(root, "out", "native-mmdplugin-smoke", "mmd_oracle_plugin.dll"),
    required: false,
  },
  {
    name: "README.md",
    source: resolve(root, "native", "README.md"),
    required: true,
  },
  {
    name: "oracle-v1.schema.json",
    source: resolve(root, "schema", "oracle-v1.schema.json"),
    required: true,
  },
];

await mkdir(outDir, { recursive: true });

const copied = [];
for (const artifact of artifacts) {
  if (!existsSync(artifact.source)) {
    if (artifact.required) {
      throw new Error(`Required artifact is missing: ${artifact.source}. Run pnpm -C MMDDumper native:test first.`);
    }
    continue;
  }
  const destination = resolve(outDir, artifact.name);
  await mkdir(dirname(destination), { recursive: true });
  await copyFile(artifact.source, destination);
  copied.push({ name: artifact.name, source: artifact.source, destination });
}

const exampleDumpPath = resolve(root, "fixtures", "sample-basic", "oracle.actual.jsonl").replaceAll("\\", "\\\\");
const installText = `# MMD Oracle Dumper local package

Preferred MMDPlugin install:

- Keep the MMDPlugin-provided \`d3d9.dll\` next to \`MikuMikuDance.exe\`.
- Copy \`Plugin/mmd_oracle_plugin.dll\` into the local MMD \`Plugin/\` folder.

Legacy proxy fallback:

- mmd_oracle_dumper.dll
- MSIMG32.dll
- d3d9.dll, only when intentionally using the old d3d9 proxy trigger

Optional first-load smoke environment variables:

\`\`\`powershell
$env:MMD_ORACLE_DUMP_PATH = "${exampleDumpPath}"
$env:MMD_ORACLE_DUMP_ON_PROXY_LOAD = "1"
$env:MMD_ORACLE_DUMP_ON_D3D9 = "1"
$env:MMD_ORACLE_DUMP_ON_MMDPLUGIN = "1"
\`\`\`

This package is a local oracle recorder prototype. It is not a generic injector and does not install anything globally.
`;

await writeFile(resolve(outDir, "INSTALL.md"), installText, "utf8");

const schema = JSON.parse(await readFile(resolve(root, "schema", "oracle-v1.schema.json"), "utf8"));
const manifest = {
  name: "mmd-oracle-dumper",
  schema: schema.$id,
  mmdVersion: "9.32-x64",
  files: copied.map((entry) => ({
    name: entry.name,
    destinationHint: entry.name === "oracle-v1.schema.json" || entry.name === "README.md" ? "documentation" : "next to MikuMikuDance.exe",
  })),
  safety: {
    genericInjector: false,
    globalInstall: false,
    networkAccess: false,
    targetProcess: "MikuMikuDance.exe",
  },
};

await writeFile(resolve(outDir, "package-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

console.log(
  JSON.stringify(
    {
      ok: true,
      outDir,
      files: copied.map((entry) => basename(entry.destination)),
    },
    null,
    2,
  ),
);
