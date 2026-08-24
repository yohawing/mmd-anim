const DUMPER_VERSION = "0.1.0";

export { DUMPER_VERSION };

export function assertOracleRecord(value, context = "record") {
  const errors = [];
  validateOracleRecord(value, context, errors);
  if (errors.length > 0) {
    const error = new Error(`Invalid MMD oracle ${context}: ${errors.join("; ")}`);
    error.errors = errors;
    throw error;
  }
  return value;
}

export function validateOracleRecord(value, context = "record", errors = []) {
  if (!isPlainObject(value)) {
    errors.push(`${context} must be an object`);
    return false;
  }

  requireAllowedKeys(value, ["schemaVersion", "source", "frame", "camera", "sceneParameters", "models"], context, errors);
  for (const key of ["schemaVersion", "source", "frame", "models"]) {
    if (!(key in value)) {
      errors.push(`${context}.${key} is required`);
    }
  }
  if (value.schemaVersion !== 1) {
    errors.push(`${context}.schemaVersion must be 1`);
  }
  if (!Number.isFinite(value.frame)) {
    errors.push(`${context}.frame must be finite`);
  }

  validateSource(value.source, `${context}.source`, errors);
  if (value.camera !== undefined) {
    validateCamera(value.camera, `${context}.camera`, errors);
  }
  if (value.sceneParameters !== undefined) {
    validateSceneParameters(value.sceneParameters, `${context}.sceneParameters`, errors);
  }
  validateArray(value.models, `${context}.models`, errors, (model, modelPath) => {
    validateModel(model, modelPath, errors);
  });

  return errors.length === 0;
}

function validateSource(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireAllowedKeys(value, ["mmdVersion", "dumperVersion", "project"], path, errors);
  requireString(value.mmdVersion, `${path}.mmdVersion`, errors, true);
  requireString(value.dumperVersion, `${path}.dumperVersion`, errors, true);
  if (value.project !== undefined) {
    requireString(value.project, `${path}.project`, errors, false);
  }
}

function validateCamera(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireAllowedKeys(value, ["available", "current", "keyframes"], path, errors);
  if (typeof value.available !== "boolean") {
    errors.push(`${path}.available must be boolean`);
  }
  if (value.available) {
    validateCameraCurrent(value.current, `${path}.current`, errors);
    validateArray(value.keyframes, `${path}.keyframes`, errors, (keyframe, keyframePath) => {
      validateCameraKeyframe(keyframe, keyframePath, errors);
    });
  }
}

function validateCameraCurrent(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(value, ["distance", "position", "rotation"], path, errors);
  requireFiniteNumber(value.distance, `${path}.distance`, errors);
  validateFiniteNumberArray(value.position, 3, `${path}.position`, errors);
  validateFiniteNumberArray(value.rotation, 3, `${path}.rotation`, errors);
}

function validateCameraKeyframe(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(
    value,
    [
      "index",
      "frame",
      "previousKeyframeIndex",
      "nextKeyframeIndex",
      "distance",
      "position",
      "rotation",
      "fov",
      "perspective",
      "selected",
      "followModelIndex",
      "followBoneIndex",
      "interpolation",
    ],
    path,
    errors,
  );
  requireNonNegativeInteger(value.index, `${path}.index`, errors);
  requireNonNegativeInteger(value.frame, `${path}.frame`, errors);
  requireInteger(value.previousKeyframeIndex, `${path}.previousKeyframeIndex`, errors);
  requireInteger(value.nextKeyframeIndex, `${path}.nextKeyframeIndex`, errors);
  requireFiniteNumber(value.distance, `${path}.distance`, errors);
  validateFiniteNumberArray(value.position, 3, `${path}.position`, errors);
  validateFiniteNumberArray(value.rotation, 3, `${path}.rotation`, errors);
  requireInteger(value.fov, `${path}.fov`, errors);
  if (typeof value.perspective !== "boolean") {
    errors.push(`${path}.perspective must be boolean`);
  }
  if (typeof value.selected !== "boolean") {
    errors.push(`${path}.selected must be boolean`);
  }
  requireInteger(value.followModelIndex, `${path}.followModelIndex`, errors);
  requireInteger(value.followBoneIndex, `${path}.followBoneIndex`, errors);
  validateCameraInterpolation(value.interpolation, `${path}.interpolation`, errors);
}

function validateCameraInterpolation(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  const channels = ["x", "y", "z", "rotation", "distance", "fov"];
  requireExactKeys(value, channels, path, errors);
  for (const channel of channels) {
    validateInterpolationChannel(value[channel], `${path}.${channel}`, errors);
  }
}

function validateInterpolationChannel(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(value, ["x1", "y1", "x2", "y2"], path, errors);
  for (const key of ["x1", "y1", "x2", "y2"]) {
    requireInteger(value[key], `${path}.${key}`, errors);
  }
}

function validateSceneParameters(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireAllowedKeys(value, ["available", "outputWidth", "outputHeight", "pmmPath"], path, errors);
  if (typeof value.available !== "boolean") {
    errors.push(`${path}.available must be boolean`);
  }
  if (value.available) {
    requireNonNegativeInteger(value.outputWidth, `${path}.outputWidth`, errors);
    requireNonNegativeInteger(value.outputHeight, `${path}.outputHeight`, errors);
    requireString(value.pmmPath, `${path}.pmmPath`, errors, false);
  }
}

function validateModel(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(value, ["index", "name", "filename", "visible", "bones", "morphs"], path, errors);
  requireNonNegativeInteger(value.index, `${path}.index`, errors);
  requireString(value.name, `${path}.name`, errors, false);
  requireString(value.filename, `${path}.filename`, errors, false);
  if (typeof value.visible !== "boolean") {
    errors.push(`${path}.visible must be boolean`);
  }
  validateArray(value.bones, `${path}.bones`, errors, (bone, bonePath) => {
    validateBone(bone, bonePath, errors);
  });
  validateArray(value.morphs, `${path}.morphs`, errors, (morph, morphPath) => {
    validateMorph(morph, morphPath, errors);
  });
}

function validateBone(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(value, ["index", "name", "worldMatrix"], path, errors);
  requireNonNegativeInteger(value.index, `${path}.index`, errors);
  requireString(value.name, `${path}.name`, errors, false);
  if (!Array.isArray(value.worldMatrix) || value.worldMatrix.length !== 16) {
    errors.push(`${path}.worldMatrix must have 16 finite numbers`);
    return;
  }
  for (let i = 0; i < value.worldMatrix.length; i += 1) {
    if (!Number.isFinite(value.worldMatrix[i])) {
      errors.push(`${path}.worldMatrix[${i}] must be finite`);
    }
  }
}

function validateMorph(value, path, errors) {
  if (!isPlainObject(value)) {
    errors.push(`${path} must be an object`);
    return;
  }
  requireExactKeys(value, ["index", "name", "weight"], path, errors);
  requireNonNegativeInteger(value.index, `${path}.index`, errors);
  requireString(value.name, `${path}.name`, errors, false);
  if (!Number.isFinite(value.weight)) {
    errors.push(`${path}.weight must be finite`);
  }
}

function validateFiniteNumberArray(value, length, path, errors) {
  if (!Array.isArray(value) || value.length !== length) {
    errors.push(`${path} must have ${length} finite numbers`);
    return;
  }
  for (let i = 0; i < value.length; i += 1) {
    requireFiniteNumber(value[i], `${path}[${i}]`, errors);
  }
}

function validateArray(value, path, errors, validateItem) {
  if (!Array.isArray(value)) {
    errors.push(`${path} must be an array`);
    return;
  }
  for (let i = 0; i < value.length; i += 1) {
    validateItem(value[i], `${path}[${i}]`);
  }
}

function requireExactKeys(value, keys, path, errors) {
  requireAllowedKeys(value, keys, path, errors);
  for (const key of keys) {
    if (!(key in value)) {
      errors.push(`${path}.${key} is required`);
    }
  }
}

function requireAllowedKeys(value, keys, path, errors) {
  const allowed = new Set(keys);
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) {
      errors.push(`${path}.${key} is not allowed`);
    }
  }
}

function requireNonNegativeInteger(value, path, errors) {
  if (!Number.isInteger(value) || value < 0) {
    errors.push(`${path} must be a non-negative integer`);
  }
}

function requireInteger(value, path, errors) {
  if (!Number.isInteger(value)) {
    errors.push(`${path} must be an integer`);
  }
}

function requireFiniteNumber(value, path, errors) {
  if (!Number.isFinite(value)) {
    errors.push(`${path} must be finite`);
  }
}

function requireString(value, path, errors, minLength) {
  if (typeof value !== "string") {
    errors.push(`${path} must be a string`);
    return;
  }
  if (minLength && value.length === 0) {
    errors.push(`${path} must not be empty`);
  }
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
