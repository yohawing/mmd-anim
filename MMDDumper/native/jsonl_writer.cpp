#include "jsonl_writer.h"

#include <cmath>
#include <iomanip>
#include <stdexcept>

namespace mmd_oracle {
namespace {

void writeJsonString(std::ostream& out, const std::string& value) {
  out << '"';
  for (const char ch : value) {
    switch (ch) {
      case '\\':
        out << "\\\\";
        break;
      case '"':
        out << "\\\"";
        break;
      case '\n':
        out << "\\n";
        break;
      case '\r':
        out << "\\r";
        break;
      case '\t':
        out << "\\t";
        break;
      default:
        out << ch;
        break;
    }
  }
  out << '"';
}

void writeFiniteNumber(std::ostream& out, double value) {
  if (!std::isfinite(value)) {
    throw std::runtime_error("oracle record contains a non-finite number");
  }
  out << std::setprecision(9) << value;
}

void writeMatrix(std::ostream& out, const std::array<float, 16>& matrix) {
  out << '[';
  for (size_t i = 0; i < matrix.size(); ++i) {
    if (i != 0) {
      out << ',';
    }
    writeFiniteNumber(out, matrix[i]);
  }
  out << ']';
}

template <typename T, size_t N>
void writeArray(std::ostream& out, const std::array<T, N>& values) {
  out << '[';
  for (size_t i = 0; i < values.size(); ++i) {
    if (i != 0) {
      out << ',';
    }
    writeFiniteNumber(out, values[i]);
  }
  out << ']';
}

void writeInterpolationChannel(std::ostream& out, const std::array<int, 4>& values) {
  out << "{\"x1\":" << values[0]
      << ",\"y1\":" << values[1]
      << ",\"x2\":" << values[2]
      << ",\"y2\":" << values[3] << '}';
}

void writeCameraInterpolation(std::ostream& out, const CameraInterpolationSnapshot& interpolation) {
  out << "{\"x\":";
  writeInterpolationChannel(out, interpolation.x);
  out << ",\"y\":";
  writeInterpolationChannel(out, interpolation.y);
  out << ",\"z\":";
  writeInterpolationChannel(out, interpolation.z);
  out << ",\"rotation\":";
  writeInterpolationChannel(out, interpolation.rotation);
  out << ",\"distance\":";
  writeInterpolationChannel(out, interpolation.distance);
  out << ",\"fov\":";
  writeInterpolationChannel(out, interpolation.fov);
  out << '}';
}

void writeCamera(std::ostream& out, const CameraSnapshot& camera) {
  out << "\"camera\":{\"available\":" << (camera.available ? "true" : "false");
  if (camera.available) {
    out << ",\"current\":{\"distance\":";
    writeFiniteNumber(out, camera.distance);
    out << ",\"position\":";
    writeArray(out, camera.position);
    out << ",\"rotation\":";
    writeArray(out, camera.rotation);
    out << "},\"keyframes\":[";
    for (size_t index = 0; index < camera.keyframes.size(); ++index) {
      const auto& keyframe = camera.keyframes[index];
      if (index != 0) {
        out << ',';
      }
      out << "{\"index\":" << keyframe.index
          << ",\"frame\":" << keyframe.frame
          << ",\"previousKeyframeIndex\":" << keyframe.previousKeyframeIndex
          << ",\"nextKeyframeIndex\":" << keyframe.nextKeyframeIndex
          << ",\"distance\":";
      writeFiniteNumber(out, keyframe.distance);
      out << ",\"position\":";
      writeArray(out, keyframe.position);
      out << ",\"rotation\":";
      writeArray(out, keyframe.rotation);
      out << ",\"fov\":" << keyframe.fov
          << ",\"perspective\":" << (keyframe.perspective ? "true" : "false")
          << ",\"selected\":" << (keyframe.selected ? "true" : "false")
          << ",\"followModelIndex\":" << keyframe.followModelIndex
          << ",\"followBoneIndex\":" << keyframe.followBoneIndex
          << ",\"interpolation\":";
      writeCameraInterpolation(out, keyframe.interpolation);
      out << '}';
    }
    out << ']';
  }
  out << '}';
}

void writeSceneParameters(std::ostream& out, const SceneParametersSnapshot& sceneParameters) {
  out << "\"sceneParameters\":{\"available\":" << (sceneParameters.available ? "true" : "false");
  if (sceneParameters.available) {
    out << ",\"outputWidth\":" << sceneParameters.outputWidth
        << ",\"outputHeight\":" << sceneParameters.outputHeight
        << ",\"pmmPath\":";
    writeJsonString(out, sceneParameters.pmmPath);
  }
  out << '}';
}

} // namespace

void writeOracleRecordJsonl(std::ostream& out, const OracleRecord& record) {
  out << "{\"schemaVersion\":" << record.schemaVersion << ",\"source\":{\"mmdVersion\":";
  writeJsonString(out, record.mmdVersion);
  out << ",\"dumperVersion\":";
  writeJsonString(out, record.dumperVersion);
  if (!record.project.empty()) {
    out << ",\"project\":";
    writeJsonString(out, record.project);
  }
  out << "},\"frame\":";
  writeFiniteNumber(out, record.frame);
  out << ',';
  writeCamera(out, record.camera);
  out << ',';
  writeSceneParameters(out, record.sceneParameters);
  out << ",\"models\":[";

  for (size_t modelIndex = 0; modelIndex < record.models.size(); ++modelIndex) {
    const auto& model = record.models[modelIndex];
    if (modelIndex != 0) {
      out << ',';
    }
    out << "{\"index\":" << model.index << ",\"name\":";
    writeJsonString(out, model.name);
    out << ",\"filename\":";
    writeJsonString(out, model.filename);
    out << ",\"visible\":" << (model.visible ? "true" : "false") << ",\"bones\":[";
    for (size_t boneIndex = 0; boneIndex < model.bones.size(); ++boneIndex) {
      const auto& bone = model.bones[boneIndex];
      if (boneIndex != 0) {
        out << ',';
      }
      out << "{\"index\":" << bone.index << ",\"name\":";
      writeJsonString(out, bone.name);
      out << ",\"worldMatrix\":";
      writeMatrix(out, bone.worldMatrix);
      out << '}';
    }
    out << "],\"morphs\":[";
    for (size_t morphIndex = 0; morphIndex < model.morphs.size(); ++morphIndex) {
      const auto& morph = model.morphs[morphIndex];
      if (morphIndex != 0) {
        out << ',';
      }
      out << "{\"index\":" << morph.index << ",\"name\":";
      writeJsonString(out, morph.name);
      out << ",\"weight\":";
      writeFiniteNumber(out, morph.weight);
      out << '}';
    }
    out << "]}";
  }
  out << "]}\n";
}

} // namespace mmd_oracle
