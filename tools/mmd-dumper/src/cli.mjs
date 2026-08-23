#!/usr/bin/env node
import { readdir, readFile, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { deflateSync } from "node:zlib";
import { readFixture } from "./fixture.mjs";
import { readOracleJsonl } from "./jsonl.mjs";
import { compareOracleFiles } from "./compare.mjs";
import { writeFakeOracleDump } from "./fake-record.mjs";
import { captureWithMmd, exportAviWithMmd, exportImageWithMmd, exportMp4WithMmd, recordDirectWithMmd, recordWithMmd, writeDoneFile } from "./runner.mjs";
import { prepareOracleFromVmd, recordOracleFromVmd } from "./oracle-from-vmd.mjs";
import { prepareOracleBatch, recordOracleBatch } from "./oracle-batch.mjs";
import { exportStaticRenderBatch, prepareStaticRenderBatch } from "./static-render-batch.mjs";
import { readModelInventory, writeBasePmmFromPmx, writePmmFromPmxVmd } from "./pmm-from-pmx-vmd.mjs";
import { readPmmManifest } from "./pmm-manifest.mjs";
import { readPmmModelSlotReport } from "./pmm-model-slots.mjs";
import { createPmmFromTemplateDryRun, writePmmFromTemplateProfile } from "./pmm-template-create.mjs";
import { readPmmAnalysis } from "./pmm-analysis.mjs";
import { readPmmDiff } from "./pmm-diff.mjs";
import { readPmmDocumentKeyframes } from "./pmm-document-keyframes.mjs";
import { writePmmParameterDump } from "./pmm-parameter-dump.mjs";
import { readPmmDocumentVmdComparison } from "./pmm-document-vmd-compare.mjs";
import { writePmmDocumentVmdKeyframePatch } from "./pmm-document-vmd-patch.mjs";
import { readPmmInvestigation } from "./pmm-investigation.mjs";
import { writePmmInvestigationKit } from "./pmm-investigation-kit.mjs";
import {
  readPmmKeyframeProfileCheck,
  readPmmKeyframeProfileRegistryCheck,
  readPmmKeyframesWithProfile,
  readPmmKeyframesWithProfileComparison,
  readPmmKeyframesWithProfileRegistry,
  readPmmVmdKeyframeComparison,
  readPmmVmdKeyframes,
} from "./pmm-keyframe-extract.mjs";
import { readPmmKeyCountDelta } from "./pmm-key-count-delta.mjs";
import { readPmmVmdDiffClusters } from "./pmm-vmd-diff-clusters.mjs";
import {
  readPmmPatchProfileRegistryInspection,
  readPmmPatchProfileRegistryInventory,
  readPmmVmdAnyPatchRegistryPlan,
  readPmmVmdPatchCompatibility,
  readPmmVmdKeyCountDeltaPatchRegistryPlan,
  readPmmVmdPatchRegistryPlan,
  writeUsablePmmPatchProfileRegistry,
  writePmmVmdDiffClusterPatch,
  writePmmVmdKeyCountDeltaPatch,
  writePmmVmdKeyCountDeltaPatchFromProfileRegistry,
  writePmmVmdPatchFromAnyProfileRegistry,
  writePmmVmdPatchFromProfileRegistry,
} from "./pmm-vmd-diff-patch.mjs";
import {
  readPmmFixtureMotionReport,
  writePmmFixtureMotionPatch,
  writePmmScalarRewrite,
  writePmmUnittestBoneKeys,
  writePmmUnittestVmdBoneKeys,
} from "./pmm-fixture-motion.mjs";
import { readPmmMotionScan } from "./pmm-motion-scan.mjs";
import { patchPmmMotionRecordSlice, readPmmMotionRecords, readPmmMotionRecordSlice } from "./pmm-motion-records.mjs";
import { mapVmdBoneFramesToPmm } from "./pmm-vmd-bone-map.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";
import { compareVmdToPmmMotion } from "./vmd-pmm-compare.mjs";
import { writeSyntheticVmd } from "./vmd-writer.mjs";
import { verifyFixtureCoverage } from "./coverage.mjs";
import { stageMmdCompatiblePmx } from "./pmx-mmd-compat.mjs";

const [, , command, ...rawArgs] = process.argv;
const args = rawArgs[0] === "--" ? rawArgs.slice(1) : rawArgs;

try {
  switch (command) {
    case "validate":
      await validateCommand(args);
      break;
    case "compare":
      await compareCommand(args);
      break;
    case "fake-record":
      await fakeRecordCommand(args);
      break;
    case "record":
      await recordCommand(args);
      break;
    case "record-direct":
      await recordDirectCommand(args);
      break;
    case "capture":
      await captureCommand(args);
      break;
    case "export-image":
      await exportImageCommand(args);
      break;
    case "export-avi":
      await exportAviCommand(args);
      break;
    case "export-mp4":
      await exportMp4Command(args);
      break;
    case "oracle-from-vmd":
      await oracleFromVmdCommand(args);
      break;
    case "oracle-batch":
      await oracleBatchCommand(args);
      break;
    case "static-render":
      await staticRenderCommand(args);
      break;
    case "create-base-pmm-from-pmx":
      await createBasePmmFromPmxCommand(args);
      break;
    case "create-pmm-from-pmx-vmd":
      await createPmmFromPmxVmdCommand(args);
      break;
    case "stage-pmx":
      await stagePmxCommand(args);
      break;
    case "inspect-pmx":
      await inspectModelCommand(args);
      break;
    case "inspect-model":
      await inspectModelCommand(args);
      break;
    case "inspect-pmm":
      await inspectPmmCommand(args);
      break;
    case "inspect-pmm-model-slots":
      await inspectPmmModelSlotsCommand(args);
      break;
    case "inspect-pmm-document-keyframes":
      await inspectPmmDocumentKeyframesCommand(args);
      break;
    case "dump-pmm-parameters":
      await dumpPmmParametersCommand(args);
      break;
    case "compare-pmm-document-vmd-keyframes":
      await comparePmmDocumentVmdKeyframesCommand(args);
      break;
    case "patch-pmm-document-vmd-keyframes":
      await patchPmmDocumentVmdKeyframesCommand(args);
      break;
    case "create-pmm-from-template":
      await createPmmFromTemplateCommand(args);
      break;
    case "analyze-pmm":
      await analyzePmmCommand(args);
      break;
    case "diff-pmm":
      await diffPmmCommand(args);
      break;
    case "investigate-pmm":
      await investigatePmmCommand(args);
      break;
    case "cluster-pmm-vmd-diff":
      await clusterPmmVmdDiffCommand(args);
      break;
    case "analyze-pmm-key-count-delta":
      await analyzePmmKeyCountDeltaCommand(args);
      break;
    case "extract-pmm-vmd-keyframes":
      await extractPmmVmdKeyframesCommand(args);
      break;
    case "compare-pmm-vmd-keyframes":
      await comparePmmVmdKeyframesCommand(args);
      break;
    case "extract-pmm-keyframes-with-profile":
      await extractPmmKeyframesWithProfileCommand(args);
      break;
    case "extract-pmm-keyframes-with-profile-registry":
      await extractPmmKeyframesWithProfileRegistryCommand(args);
      break;
    case "check-pmm-keyframe-profile":
      await checkPmmKeyframeProfileCommand(args);
      break;
    case "check-pmm-keyframe-profile-registry":
      await checkPmmKeyframeProfileRegistryCommand(args);
      break;
    case "compare-pmm-keyframes-with-profile":
      await comparePmmKeyframesWithProfileCommand(args);
      break;
    case "patch-pmm-vmd-diff-cluster":
      await patchPmmVmdDiffClusterCommand(args);
      break;
    case "inspect-pmm-patch-profile-registry":
      await inspectPmmPatchProfileRegistryCommand(args);
      break;
    case "inventory-pmm-patch-profile-registries":
      await inventoryPmmPatchProfileRegistriesCommand(args);
      break;
    case "write-usable-pmm-patch-profile-registry":
      await writeUsablePmmPatchProfileRegistryCommand(args);
      break;
    case "check-pmm-vmd-patch-compatibility":
      await checkPmmVmdPatchCompatibilityCommand(args);
      break;
    case "plan-pmm-vmd-patch-from-any-profile-registry":
      await planPmmVmdPatchFromAnyProfileRegistryCommand(args);
      break;
    case "patch-pmm-vmd-from-any-profile-registry":
      await patchPmmVmdFromAnyProfileRegistryCommand(args);
      break;
    case "plan-pmm-vmd-patch-from-profile-registry":
      await planPmmVmdPatchFromProfileRegistryCommand(args);
      break;
    case "patch-pmm-vmd-from-profile-registry":
      await patchPmmVmdFromProfileRegistryCommand(args);
      break;
    case "plan-pmm-vmd-key-count-delta-from-profile-registry":
      await planPmmVmdKeyCountDeltaFromProfileRegistryCommand(args);
      break;
    case "patch-pmm-vmd-key-count-delta-from-profile-registry":
      await patchPmmVmdKeyCountDeltaFromProfileRegistryCommand(args);
      break;
    case "patch-pmm-vmd-key-count-delta":
      await patchPmmVmdKeyCountDeltaCommand(args);
      break;
    case "write-test-vmd":
      await writeTestVmdCommand(args);
      break;
    case "inspect-vmd":
      await inspectVmdCommand(args);
      break;
    case "compare-vmd-pmm-motion":
      await compareVmdPmmMotionCommand(args);
      break;
    case "map-vmd-pmm-bone-frames":
      await mapVmdPmmBoneFramesCommand(args);
      break;
    case "write-pmm-investigation-kit":
      await writePmmInvestigationKitCommand(args);
      break;
    case "analyze-pmm-fixture-motion":
      await analyzePmmFixtureMotionCommand(args);
      break;
    case "patch-pmm-fixture-motion":
      await patchPmmFixtureMotionCommand(args);
      break;
    case "rewrite-pmm-scalars":
      await rewritePmmScalarsCommand(args);
      break;
    case "write-pmm-unittest-bone-keys":
      await writePmmUnittestBoneKeysCommand(args);
      break;
    case "write-pmm-unittest-vmd-bone-keys":
      await writePmmUnittestVmdBoneKeysCommand(args);
      break;
    case "scan-pmm-motion":
      await scanPmmMotionCommand(args);
      break;
    case "extract-pmm-motion-records":
      await extractPmmMotionRecordsCommand(args);
      break;
    case "dump-pmm-motion-records":
      await dumpPmmMotionRecordsCommand(args);
      break;
    case "patch-pmm-motion-records":
      await patchPmmMotionRecordsCommand(args);
      break;
    case "verify-coverage":
      await verifyCoverageCommand(args);
      break;
    default:
      usage();
      process.exitCode = 1;
      break;
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}

async function validateCommand(args) {
  const file = positional(args, 0);
  const records = await readOracleJsonl(file);
  console.log(JSON.stringify({ ok: true, file, records: records.length }, null, 2));
}

async function compareCommand(args) {
  const options = parseFlags(args);
  const report = await compareOracleFiles({
    expected: requireFlag(options, "expected"),
    actual: requireFlag(options, "actual"),
    matrixEpsilon: optionalNumber(options, "matrix-epsilon"),
    morphEpsilon: optionalNumber(options, "morph-epsilon"),
  });
  const text = JSON.stringify(report, null, 2);
  if (options.out) {
    await writeFile(options.out, `${text}\n`, "utf8");
  }
  console.log(text);
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function fakeRecordCommand(args) {
  const options = parseFlags(args);
  const fixture = await readFixture(requireFlag(options, "fixture"));
  const records = await writeFakeOracleDump(fixture);
  await writeDoneFile(fixture.done, { mode: "fake", records: records.length });
  console.log(JSON.stringify({ ok: true, output: fixture.output, done: fixture.done, records: records.length }, null, 2));
}

async function recordCommand(args) {
  const options = parseFlags(args);
  const fixturePath = requireFlag(options, "fixture");
  const fixture = await readFixture(fixturePath);
  const records = await recordWithMmd(fixture, { fixturePath, trigger: options.trigger });
  console.log(JSON.stringify({ ok: true, output: fixture.output, records: records.length }, null, 2));
}

async function recordDirectCommand(args) {
  const options = parseFlags(args);
  const result = await recordDirectWithMmd({
    template: options.template,
    model: requireFlag(options, "model"),
    motion: options.motion,
    frames: parseFrames(options.frames ?? "0,30,60"),
    output: options.output,
    outDir: options["out-dir"],
    projectOut: options["project-out"],
    mmdExe: options["mmd-exe"],
    timeoutMs: optionalNumber(options, "timeout-ms") ?? 60000,
    trigger: options.trigger,
  });
  console.log(
    JSON.stringify(
      {
        ok: true,
        project: result.fixture.project,
        output: result.fixture.output,
        records: result.records.length,
        replacements: result.patch.replacements,
      },
      null,
      2,
    ),
  );
}

async function captureCommand(args) {
  const options = parseFlags(args);
  const fixturePath = requireFlag(options, "fixture");
  const fixture = await readFixture(fixturePath);
  const frames = options.frames ? parseFrames(options.frames) : fixture.frames;
  const captureDir = resolve(options["capture-dir"] ?? "out/capture");
  const result =
    options["convert-existing"] === "true"
      ? { records: [] }
      : await captureWithMmd(fixture, {
          fixturePath,
          frames,
          captureDir,
          timeoutMs: optionalNumber(options, "timeout-ms") ?? fixture.timeoutMs,
          sendKeyAfterMs: optionalNumber(options, "send-key-after-ms") ?? 3000,
          trigger: options.trigger,
        });
  const files = (await readdir(captureDir, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && /^frame_\d+\.bmp$/u.test(entry.name))
    .map((entry) => resolve(captureDir, entry.name));
  const pngFiles = await writeCapturePngs(files, { cropContent: options["crop-content"] === "true" });
  console.log(
    JSON.stringify(
      {
        ok: files.length >= frames.length,
        captureDir,
        requestedFrames: frames,
        files,
        pngFiles,
        records: result.records.length,
      },
      null,
      2,
    ),
  );
  if (files.length < frames.length) {
    process.exitCode = 1;
  }
}

async function exportImageCommand(args) {
  const options = parseFlags(args);
  const fixturePath = requireFlag(options, "fixture");
  const fixture = await readFixture(fixturePath);
  const bmpOutput = resolve(options.output ?? "out/mmd-export-image.bmp");
  const frame = optionalNumber(options, "frame") ?? fixture.frames[0] ?? 0;
  const result = await exportImageWithMmd(fixture, {
    output: bmpOutput,
    frame,
    loadWaitMs: optionalNumber(options, "load-wait-ms") ?? 5000,
    timeoutMs: optionalNumber(options, "timeout-ms") ?? fixture.timeoutMs,
  });
  const pngFiles = await writeCapturePngs([bmpOutput], { cropContent: options["crop-content"] === "true" });
  console.log(
    JSON.stringify(
      {
        ok: true,
        frame,
        output: result.output,
        pngFiles,
      },
      null,
      2,
    ),
  );
}

async function exportAviCommand(args) {
  const options = parseFlags(args);
  const fixturePath = requireFlag(options, "fixture");
  const fixture = await readFixture(fixturePath);
  const output = resolve(options.output ?? "out/mmd-export.avi");
  const result = await exportAviWithMmd(fixture, {
    output,
    startFrame: optionalNumber(options, "start-frame") ?? 0,
    endFrame: optionalNumber(options, "end-frame") ?? fixture.frames.at(-1) ?? 30,
    fps: optionalNumber(options, "fps") ?? 30,
    loadWaitMs: optionalNumber(options, "load-wait-ms") ?? 5000,
    timeoutMs: optionalNumber(options, "timeout-ms") ?? fixture.timeoutMs ?? 120000,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function exportMp4Command(args) {
  const options = parseFlags(args);
  const fixturePath = requireFlag(options, "fixture");
  const fixture = await readFixture(fixturePath);
  const output = resolve(options.output ?? "out/mmdplugin-mp4/capture.mp4");
  const result = await exportMp4WithMmd(fixture, {
    fixturePath,
    output,
    captureDir: options["capture-dir"],
    startFrame: optionalNumber(options, "start-frame") ?? 0,
    endFrame: optionalNumber(options, "end-frame") ?? fixture.frames.at(-1) ?? 30,
    fps: optionalNumber(options, "fps") ?? 30,
    minCaptureFiles: optionalNumber(options, "min-capture-files"),
    timeoutMs: optionalNumber(options, "timeout-ms") ?? fixture.timeoutMs ?? 120000,
    sendKeyAfterMs: optionalNumber(options, "send-key-after-ms") ?? 3000,
    sendKey: options["send-key"],
    captureEveryNFrames: optionalNumber(options, "capture-every-n-frames"),
    crf: options.crf,
    preset: options.preset,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function oracleFromVmdCommand(args) {
  const options = parseFlags(args);
  const request = {
    templatePmm: requireFlag(options, "template-pmm"),
    targetVmd: requireFlag(options, "vmd"),
    targetSlot: optionalNumber(options, "target-slot"),
    frames: options.frames ? parseFrames(options.frames) : undefined,
    output: options.output,
    outDir: options["out-dir"],
    projectOut: options["project-out"],
    fixtureOut: options["fixture-out"],
    mmdExe: options["mmd-exe"],
    timeoutMs: optionalNumber(options, "timeout-ms") ?? 60000,
    sendKeyAfterMs: optionalNumber(options, "send-key-after-ms") ?? 3000,
  };
  const result =
    options["dry-run"] === "true"
      ? await prepareOracleFromVmd(request)
      : await recordOracleFromVmd(request);
  console.log(JSON.stringify(compactOracleFromVmdResult(result), null, 2));
}

async function oracleBatchCommand(args) {
  const options = parseFlags(args);
  const request = {
    manifest: requireFlag(options, "manifest"),
    outDir: options["out-dir"],
    caseName: options.case,
    mmdExe: options["mmd-exe"],
    timeoutMs: optionalNumber(options, "timeout-ms"),
    sendKeyAfterMs: optionalNumber(options, "send-key-after-ms"),
  };
  const result =
    options["dry-run"] === "true"
      ? await prepareOracleBatch(request)
      : await recordOracleBatch(request);
  console.log(JSON.stringify(result, null, 2));
  if (!result.ok) {
    process.exitCode = 1;
  }
}

async function createBasePmmFromPmxCommand(args) {
  const options = parseFlags(args);
  const report = await writeBasePmmFromPmx({
    pmx: requireFlag(options, "pmx"),
    out: requireFlag(options, "out"),
    outputWidth: optionalNumber(options, "output-width"),
    outputHeight: optionalNumber(options, "output-height"),
    timelineWidth: optionalNumber(options, "timeline-width"),
    cameraFov: optionalNumber(options, "camera-fov"),
    missingNames: options["missing-names"],
  });
  console.log(JSON.stringify(report, null, 2));
}

async function createPmmFromPmxVmdCommand(args) {
  const options = parseFlags(args);
  const report = await writePmmFromPmxVmd({
    pmx: requireFlag(options, "pmx"),
    vmd: requireFlag(options, "vmd"),
    cameraVmd: options["camera-vmd"],
    out: requireFlag(options, "out"),
    outputWidth: optionalNumber(options, "output-width"),
    outputHeight: optionalNumber(options, "output-height"),
    timelineWidth: optionalNumber(options, "timeline-width"),
    cameraFov: optionalNumber(options, "camera-fov"),
  });
  console.log(JSON.stringify(report, null, 2));
}

async function stagePmxCommand(args) {
  const options = parseFlags(args);
  const result = await stageMmdCompatiblePmx(requireFlag(options, "input"), requireFlag(options, "output"));
  if (result.encoding === "not-pmx") {
    throw new Error(`stage-pmx requires a .pmx input, got ${result.input}.`);
  }
  writeJson({ ok: true, ...result });
}

function compactOracleFromVmdResult(result) {
  return {
    ok: result.ok,
    mode: result.mode,
    templatePmm: result.templatePmm,
    targetVmd: result.targetVmd,
    targetSlot: result.targetSlot,
    project: result.project,
    fixturePath: result.fixturePath,
    output: result.output,
    frames: result.frames,
    records: result.records,
    firstFrame: result.firstFrame,
    lastFrame: result.lastFrame,
    patch: result.patch
      ? {
          ok: result.patch.ok,
          mode: result.patch.mode,
          byteLengthDelta: result.patch.byteLengthDelta,
          rewriteCount: result.patch.rewriteCount,
          resize: result.patch.resize,
          comparison: {
            ok: result.patch.comparison?.ok,
            counts: result.patch.comparison?.counts,
          },
        }
      : undefined,
  };
}

async function inspectModelCommand(args) {
  const file = positional(args, 0);
  const inventory = await readModelInventory(file, { limit: optionalNumber(parseFlags(args.slice(1)), "limit") ?? 20 });
  console.log(JSON.stringify({ ok: true, file, ...inventory }, null, 2));
}

async function inspectPmmCommand(args) {
  const file = positional(args, 0);
  const manifest = await readPmmManifest(file);
  console.log(JSON.stringify({ ok: true, file, ...manifest }, null, 2));
}

async function inspectPmmModelSlotsCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const report = await readPmmModelSlotReport(file, {
    limit: optionalNumber(options, "limit") ?? 32,
  });
  console.log(JSON.stringify({ ok: true, file, ...report }, null, 2));
}

async function inspectPmmDocumentKeyframesCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const report = await readPmmDocumentKeyframes(file, {
    keyframeLimit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify({ ok: true, file, ...report }, null, 2));
}

async function dumpPmmParametersCommand(args) {
  const options = parseFlags(args);
  const report = await writePmmParameterDump({
    pmm: requireFlag(options, "pmm"),
    out: requireFlag(options, "out"),
    keyframeLimit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
}

async function comparePmmDocumentVmdKeyframesCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmDocumentVmdComparison({
    pmm: requireFlag(options, "pmm"),
    vmd: requireFlag(options, "vmd"),
    targetSlot: optionalNumber(options, "target-slot"),
    positionEpsilon: optionalNumber(options, "position-epsilon"),
    rotationEpsilon: optionalNumber(options, "rotation-epsilon"),
    weightEpsilon: optionalNumber(options, "weight-epsilon"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function patchPmmDocumentVmdKeyframesCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-document-vmd-keyframes write mode requires verification; omit --require-verified or set it to true.");
  }
  const report = await writePmmDocumentVmdKeyframePatch({
    template: requireFlag(options, "template"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    targetSlot: optionalNumber(options, "target-slot"),
    requireVerified: true,
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function createPmmFromTemplateCommand(args) {
  const options = parseFlags(args);
  if (options["dry-run"] === "true") {
    const report = await createPmmFromTemplateDryRun({
      template: requireFlag(options, "template"),
      vmd: requireFlag(options, "vmd"),
      modelSlot: parseInteger(requireFlag(options, "model-slot")),
      limit: optionalNumber(options, "limit"),
    });
    console.log(JSON.stringify({ ok: report.okToWrite, ...report }, null, 2));
    return;
  }
  if (options["require-verified"] !== "true") {
    throw new Error("create-pmm-from-template write mode requires --require-verified true.");
  }
  const result = await writePmmFromTemplateProfile({
    template: requireFlag(options, "template"),
    vmd: requireFlag(options, "vmd"),
    out: requireFlag(options, "out"),
    modelSlot: parseInteger(requireFlag(options, "model-slot")),
    limit: optionalNumber(options, "limit"),
    allowNonIdentityRotation: options["allow-non-identity-rotation"] === "true",
    requireVerified: true,
    oracle: options.oracle,
  });
  console.log(JSON.stringify(result, null, 2));
}

async function staticRenderCommand(args) {
  const options = parseFlags(args);
  const imageOptions = Object.fromEntries(
    [
      ["width", optionalNumber(options, "output-width")],
      ["height", optionalNumber(options, "output-height")],
      ["format", options["image-format"]],
      ["cropContent", options["crop-content"] === undefined ? undefined : options["crop-content"] === "true"],
    ].filter(([, value]) => value !== undefined),
  );
  const request = {
    manifest: requireFlag(options, "manifest"),
    outDir: options["out-dir"],
    caseName: options.case,
    mmdExe: options["mmd-exe"],
    timeoutMs: optionalNumber(options, "timeout-ms"),
    loadWaitMs: optionalNumber(options, "load-wait-ms") ?? 5000,
    image: imageOptions,
  };
  const result =
    options["dry-run"] === "true"
      ? await prepareStaticRenderBatch(request)
      : await exportStaticRenderBatch(request);
  if (options["dry-run"] !== "true") {
    for (const testCase of result.results) {
      const files = (testCase.images ?? []).map((image) => image.output);
      const cropContent = options["crop-content"] === undefined ? testCase.image?.cropContent === true : options["crop-content"] === "true";
      testCase.pngFiles = await writeCapturePngs(files, { cropContent, removeSourceBmp: true });
      const pngBySource = new Map(testCase.pngFiles.map((png) => [png.sourceFile ?? png.file, png]));
      testCase.images = (testCase.images ?? []).map((image) => {
        const png = pngBySource.get(image.output);
        return png ? { ...image, output: png.file, sourceOutput: image.output } : image;
      });
    }
  }
  console.log(JSON.stringify(result, null, 2));
}

async function analyzePmmCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const analysis = await readPmmAnalysis(file, {
    texts: parseList(options.text),
    int32s: parseNumberList(options.int32),
    float32s: parseNumberList(options.float32),
    limit: optionalNumber(options, "limit") ?? 32,
  });
  console.log(JSON.stringify({ ok: true, file, ...analysis }, null, 2));
}

async function diffPmmCommand(args) {
  const options = parseFlags(args);
  const left = requireFlag(options, "left");
  const right = requireFlag(options, "right");
  const diff = await readPmmDiff(left, right, {
    context: optionalNumber(options, "context") ?? 16,
    limit: optionalNumber(options, "limit") ?? 32,
  });
  console.log(JSON.stringify({ ok: true, left, right, ...diff }, null, 2));
}

async function investigatePmmCommand(args) {
  const options = parseFlags(args);
  const left = requireFlag(options, "left");
  const right = requireFlag(options, "right");
  const report = await readPmmInvestigation(left, right, {
    texts: parseList(options.text),
    int32s: parseNumberList(options.int32),
    float32s: parseNumberList(options.float32),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify({ ok: true, left, right, ...report }, null, 2));
}

async function clusterPmmVmdDiffCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdDiffClusters({
    base: requireFlag(options, "base"),
    variant: requireFlag(options, "variant"),
    vmd: requireFlag(options, "vmd"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    candidateLimit: optionalNumber(options, "candidate-limit"),
    maxFramePositionDelta: optionalNumber(options, "max-frame-position-delta"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify({ ok: true, ...report }, null, 2));
}

async function patchPmmVmdDiffClusterCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-vmd-diff-cluster write mode requires verification; omit --require-verified or set it to true.");
  }
  const result = await writePmmVmdDiffClusterPatch({
    base: requireFlag(options, "base"),
    donorBase: requireFlag(options, "donor-base"),
    donorVariant: requireFlag(options, "donor-variant"),
    donorVmd: requireFlag(options, "donor-vmd"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
    requireVerified: true,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function inspectPmmPatchProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmPatchProfileRegistryInspection({
    registry: requireFlag(options, "registry"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function inventoryPmmPatchProfileRegistriesCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmPatchProfileRegistryInventory({
    registries: parseList(requireFlag(options, "registries")),
    limit: optionalNumber(options, "limit"),
    entryLimit: optionalNumber(options, "entry-limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function writeUsablePmmPatchProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await writeUsablePmmPatchProfileRegistry({
    registries: parseList(requireFlag(options, "registries")),
    out: requireFlag(options, "out"),
    limit: optionalNumber(options, "limit"),
    omittedLimit: optionalNumber(options, "omitted-limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function checkPmmVmdPatchCompatibilityCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdPatchCompatibility({
    registries: parseList(requireFlag(options, "registries")),
    base: requireFlag(options, "base"),
    targetPmx: options["target-pmx"],
    targetVmd: requireFlag(options, "target-vmd"),
    out: options.out,
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function planPmmVmdPatchFromAnyProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdAnyPatchRegistryPlan({
    registries: parseList(requireFlag(options, "registries")),
    base: requireFlag(options, "base"),
    targetPmx: options["target-pmx"],
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function patchPmmVmdFromAnyProfileRegistryCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-vmd-from-any-profile-registry write mode requires verification; omit --require-verified or set it to true.");
  }
  const report = await writePmmVmdPatchFromAnyProfileRegistry({
    registries: parseList(requireFlag(options, "registries")),
    base: requireFlag(options, "base"),
    targetPmx: options["target-pmx"],
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
    requireVerified: true,
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function planPmmVmdPatchFromProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdPatchRegistryPlan({
    registry: requireFlag(options, "registry"),
    base: requireFlag(options, "base"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function patchPmmVmdFromProfileRegistryCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-vmd-from-profile-registry write mode requires verification; omit --require-verified or set it to true.");
  }
  const report = await writePmmVmdPatchFromProfileRegistry({
    registry: requireFlag(options, "registry"),
    base: requireFlag(options, "base"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    donorSlot: optionalNumber(options, "donor-slot"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
    requireVerified: true,
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function planPmmVmdKeyCountDeltaFromProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdKeyCountDeltaPatchRegistryPlan({
    registry: requireFlag(options, "registry"),
    base: requireFlag(options, "base"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function patchPmmVmdKeyCountDeltaFromProfileRegistryCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-vmd-key-count-delta-from-profile-registry write mode requires verification; omit --require-verified or set it to true.");
  }
  const report = await writePmmVmdKeyCountDeltaPatchFromProfileRegistry({
    registry: requireFlag(options, "registry"),
    base: requireFlag(options, "base"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
    requireVerified: true,
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function patchPmmVmdKeyCountDeltaCommand(args) {
  const options = parseFlags(args);
  if (options["require-verified"] === "false") {
    throw new Error("patch-pmm-vmd-key-count-delta write mode requires verification; omit --require-verified or set it to true.");
  }
  const result = await writePmmVmdKeyCountDeltaPatch({
    base: requireFlag(options, "base"),
    smallVariant: requireFlag(options, "small-variant"),
    largeVariant: requireFlag(options, "large-variant"),
    smallVmd: requireFlag(options, "small-vmd"),
    largeVmd: requireFlag(options, "large-vmd"),
    targetVmd: requireFlag(options, "target-vmd"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context") ?? 16,
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    targetSlot: optionalNumber(options, "target-slot"),
    limit: optionalNumber(options, "limit"),
    requireVerified: true,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function analyzePmmKeyCountDeltaCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyCountDelta({
    base: requireFlag(options, "base"),
    smallVariant: requireFlag(options, "small-variant"),
    largeVariant: requireFlag(options, "large-variant"),
    smallVmd: requireFlag(options, "small-vmd"),
    largeVmd: requireFlag(options, "large-vmd"),
    context: optionalNumber(options, "context"),
    diffLimit: optionalNumber(options, "diff-limit"),
    clusterDiffLimit: optionalNumber(options, "cluster-diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    candidateLimit: optionalNumber(options, "candidate-limit"),
    sequenceLimit: optionalNumber(options, "sequence-limit"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    frameOffsetInRecord: optionalNumber(options, "frame-offset"),
    scalarLimit: optionalNumber(options, "scalar-limit"),
  });
  console.log(JSON.stringify({ ok: true, ...report }, null, 2));
}

async function extractPmmVmdKeyframesCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdKeyframes({
    base: requireFlag(options, "base"),
    variant: requireFlag(options, "variant"),
    vmd: requireFlag(options, "vmd"),
    context: optionalNumber(options, "context"),
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    candidateLimit: optionalNumber(options, "candidate-limit"),
    sequenceLimit: optionalNumber(options, "sequence-limit"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    frameOffsetInRecord: optionalNumber(options, "frame-offset"),
    positionOffsetInRecord: optionalNumber(options, "position-offset"),
    rotationOffsetInRecord: optionalNumber(options, "rotation-offset"),
  });
  if (options["profile-out"]) {
    const profileReport = {
      ok: true,
      kind: "pmm-keyframe-profile",
      version: 1,
      source: {
        base: requireFlag(options, "base"),
        variant: requireFlag(options, "variant"),
        vmd: requireFlag(options, "vmd"),
      },
      profile: report.profile,
      coverage: report.coverage,
    };
    await writeFile(options["profile-out"], `${JSON.stringify(profileReport, null, 2)}\n`, "utf8");
  }
  console.log(JSON.stringify({ ok: true, ...report }, null, 2));
}

async function comparePmmVmdKeyframesCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmVmdKeyframeComparison({
    base: requireFlag(options, "base"),
    variant: requireFlag(options, "variant"),
    vmd: requireFlag(options, "vmd"),
    context: optionalNumber(options, "context"),
    diffLimit: optionalNumber(options, "diff-limit"),
    matchLimit: optionalNumber(options, "match-limit"),
    candidateLimit: optionalNumber(options, "candidate-limit"),
    sequenceLimit: optionalNumber(options, "sequence-limit"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    frameOffsetInRecord: optionalNumber(options, "frame-offset"),
    positionOffsetInRecord: optionalNumber(options, "position-offset"),
    rotationOffsetInRecord: optionalNumber(options, "rotation-offset"),
    frameEpsilon: optionalNumber(options, "frame-epsilon"),
    positionEpsilon: optionalNumber(options, "position-epsilon"),
    rotationEpsilon: optionalNumber(options, "rotation-epsilon"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function extractPmmKeyframesWithProfileCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyframesWithProfile({
    pmm: requireFlag(options, "pmm"),
    profile: requireFlag(options, "profile"),
  });
  console.log(JSON.stringify(report, null, 2));
}

async function extractPmmKeyframesWithProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyframesWithProfileRegistry({
    pmm: requireFlag(options, "pmm"),
    registry: requireFlag(options, "registry"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function checkPmmKeyframeProfileCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyframeProfileCheck({
    pmm: requireFlag(options, "pmm"),
    profile: requireFlag(options, "profile"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function checkPmmKeyframeProfileRegistryCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyframeProfileRegistryCheck({
    pmm: requireFlag(options, "pmm"),
    registry: requireFlag(options, "registry"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function comparePmmKeyframesWithProfileCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmKeyframesWithProfileComparison({
    pmm: requireFlag(options, "pmm"),
    profile: requireFlag(options, "profile"),
    vmd: requireFlag(options, "vmd"),
    frameEpsilon: optionalNumber(options, "frame-epsilon"),
    positionEpsilon: optionalNumber(options, "position-epsilon"),
    rotationEpsilon: optionalNumber(options, "rotation-epsilon"),
    limit: optionalNumber(options, "limit"),
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

async function writeTestVmdCommand(args) {
  const options = parseFlags(args);
  const boneFrames = createSyntheticBoneFrames(options);
  const result = await writeSyntheticVmd(requireFlag(options, "out"), {
    modelName: options["model-name"],
    boneName: options["bone-name"],
    boneFrames,
    morphName: options["morph-name"],
    frame: optionalNumber(options, "frame") ?? 30,
    position: parseVector3(options.position ?? "1,2,3"),
    rotation: parseVector4(options.rotation ?? "0,0,0,1"),
    weight: optionalNumber(options, "weight") ?? 0.5,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

function createSyntheticBoneFrames(options) {
  if (options["bone-frame-keys"]) {
    return parseNamedBoneFrameKeys(options["bone-frame-keys"]);
  }
  if (options["bone-transform-keys"]) {
    return parseBoneTransformKeys(options["bone-transform-keys"]).map((key) => ({ name: options["bone-name"], ...key }));
  }
  if (options["bone-transform-key-count"]) {
    return createGeneratedBoneTransformKeys({
      boneName: requireFlag(options, "bone-name"),
      count: parseInteger(options["bone-transform-key-count"]),
      startFrame: optionalNumber(options, "bone-key-start-frame") ?? 30,
      frameStep: optionalNumber(options, "bone-key-frame-step") ?? 30,
      startPosition: parseVector3(options["bone-key-start-position"] ?? "1,2,3"),
      positionStep: parseVector3(options["bone-key-position-step"] ?? "3,3,3"),
      rotationSequence: parseQuaternionSequence(options["bone-transform-rotation-sequence"] ?? "0.382683,0,0,0.92388;0,0.382683,0,0.92388;0,0,0.382683,0.92388"),
    });
  }
  if (options["bone-rotation-keys"]) {
    const position = parseVector3(options.position ?? "0,0,0");
    return parseBoneRotationKeys(options["bone-rotation-keys"], position).map((key) => ({ name: options["bone-name"], ...key }));
  }
  if (options["bone-keys"]) {
    return parseBonePositionKeys(options["bone-keys"]).map((key) => ({ name: options["bone-name"], ...key }));
  }
  if (options["bone-key-count"]) {
    return createGeneratedBonePositionKeys({
      boneName: requireFlag(options, "bone-name"),
      count: parseInteger(options["bone-key-count"]),
      startFrame: optionalNumber(options, "bone-key-start-frame") ?? 30,
      frameStep: optionalNumber(options, "bone-key-frame-step") ?? 30,
      startPosition: parseVector3(options["bone-key-start-position"] ?? "1,2,3"),
      positionStep: parseVector3(options["bone-key-position-step"] ?? "3,3,3"),
    });
  }
  return undefined;
}

async function inspectVmdCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const inventory = await readVmdInventory(file, { limit: optionalNumber(options, "limit") ?? 16 });
  console.log(JSON.stringify({ ok: true, file, ...inventory }, null, 2));
}

async function compareVmdPmmMotionCommand(args) {
  const options = parseFlags(args);
  const result = await compareVmdToPmmMotion({
    vmd: requireFlag(options, "vmd"),
    pmm: requireFlag(options, "pmm"),
    markerHex: options.marker,
    markerOffsetInRecord: optionalNumber(options, "marker-offset"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    limit: optionalNumber(options, "limit") ?? 8,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function mapVmdPmmBoneFramesCommand(args) {
  const options = parseFlags(args);
  const result = await mapVmdBoneFramesToPmm({
    vmd: requireFlag(options, "vmd"),
    pmm: requireFlag(options, "pmm"),
    boneName: options["bone-name"],
    allBones: options["all-bones"] === "true",
    markerHex: options.marker,
    markerOffsetInRecord: optionalNumber(options, "marker-offset"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    recordLimit: optionalNumber(options, "record-limit"),
    limit: optionalNumber(options, "limit"),
    matchLimit: optionalNumber(options, "match-limit"),
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function writePmmInvestigationKitCommand(args) {
  const options = parseFlags(args);
  const result = await writePmmInvestigationKit(requireFlag(options, "out-dir"), {
    modelName: options["model-name"],
    boneName: options["bone-name"],
    morphName: options["morph-name"],
    frame: optionalNumber(options, "frame") ?? 30,
    position: parseVector3(options.position ?? "1,2,3"),
    weight: optionalNumber(options, "weight") ?? 0.5,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function analyzePmmFixtureMotionCommand(args) {
  const options = parseFlags(args);
  const report = await readPmmFixtureMotionReport({
    base: requireFlag(options, "base"),
    variant: requireFlag(options, "variant"),
    vmd: options.vmd,
    markerHex: options.marker,
    markerOffsetInRecord: optionalNumber(options, "marker-offset"),
    recordByteLength: optionalNumber(options, "record-bytes") ?? 62,
    recordLimit: optionalNumber(options, "record-limit"),
    hexLimit: optionalNumber(options, "hex-limit"),
    wordLimit: optionalNumber(options, "word-limit"),
    values: parseNumberList(options.values),
    frames: parseNumberList(options.frames),
    context: optionalNumber(options, "context"),
    limit: optionalNumber(options, "limit") ?? 32,
  });
  console.log(JSON.stringify({ ok: true, ...report }, null, 2));
}

async function patchPmmFixtureMotionCommand(args) {
  const options = parseFlags(args);
  const result = await writePmmFixtureMotionPatch({
    base: requireFlag(options, "base"),
    donorBase: requireFlag(options, "donor-base"),
    donorVariant: requireFlag(options, "donor-variant"),
    out: requireFlag(options, "out"),
    context: optionalNumber(options, "context"),
    limit: optionalNumber(options, "limit") ?? 32,
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function rewritePmmScalarsCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const result = await writePmmScalarRewrite({
    file,
    out: requireFlag(options, "out"),
    u32At: parseOffsetWrites(options["u32-at"]),
    float32At: parseOffsetWrites(options["float32-at"]),
    hexAt: parseHexWrites(options["hex-at"]),
    insertHexAt: parseHexWrites(options["insert-hex-at"]),
    frames: parseReplacements(options.frames),
    float32s: parseReplacements(options.float32),
  });
  console.log(JSON.stringify({ ok: true, ...result }, null, 2));
}

async function writePmmUnittestBoneKeysCommand(args) {
  const template = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const result = await writePmmUnittestBoneKeys({
    template,
    out: requireFlag(options, "out"),
    keys: parseBonePositionKeys(requireFlag(options, "keys")),
    oracle: options.oracle,
  });
  writeJson({ ok: true, ...result }, options);
}

async function writePmmUnittestVmdBoneKeysCommand(args) {
  const template = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const result = await writePmmUnittestVmdBoneKeys({
    template,
    vmd: requireFlag(options, "vmd"),
    out: requireFlag(options, "out"),
    boneName: options["bone-name"],
    ignoreUnsupported: options["ignore-unsupported"] === "true",
    allowNonIdentityRotation: options["allow-non-identity-rotation"] === "true",
    requireGeneratedMapping: options["require-verified"] === "true",
    oracle: options.oracle,
    limit: optionalNumber(options, "limit"),
  });
  writeJson({ ok: true, ...result }, options);
}

async function scanPmmMotionCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const result = await readPmmMotionScan(file, {
    int32s: parseNumberList(options.int32),
    float32s: parseNumberList(options.float32),
    markerHex: parseList(options.marker),
    radius: optionalNumber(options, "radius") ?? 96,
    limit: optionalNumber(options, "limit") ?? 64,
  });
  console.log(JSON.stringify({ ok: true, file, ...result }, null, 2));
}

async function extractPmmMotionRecordsCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const result = await readPmmMotionRecords(file, {
    markerHex: options.marker,
    markerOffsetInRecord: optionalNumber(options, "marker-offset"),
    recordByteLength: optionalNumber(options, "record-bytes"),
    limit: optionalNumber(options, "limit") ?? 64,
  });
  console.log(JSON.stringify({ ok: true, file, ...result }, null, 2));
}

async function dumpPmmMotionRecordsCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const out = requireFlag(options, "out");
  const result = await readPmmMotionRecordSlice(file, {
    recordStart: parseInteger(requireFlag(options, "record-start")),
    count: parseInteger(requireFlag(options, "count")),
    recordByteLength: optionalNumber(options, "record-bytes") ?? 58,
  });
  await writeFile(out, result.bytes);
  console.log(JSON.stringify({ ok: true, file, out, ...withoutBytes(result) }, null, 2));
}

async function patchPmmMotionRecordsCommand(args) {
  const file = positional(args, 0);
  const options = parseFlags(args.slice(1));
  const out = requireFlag(options, "out");
  const replacement = await readFile(requireFlag(options, "records"));
  const original = await readFile(file);
  const result = patchPmmMotionRecordSlice(original, replacement, {
    recordStart: parseInteger(requireFlag(options, "record-start")),
    count: parseInteger(requireFlag(options, "count")),
    recordByteLength: optionalNumber(options, "record-bytes") ?? 58,
  });
  await writeFile(out, result.bytes);
  console.log(JSON.stringify({ ok: true, file, out, records: options.records, ...withoutBytes(result) }, null, 2));
}

async function verifyCoverageCommand(args) {
  const options = parseFlags(args);
  const report = await verifyFixtureCoverage({
    fixture: requireFlag(options, "fixture"),
    actual: options.actual,
    frameEpsilon: optionalNumber(options, "frame-epsilon"),
    requireBones: options["require-bones"] !== "false",
    requireMorphs: options["require-morphs"] !== "false",
  });
  console.log(JSON.stringify(report, null, 2));
  if (!report.ok) {
    process.exitCode = 1;
  }
}

function parseFlags(args) {
  const options = {};
  for (let i = 0; i < args.length; i += 1) {
    const token = args[i];
    if (!token.startsWith("--")) {
      throw new Error(`Unexpected positional argument: ${token}`);
    }
    const key = token.slice(2);
    const value = args[i + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`Missing value for --${key}`);
    }
    options[key] = value;
    i += 1;
  }
  return options;
}

function requireFlag(options, key) {
  if (!options[key]) {
    throw new Error(`Missing --${key}`);
  }
  return options[key];
}

function optionalNumber(options, key) {
  if (options[key] === undefined) {
    return undefined;
  }
  const value = Number(options[key]);
  if (!Number.isFinite(value)) {
    throw new Error(`--${key} must be finite`);
  }
  return value;
}

function parseFrames(value) {
  const frames = value
    .split(",")
    .map((part) => Number(part.trim()))
    .filter((frame) => !Number.isNaN(frame));
  if (frames.length === 0 || frames.some((frame) => !Number.isFinite(frame))) {
    throw new Error("--frames must be a comma-separated list of finite numbers.");
  }
  return frames;
}

function parseList(value) {
  if (value === undefined) {
    return undefined;
  }
  return value
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function parseNumberList(value) {
  const parts = parseList(value);
  if (parts === undefined) {
    return undefined;
  }
  const numbers = parts.map((part) => Number(part));
  if (numbers.some((number) => !Number.isFinite(number))) {
    throw new Error("Numeric search values must be comma-separated finite numbers.");
  }
  return numbers;
}

function parseReplacements(value) {
  if (value === undefined) {
    return [];
  }
  return parseList(value).map((part) => {
    const [from, to] = part.split(":").map((number) => Number(number));
    if (!Number.isFinite(from) || !Number.isFinite(to)) {
      throw new Error(`Replacement must be from:to, got ${part}`);
    }
    return { from, to };
  });
}

function parseOffsetWrites(value) {
  if (value === undefined) {
    return [];
  }
  return parseList(value).map((part) => {
    const [offsetText, valueText] = part.split(":");
    const offset = parseInteger(offsetText);
    const value = Number(valueText);
    if (!Number.isInteger(offset) || !Number.isFinite(value)) {
      throw new Error(`Offset write must be offset:value, got ${part}`);
    }
    return { offset, value };
  });
}

function parseHexWrites(value) {
  if (value === undefined) {
    return [];
  }
  return parseList(value).map((part) => {
    const [offsetText, hexText] = part.split(":");
    const offset = parseInteger(offsetText);
    const hex = hexText?.replace(/[^0-9a-f]/gi, "").toLowerCase();
    if (!Number.isInteger(offset) || !hex || hex.length % 2 !== 0) {
      throw new Error(`Hex write must be offset:hex, got ${part}`);
    }
    return { offset, hex };
  });
}

function parseInteger(value) {
  const number = Number.parseInt(value, value.startsWith("0x") ? 16 : 10);
  if (!Number.isInteger(number)) {
    throw new Error(`Expected integer, got ${value}`);
  }
  return number;
}

function parseVector3(value) {
  const numbers = parseNumberList(value);
  if (!numbers || numbers.length !== 3) {
    throw new Error("Vector values must contain exactly three comma-separated numbers.");
  }
  return numbers;
}

function parseVector4(value) {
  const numbers = parseNumberList(value);
  if (!numbers || numbers.length !== 4) {
    throw new Error("Quaternion values must contain exactly four comma-separated numbers.");
  }
  return numbers;
}

function parseBonePositionKeys(value) {
  const keys = value
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [frameText, vectorText] = part.split(":");
      const frame = parseInteger(frameText);
      const position = parseVector3(vectorText ?? "");
      return { frame, position };
    });
  if (keys.length === 0) {
    throw new Error("--keys must contain at least one frame:x,y,z entry.");
  }
  return keys;
}

function parseBoneRotationKeys(value, position) {
  const keys = value
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [frameText, vectorText] = part.split(":");
      const frame = parseInteger(frameText);
      const rotation = parseVector4(vectorText ?? "");
      return { frame, position: [...position], rotation };
    });
  if (keys.length === 0) {
    throw new Error("--bone-rotation-keys must contain at least one frame:x,y,z,w entry.");
  }
  return keys;
}

function parseBoneTransformKeys(value) {
  const keys = value
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [frameText, positionText, rotationText] = part.split(":");
      const frame = parseInteger(frameText);
      const position = parseVector3(positionText ?? "");
      const rotation = parseVector4(rotationText ?? "");
      return { frame, position, rotation };
    });
  if (keys.length === 0) {
    throw new Error("--bone-transform-keys must contain at least one frame:x,y,z:qx,qy,qz,qw entry.");
  }
  return keys;
}

function parseNamedBoneFrameKeys(value) {
  const keys = value
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [name, frameText, positionText, rotationText] = part.split(":");
      if (!name) {
        throw new Error(`--bone-frame-keys entry needs name:frame:x,y,z, got ${part}`);
      }
      return {
        name,
        frame: parseInteger(frameText),
        position: parseVector3(positionText ?? ""),
        rotation: rotationText ? parseVector4(rotationText) : [0, 0, 0, 1],
      };
    });
  if (keys.length === 0) {
    throw new Error("--bone-frame-keys must contain at least one name:frame:x,y,z entry.");
  }
  return keys;
}

function createGeneratedBonePositionKeys(options) {
  validateGeneratedBoneKeyOptions(options, "--bone-key-count");
  return Array.from({ length: options.count }, (_, index) => ({
    name: options.boneName,
    frame: options.startFrame + options.frameStep * index,
    position: options.startPosition.map((value, component) => value + options.positionStep[component] * index),
  }));
}

function createGeneratedBoneTransformKeys(options) {
  validateGeneratedBoneKeyOptions(options, "--bone-transform-key-count");
  return Array.from({ length: options.count }, (_, index) => ({
    name: options.boneName,
    frame: options.startFrame + options.frameStep * index,
    position: options.startPosition.map((value, component) => value + options.positionStep[component] * index),
    rotation: [...options.rotationSequence[index % options.rotationSequence.length]],
  }));
}

function validateGeneratedBoneKeyOptions(options, countOptionName) {
  if (!Number.isInteger(options.count) || options.count < 1 || options.count > 0xffff) {
    throw new Error(`${countOptionName} must be an integer in 1..65535, got ${options.count}.`);
  }
  if (!Number.isInteger(options.startFrame) || !Number.isInteger(options.frameStep) || options.startFrame < 0 || options.frameStep < 1) {
    throw new Error("--bone-key-start-frame and --bone-key-frame-step must be non-negative/incrementing integers.");
  }
  const lastFrame = options.startFrame + options.frameStep * (options.count - 1);
  if (lastFrame > 0xffffffff) {
    throw new Error(`Generated VMD frame exceeds uint32 range: ${lastFrame}.`);
  }
}

function parseQuaternionSequence(value) {
  const rotations = value
    .split(";")
    .map((part) => part.trim())
    .filter(Boolean)
    .map(parseVector4);
  if (rotations.length === 0) {
    throw new Error("--bone-transform-rotation-sequence must contain at least one qx,qy,qz,qw entry.");
  }
  return rotations;
}

function writeJson(result, options = {}) {
  console.log(JSON.stringify(options.compact === "true" ? compactPmmWriteResult(result) : result, null, 2));
}

async function writeCapturePngs(files, options = {}) {
  const pngs = [];
  for (const file of files) {
    if (/\.png$/iu.test(file) && !options.cropContent) {
      const size = parsePngSize(await readFile(file));
      pngs.push({ file, width: size.width, height: size.height, direct: true });
      continue;
    }
    if (/\.png$/iu.test(file) && options.cropContent) {
      throw new Error("crop-content requires BMP output; disable crop-content or use image.format=\"bmp\".");
    }
    const bmp = parseBmp32(await readFile(file));
    const image = options.cropContent ? cropImageContent(bmp) : bmp;
    const pngPath = file.replace(/\.bmp$/iu, options.cropContent ? ".content.png" : ".png");
    await writeFile(pngPath, encodePngRgba(image));
    const result = { file: pngPath, sourceFile: file, width: image.width, height: image.height, crop: image.crop };
    if (options.removeSourceBmp && /\.bmp$/iu.test(file)) {
      await rm(file, { force: true });
      result.sourceRemoved = true;
    }
    pngs.push(result);
  }
  return pngs;
}

function parsePngSize(bytes) {
  if (
    bytes.length < 24 ||
    bytes[0] !== 0x89 ||
    bytes.toString("ascii", 1, 4) !== "PNG" ||
    bytes.readUInt32BE(8) !== 13 ||
    bytes.toString("ascii", 12, 16) !== "IHDR"
  ) {
    throw new Error("Expected PNG image file.");
  }
  return {
    width: bytes.readUInt32BE(16),
    height: bytes.readUInt32BE(20),
  };
}

function parseBmp32(bytes) {
  if (bytes.toString("ascii", 0, 2) !== "BM") {
    throw new Error("Expected BMP capture file.");
  }
  const pixelOffset = bytes.readUInt32LE(10);
  const dibSize = bytes.readUInt32LE(14);
  const width = bytes.readInt32LE(18);
  const signedHeight = bytes.readInt32LE(22);
  const bitsPerPixel = bytes.readUInt16LE(28);
  const compression = bytes.readUInt32LE(30);
  if (dibSize !== 40 || width <= 0 || signedHeight === 0 || bitsPerPixel !== 32 || compression !== 0) {
    throw new Error("Unsupported BMP capture format.");
  }
  const height = Math.abs(signedHeight);
  const topDown = signedHeight < 0;
  const rgba = Buffer.alloc(width * height * 4);
  const rowBytes = width * 4;
  for (let y = 0; y < height; y += 1) {
    const sourceY = topDown ? y : height - 1 - y;
    const sourceOffset = pixelOffset + sourceY * rowBytes;
    const targetOffset = y * rowBytes;
    for (let x = 0; x < width; x += 1) {
      const source = sourceOffset + x * 4;
      const target = targetOffset + x * 4;
      rgba[target] = bytes[source + 2];
      rgba[target + 1] = bytes[source + 1];
      rgba[target + 2] = bytes[source];
      rgba[target + 3] = 255;
    }
  }
  return { width, height, rgba };
}

function cropImageContent(image, threshold = 8) {
  let minX = image.width;
  let minY = image.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      if (image.rgba[offset] > threshold || image.rgba[offset + 1] > threshold || image.rgba[offset + 2] > threshold) {
        minX = Math.min(minX, x);
        minY = Math.min(minY, y);
        maxX = Math.max(maxX, x);
        maxY = Math.max(maxY, y);
      }
    }
  }
  if (maxX < minX || maxY < minY) {
    return { ...image, crop: { x: 0, y: 0, width: image.width, height: image.height, unchanged: true } };
  }
  const width = maxX - minX + 1;
  const height = maxY - minY + 1;
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    const sourceOffset = ((minY + y) * image.width + minX) * 4;
    const targetOffset = y * width * 4;
    image.rgba.copy(rgba, targetOffset, sourceOffset, sourceOffset + width * 4);
  }
  return { width, height, rgba, crop: { x: minX, y: minY, width, height } };
}

function encodePngRgba(image) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(image.width, 0);
  ihdr.writeUInt32BE(image.height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;
  const stride = image.width * 4;
  const raw = Buffer.alloc((stride + 1) * image.height);
  for (let y = 0; y < image.height; y += 1) {
    const row = y * (stride + 1);
    raw[row] = 0;
    image.rgba.copy(raw, row + 1, y * stride, y * stride + stride);
  }
  return Buffer.concat([signature, pngChunk("IHDR", ihdr), pngChunk("IDAT", deflateSync(raw)), pngChunk("IEND", Buffer.alloc(0))]);
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const chunk = Buffer.alloc(8 + data.length + 4);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
}

var crc32Table = null;

function crc32(bytes) {
  crc32Table ??= Array.from({ length: 256 }, (_, index) => {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
      value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    return value >>> 0;
  });
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = crc32Table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function compactPmmWriteResult(result) {
  return {
    ok: result.ok,
    mode: result.mode,
    keyCount: result.keyCount,
    maxFrame: result.maxFrame,
    boneName: result.boneName,
    byteLength: result.byteLength,
    byteLengthDelta: result.byteLengthDelta,
    sha256: result.sha256,
    warning: result.warning,
    replacementCount: result.replacementCount,
    insertionCount: result.insertionCount,
    templateFile: result.templateFile,
    vmdFile: result.vmdFile,
    outFile: result.outFile,
    oracleFile: result.oracleFile,
    oracleComparison: result.oracleComparison,
    sourceCounts: result.sourceCounts,
    keys: summarizeArray(result.keys),
    generatedMapping: result.generatedMapping
      ? {
          markerHex: result.generatedMapping.markerHex,
          recordByteLength: result.generatedMapping.recordByteLength,
          recordTotal: result.generatedMapping.recordTotal,
          layoutRecordByteLength: result.generatedMapping.layoutRecordByteLength,
          layoutRecordTotal: result.generatedMapping.layoutRecordTotal,
          structurallyVerified: result.generatedMapping.structurallyVerified,
          coverage: {
            ...result.generatedMapping.coverage,
            exactFrameRecordOffsets: summarizeArray(result.generatedMapping.coverage?.exactFrameRecordOffsets),
          },
        }
      : undefined,
  };
}

function summarizeArray(values) {
  if (!Array.isArray(values)) {
    return values;
  }
  return {
    count: values.length,
    first: values.slice(0, 3),
    last: values.length > 3 ? values.slice(-3) : [],
  };
}

function withoutBytes(result) {
  const { bytes, ...metadata } = result;
  return metadata;
}

function positional(args, index) {
  const value = args[index];
  if (!value || value.startsWith("--")) {
    throw new Error("Missing file path");
  }
  return value;
}

function usage() {
  console.error(`Usage:
  node src/cli.mjs validate <oracle.jsonl>
  node src/cli.mjs compare --expected <expected.jsonl> --actual <actual.jsonl> [--out <report.json>]
  node src/cli.mjs fake-record --fixture <fixture.json>
  node src/cli.mjs record --fixture <fixture.json>
  node src/cli.mjs record-direct --model <model.pmx> [--template <template.pmm>] [--motion <motion.vmd>] [--frames 0,30,60]
  node src/cli.mjs capture --fixture <fixture.json> --capture-dir <dir> [--frames 0,30,60] [--crop-content true] [--convert-existing true]
  node src/cli.mjs export-image --fixture <fixture.json> --output <frame.bmp> [--frame 30] [--crop-content true]
  node src/cli.mjs export-avi --fixture <fixture.json> --output <clip.avi> [--start-frame 0] [--end-frame 30] [--fps 30]
  node src/cli.mjs export-mp4 --fixture <fixture.json> --output <clip.mp4> [--capture-dir <dir>] [--min-capture-files 30] [--fps 30]
  node src/cli.mjs oracle-from-vmd --template-pmm <template.pmm> --vmd <motion.vmd> [--target-slot <index>] [--output <oracle.jsonl>] [--dry-run true]
  node src/cli.mjs oracle-batch --manifest <oracle-batch.json> [--case <name[,name]>] [--out-dir <dir>] [--dry-run true]
  node src/cli.mjs static-render --manifest <static-render.json> [--case <name[,name]>] [--out-dir <dir>] [--dry-run true] [--image-format png|bmp] [--output-width 1024] [--output-height 768] [--crop-content true]
  node src/cli.mjs create-base-pmm-from-pmx --pmx <model.pmx> --out <base.pmm>
  node src/cli.mjs create-pmm-from-pmx-vmd --pmx <model.pmx> --vmd <motion.vmd> [--camera-vmd <camera.vmd>] --out <scene.pmm> [--missing-names skip|strict]
  node src/cli.mjs stage-pmx --input <model.pmx> --output <model.mmd-utf16.pmx>
  node src/cli.mjs inspect-model <model.pmx|model.pmd> [--limit <count>]
  node src/cli.mjs inspect-pmx <model.pmx|model.pmd> [--limit <count>]
  node src/cli.mjs inspect-pmm <scene.pmm>
  node src/cli.mjs inspect-pmm-model-slots <scene.pmm> [--limit <count>]
  node src/cli.mjs inspect-pmm-document-keyframes <scene.pmm> [--limit <count>]
  node src/cli.mjs dump-pmm-parameters --pmm <scene.pmm> --out <parameters.jsonl> [--limit <count>]
  node src/cli.mjs compare-pmm-document-vmd-keyframes --pmm <scene.pmm> --vmd <motion.vmd> [--target-slot <index>]
  node src/cli.mjs patch-pmm-document-vmd-keyframes --template <template.pmm> --target-vmd <motion.vmd> --out <patched.pmm> [--target-slot <index>]
  node src/cli.mjs create-pmm-from-template --template <base.pmm> --vmd <motion.vmd> --model-slot <index> --dry-run true
  node src/cli.mjs create-pmm-from-template --template <profile.pmm> --vmd <motion.vmd> --model-slot <index> --out <generated.pmm> --require-verified true
  node src/cli.mjs analyze-pmm <scene.pmm> [--text <needle>] [--int32 30] [--float32 1,2,3,0.5]
  node src/cli.mjs diff-pmm --left <base.pmm> --right <with-motion.pmm>
  node src/cli.mjs investigate-pmm --left <base.pmm> --right <with-motion.pmm> [--int32 30] [--float32 1,2,3,0.5]
  node src/cli.mjs cluster-pmm-vmd-diff --base <base.pmm> --variant <with-motion.pmm> --vmd <motion.vmd>
  node src/cli.mjs analyze-pmm-key-count-delta --base <base.pmm> --small-variant <small-motion.pmm> --small-vmd <small.vmd> --large-variant <large-motion.pmm> --large-vmd <large.vmd>
  node src/cli.mjs extract-pmm-vmd-keyframes --base <base.pmm> --variant <with-motion.pmm> --vmd <motion.vmd> [--profile-out <profile.json>]
  node src/cli.mjs compare-pmm-vmd-keyframes --base <base.pmm> --variant <with-motion.pmm> --vmd <motion.vmd>
  node src/cli.mjs extract-pmm-keyframes-with-profile --pmm <scene.pmm> --profile <profile.json>
  node src/cli.mjs extract-pmm-keyframes-with-profile-registry --pmm <scene.pmm> --registry <profiles.json>
  node src/cli.mjs check-pmm-keyframe-profile --pmm <scene.pmm> --profile <profile.json>
  node src/cli.mjs check-pmm-keyframe-profile-registry --pmm <scene.pmm> --registry <profiles.json>
  node src/cli.mjs compare-pmm-keyframes-with-profile --pmm <scene.pmm> --profile <profile.json> --vmd <motion.vmd>
  node src/cli.mjs patch-pmm-vmd-diff-cluster --base <base.pmm> --donor-base <base.pmm> --donor-variant <with-motion.pmm> --donor-vmd <donor.vmd> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs inspect-pmm-patch-profile-registry --registry <profiles.json>
  node src/cli.mjs inventory-pmm-patch-profile-registries --registries <profiles.json,delta-profiles.json>
  node src/cli.mjs write-usable-pmm-patch-profile-registry --registries <profiles.json,delta-profiles.json> --out <usable-profiles.json>
  node src/cli.mjs check-pmm-vmd-patch-compatibility --registries <usable-profiles.json> --base <base.pmm> [--target-pmx <model.pmx>] --target-vmd <target.vmd>
  node src/cli.mjs plan-pmm-vmd-patch-from-any-profile-registry --registries <profiles.json,delta-profiles.json> --base <base.pmm> [--target-pmx <model.pmx>] --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs patch-pmm-vmd-from-any-profile-registry --registries <profiles.json,delta-profiles.json> --base <base.pmm> [--target-pmx <model.pmx>] --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs plan-pmm-vmd-patch-from-profile-registry --registry <profiles.json> --base <base.pmm> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs patch-pmm-vmd-from-profile-registry --registry <profiles.json> --base <base.pmm> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs plan-pmm-vmd-key-count-delta-from-profile-registry --registry <profiles.json> --base <base.pmm> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs patch-pmm-vmd-key-count-delta-from-profile-registry --registry <profiles.json> --base <base.pmm> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs patch-pmm-vmd-key-count-delta --base <base.pmm> --small-variant <small-motion.pmm> --small-vmd <small.vmd> --large-variant <large-motion.pmm> --large-vmd <large.vmd> --target-vmd <target.vmd> --out <patched.pmm>
  node src/cli.mjs write-test-vmd --out <motion.vmd> [--bone-name センター] [--bone-frame-keys "センター:30:1,2,3;左足:60:4,5,6"] [--bone-keys "30:1,2,3;60:4,5,6"] [--bone-rotation-keys "30:0.382683,0,0,0.92388"] [--bone-transform-keys "30:1,2,3:0.382683,0,0,0.92388"] [--bone-key-count 65535] [--bone-transform-key-count 1024] [--morph-name まばたき]
  node src/cli.mjs inspect-vmd <motion.vmd> [--limit <count>]
  node src/cli.mjs compare-vmd-pmm-motion --vmd <motion.vmd> --pmm <scene.pmm>
  node src/cli.mjs map-vmd-pmm-bone-frames --vmd <motion.vmd> --pmm <scene.pmm> [--bone-name <name> | --all-bones true] [--record-bytes 62]
  node src/cli.mjs write-pmm-investigation-kit --out-dir <dir> [--bone-name センター] [--morph-name まばたき]
  node src/cli.mjs analyze-pmm-fixture-motion --base <base.pmm> --variant <with-motion.pmm> [--vmd <motion.vmd>]
  node src/cli.mjs patch-pmm-fixture-motion --base <target-base.pmm> --donor-base <base.pmm> --donor-variant <with-motion.pmm> --out <patched.pmm>
  node src/cli.mjs rewrite-pmm-scalars <scene.pmm> --out <patched.pmm> [--frames 30:31] [--float32 1:7,2:8,3:9] [--u32-at 0x190:31] [--float32-at 0x1f2:7] [--hex-at 0x1e2:4000407f] [--insert-hex-at 0x20e:...]
  node src/cli.mjs write-pmm-unittest-bone-keys <one-key-template.pmm> --out <patched.pmm> --keys "30:1,2,3;60:4,5,6" [--oracle <handmade.pmm>] [--compact true]
  node src/cli.mjs write-pmm-unittest-vmd-bone-keys <one-key-template.pmm> --vmd <motion.vmd> --out <patched.pmm> [--bone-name <name>] [--allow-non-identity-rotation true] [--oracle <handmade.pmm>] [--compact true]
  node src/cli.mjs scan-pmm-motion <scene.pmm> [--int32 30] [--float32 1,2,3,0.5]
  node src/cli.mjs extract-pmm-motion-records <scene.pmm> [--limit 32]
  node src/cli.mjs dump-pmm-motion-records <scene.pmm> --record-start 0x1175 --count 319 --out <records.bin>
  node src/cli.mjs patch-pmm-motion-records <scene.pmm> --records <records.bin> --record-start 0x1175 --count 319 --out <patched.pmm>
  node src/cli.mjs verify-coverage --fixture <fixture.json> [--actual <oracle.jsonl>]`);
}
