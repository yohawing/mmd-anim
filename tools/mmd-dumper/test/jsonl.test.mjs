import test from "node:test";
import assert from "node:assert/strict";
import { parseOracleJsonl } from "../src/jsonl.mjs";
import { createFakeRecord } from "../src/fake-record.mjs";

test("parses non-empty JSONL records and skips blank lines", () => {
  const record = createFakeRecord({
    mmdVersion: "9.32-x64",
    projectRaw: "scene.pmm",
    dump: { bones: true, morphs: true },
  }, 0);

  const parsed = parseOracleJsonl(`${JSON.stringify(record)}\n\n`, "inline");

  assert.equal(parsed.length, 1);
  assert.equal(parsed[0].frame, 0);
});
