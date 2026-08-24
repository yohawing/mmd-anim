import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { writeSyntheticVmd } from "./vmd-writer.mjs";

export async function writePmmInvestigationKit(outDir, options = {}) {
  await mkdir(outDir, { recursive: true });
  const frame = options.frame ?? 30;
  const position = options.position ?? [1, 2, 3];
  const weight = options.weight ?? 0.5;
  const modelName = options.modelName ?? "MMDDumper";
  const boneName = options.boneName ?? "センター";
  const morphName = options.morphName ?? "まばたき";
  const motions = [
    {
      name: "one-bone",
      file: join(outDir, "one-bone.vmd"),
      options: { modelName, boneName, frame, position },
    },
    {
      name: "one-morph",
      file: join(outDir, "one-morph.vmd"),
      options: { modelName, morphName, frame, weight },
    },
    {
      name: "one-bone-one-morph",
      file: join(outDir, "one-bone-one-morph.vmd"),
      options: { modelName, boneName, morphName, frame, position, weight },
    },
  ];
  const written = [];
  for (const motion of motions) {
    written.push({ name: motion.name, ...(await writeSyntheticVmd(motion.file, motion.options)) });
  }
  const manifest = {
    kind: "pmm-investigation-kit",
    frame,
    position,
    weight,
    modelName,
    boneName,
    morphName,
    motions: written,
    suggestedSearch: {
      int32: [frame],
      float32: [...position, weight],
    },
  };
  const manifestFile = join(outDir, "manifest.json");
  await writeFile(manifestFile, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return { outDir, manifestFile, ...manifest };
}
