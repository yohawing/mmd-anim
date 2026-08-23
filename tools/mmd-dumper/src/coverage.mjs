import { readFixture } from "./fixture.mjs";
import { readOracleJsonl } from "./jsonl.mjs";

export async function verifyFixtureCoverage(options) {
  const fixture = await readFixture(options.fixture);
  const records = await readOracleJsonl(options.actual ?? fixture.output);
  if (fixture.playback && options.frames === undefined) {
    return verifyOraclePlaybackCoverage({
      records,
      frames: fixture.frames,
      frameEpsilon: options.frameEpsilon ?? 0.01,
      playbackToleranceFrames: options.playbackToleranceFrames ?? 2,
      requireCamera: options.requireCamera ?? Boolean(fixture.dump?.camera),
    });
  }
  return verifyOracleCoverage({
    records,
    frames: options.frames ?? fixture.frames,
    frameEpsilon: options.frameEpsilon ?? 0.01,
    requireBones: options.requireBones ?? true,
    requireMorphs: options.requireMorphs ?? true,
    requireCamera: options.requireCamera ?? Boolean(fixture.dump?.camera && fixture.dump?.bones === false && fixture.dump?.morphs === false),
  });
}

export function verifyOraclePlaybackCoverage(options) {
  const records = options.records;
  const frames = options.frames;
  const frameEpsilon = options.frameEpsilon ?? 0.01;
  const playbackToleranceFrames = options.playbackToleranceFrames ?? 2;
  const observedFrames = records.map((record) => record.frame).filter((frame) => Number.isFinite(frame));
  const firstFrame = observedFrames[0];
  const maxFrame = observedFrames.length > 0 ? Math.max(...observedFrames) : undefined;
  const expectedFirstFrame = Math.min(...frames);
  const expectedLastFrame = Math.max(...frames);
  const firstFrameOk = firstFrame !== undefined && Math.abs(firstFrame - expectedFirstFrame) <= frameEpsilon;
  const lastFrameOk = maxFrame !== undefined && maxFrame + playbackToleranceFrames >= expectedLastFrame;
  const cameraRecords = records.filter((record) => record.camera?.available === true && record.camera?.current !== undefined);
  const cameraOk = !options.requireCamera || cameraRecords.length === records.length;

  return {
    ok: records.length > 0 && firstFrameOk && lastFrameOk && cameraOk,
    mode: "playback",
    records: records.length,
    firstFrame,
    maxFrame,
    expectedFirstFrame,
    expectedLastFrame,
    frameEpsilon,
    playbackToleranceFrames,
    camera: {
      required: Boolean(options.requireCamera),
      records: cameraRecords.length,
      ok: cameraOk,
    },
  };
}

export function verifyOracleCoverage(options) {
  const records = options.records;
  const frames = options.frames;
  const frameEpsilon = options.frameEpsilon ?? 0.01;
  const targets = frames.map((target) => {
    const nearest = findNearestRecord(records, target);
    const modelSummaries = nearest
      ? nearest.models.map((model) => ({
          index: model.index,
          name: model.name,
          bones: model.bones.length,
          morphs: model.morphs.length,
        }))
      : [];
    const hasFrame = nearest !== null && Math.abs(nearest.frame - target) <= frameEpsilon;
    const hasBones = !options.requireBones || modelSummaries.every((model) => model.bones > 0);
    const hasMorphs = !options.requireMorphs || modelSummaries.every((model) => model.morphs > 0);
    const hasModels = modelSummaries.length > 0;
    const cameraSummary = nearest?.camera
      ? {
          available: nearest.camera.available === true,
          current: nearest.camera.current !== undefined,
          keyframes: Array.isArray(nearest.camera.keyframes) ? nearest.camera.keyframes.length : undefined,
        }
      : undefined;
    const hasCamera = !options.requireCamera || Boolean(cameraSummary?.available && cameraSummary?.current);
    return {
      target,
      matchedFrame: nearest?.frame,
      frameDiff: nearest ? Math.abs(nearest.frame - target) : null,
      ok: hasFrame && hasCamera && (options.requireCamera || (hasModels && hasBones && hasMorphs)),
      models: modelSummaries,
      camera: cameraSummary,
    };
  });

  return {
    ok: targets.every((target) => target.ok),
    records: records.length,
    firstFrame: records[0]?.frame,
    lastFrame: records.at(-1)?.frame,
    frameEpsilon,
    targets,
  };
}

function findNearestRecord(records, target) {
  if (records.length === 0) {
    return null;
  }
  return records.reduce((best, record) => (Math.abs(record.frame - target) <= Math.abs(best.frame - target) ? record : best), records[0]);
}
