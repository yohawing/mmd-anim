import { readPmmMotionRecords } from "./pmm-motion-records.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function compareVmdToPmmMotion(options) {
  const vmd = await readVmdInventory(options.vmd, { limit: options.limit ?? 8 });
  const pmm = await readPmmMotionRecords(options.pmm, {
    markerHex: options.markerHex,
    markerOffsetInRecord: options.markerOffsetInRecord,
    recordByteLength: options.recordByteLength,
    limit: options.limit ?? 8,
  });
  const vmdMotionFrameTotal =
    vmd.counts.boneFrames +
    vmd.counts.morphFrames +
    vmd.counts.cameraFrames +
    vmd.counts.lightFrames +
    vmd.counts.selfShadowFrames +
    vmd.counts.propertyFrames;

  return {
    vmd: {
      file: options.vmd,
      modelName: vmd.modelName,
      counts: vmd.counts,
      maxFrame: vmd.maxFrame,
      motionFrameTotal: vmdMotionFrameTotal,
      boneNameCounts: vmd.boneNameCounts,
      morphNameCounts: vmd.morphNameCounts,
    },
    pmm: {
      file: options.pmm,
      byteLength: pmm.byteLength,
      markerHexes: pmm.markerHexes,
      recordByteLength: pmm.recordByteLength,
      recordTotal: pmm.recordTotal,
      summary: pmm.summary,
    },
    ratios: {
      pmmRecordsPerVmdMotionFrame: vmdMotionFrameTotal === 0 ? null : roundFloat(pmm.recordTotal / vmdMotionFrameTotal),
      pmmRecordsPerVmdBoneFrame: vmd.counts.boneFrames === 0 ? null : roundFloat(pmm.recordTotal / vmd.counts.boneFrames),
      pmmRecordsPerVmdMorphFrame: vmd.counts.morphFrames === 0 ? null : roundFloat(pmm.recordTotal / vmd.counts.morphFrames),
    },
  };
}

function roundFloat(value) {
  if (!Number.isFinite(value)) {
    return value;
  }
  return Math.round(value * 1_000_000) / 1_000_000;
}
