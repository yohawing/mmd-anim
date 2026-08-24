import { parsePmmDocumentKeyframes } from "../src/pmm-document-keyframes.mjs";
import { readFile } from "node:fs/promises";

async function dump(label, path) {
  try {
    const pmm = parsePmmDocumentKeyframes(await readFile(path));
    console.log(`\n=== ${label} (${path}) ===`);
    console.log("version", pmm.document?.version, "models", pmm.counts?.models);
    for (const m of pmm.models) {
      console.log(
        `  model[${m.slot}] "${m.nameJa}": selfShadowEnabled=${m.selfShadowEnabled}` +
          ` edgeWidth=${m.edgeWidth} drawOrderIndex=${m.drawOrderIndex} transformOrderIndex=${m.transformOrderIndex}` +
          ` visible=${m.visible} bones=${m.boneCount} morphs=${m.morphCount}`,
      );
    }
  } catch (e) {
    console.log(`\n=== ${label}: parse error: ${e.message}`);
  }
}

await dump("GROUND-TRUTH (real character, self-shadows)", "F:/Develop/MMDDev/GoldenOracle/temp.pmm");
await dump(
  "GENERATED (body-on, no visible shadow)",
  "F:/Develop/MMDDev/GoldenOracle/runs/fixture-render/fixture-render-generated-self-shadow-mmd-self-shadow-body-on/scene.pmm",
);
