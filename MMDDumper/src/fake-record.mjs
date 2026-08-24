import { DUMPER_VERSION } from "./schema.mjs";
import { writeOracleJsonl } from "./jsonl.mjs";

const IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

export async function writeFakeOracleDump(fixture) {
  const records = fixture.frames.map((frame) => createFakeRecord(fixture, frame));
  await writeOracleJsonl(fixture.output, records);
  return records;
}

export function createFakeRecord(fixture, frame) {
  return {
    schemaVersion: 1,
    source: {
      mmdVersion: fixture.mmdVersion,
      dumperVersion: DUMPER_VERSION,
      project: fixture.projectRaw,
    },
    frame,
    models: [
      {
        index: 0,
        name: "fake-model",
        filename: "fake-model.pmd",
        visible: true,
        bones: fixture.dump.bones
          ? [
              {
                index: 0,
                name: "センター",
                worldMatrix: withTranslation(IDENTITY, frame / 30, 0, 0),
              },
            ]
          : [],
        morphs: fixture.dump.morphs
          ? [
              {
                index: 0,
                name: "まばたき",
                weight: Number((frame / 60).toFixed(6)),
              },
            ]
          : [],
      },
    ],
  };
}

function withTranslation(matrix, x, y, z) {
  const next = [...matrix];
  next[12] = x;
  next[13] = y;
  next[14] = z;
  return next;
}
