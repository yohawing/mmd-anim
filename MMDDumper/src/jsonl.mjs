import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import { assertOracleRecord } from "./schema.mjs";

export async function readOracleJsonl(path) {
  const text = await readFile(path, "utf8");
  return parseOracleJsonl(text, path);
}

export function parseOracleJsonl(text, label = "jsonl") {
  const records = [];
  const lines = text.split(/\r?\n/);
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i].trim();
    if (line.length === 0) {
      continue;
    }
    let value;
    try {
      value = JSON.parse(line);
    } catch (cause) {
      throw new Error(`${label}:${i + 1}: invalid JSON: ${cause.message}`);
    }
    records.push(assertOracleRecord(value, `${label}:${i + 1}`));
  }
  return records;
}

export async function writeOracleJsonl(path, records) {
  await mkdir(dirname(path), { recursive: true });
  const body = records.map((record) => JSON.stringify(assertOracleRecord(record))).join("\n");
  await writeFile(path, `${body}\n`, "utf8");
}
