import { readOracleJsonl } from "./jsonl.mjs";

export async function compareOracleFiles(options) {
  const expected = await readOracleJsonl(options.expected);
  const actual = await readOracleJsonl(options.actual);
  return compareOracleRecords(expected, actual, options);
}

export function compareOracleRecords(expected, actual, options = {}) {
  const matrixEpsilon = options.matrixEpsilon ?? 1e-4;
  const morphEpsilon = options.morphEpsilon ?? 1e-4;
  const differences = [];

  if (expected.length !== actual.length) {
    differences.push({
      kind: "record-count",
      expected: expected.length,
      actual: actual.length,
    });
  }

  const count = Math.min(expected.length, actual.length);
  for (let i = 0; i < count; i += 1) {
    compareRecord(expected[i], actual[i], i, matrixEpsilon, morphEpsilon, differences);
  }

  return {
    ok: differences.length === 0,
    matrixEpsilon,
    morphEpsilon,
    expectedRecordCount: expected.length,
    actualRecordCount: actual.length,
    differences,
  };
}

function compareRecord(expected, actual, recordIndex, matrixEpsilon, morphEpsilon, differences) {
  if (expected.frame !== actual.frame) {
    differences.push({
      kind: "frame",
      recordIndex,
      expected: expected.frame,
      actual: actual.frame,
    });
  }

  const actualModelsByKey = new Map(actual.models.map((model) => [modelKey(model), model]));
  for (const expectedModel of expected.models) {
    const actualModel = actualModelsByKey.get(modelKey(expectedModel));
    if (!actualModel) {
      differences.push({
        kind: "missing-model",
        recordIndex,
        frame: expected.frame,
        model: modelKey(expectedModel),
      });
      continue;
    }
    compareModel(expected.frame, expectedModel, actualModel, matrixEpsilon, morphEpsilon, differences);
  }
}

function compareModel(frame, expected, actual, matrixEpsilon, morphEpsilon, differences) {
  const actualBonesByName = new Map(actual.bones.map((bone) => [bone.name, bone]));
  for (const expectedBone of expected.bones) {
    const actualBone = actualBonesByName.get(expectedBone.name);
    if (!actualBone) {
      differences.push({
        kind: "missing-bone",
        frame,
        model: modelKey(expected),
        bone: expectedBone.name,
      });
      continue;
    }
    const maxAbsDiff = maxMatrixAbsDiff(expectedBone.worldMatrix, actualBone.worldMatrix);
    if (maxAbsDiff > matrixEpsilon) {
      differences.push({
        kind: "bone-world-matrix",
        frame,
        model: modelKey(expected),
        bone: expectedBone.name,
        maxAbsDiff,
        threshold: matrixEpsilon,
      });
    }
  }

  const actualMorphsByName = new Map(actual.morphs.map((morph) => [morph.name, morph]));
  for (const expectedMorph of expected.morphs) {
    const actualMorph = actualMorphsByName.get(expectedMorph.name);
    if (!actualMorph) {
      differences.push({
        kind: "missing-morph",
        frame,
        model: modelKey(expected),
        morph: expectedMorph.name,
      });
      continue;
    }
    const absDiff = Math.abs(expectedMorph.weight - actualMorph.weight);
    if (absDiff > morphEpsilon) {
      differences.push({
        kind: "morph-weight",
        frame,
        model: modelKey(expected),
        morph: expectedMorph.name,
        absDiff,
        threshold: morphEpsilon,
        expected: expectedMorph.weight,
        actual: actualMorph.weight,
      });
    }
  }
}

function maxMatrixAbsDiff(expected, actual) {
  let max = 0;
  for (let i = 0; i < 16; i += 1) {
    max = Math.max(max, Math.abs(expected[i] - actual[i]));
  }
  return max;
}

function modelKey(model) {
  return `${model.index}:${model.filename}`;
}
