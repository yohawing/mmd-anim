#include "mmd_export_provider.h"

#include <array>
#include <cmath>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>
#include <windows.h>

// Minimal ABI mirror of the MMDExport API used by the oracle dumper. This
// avoids tracking the local MMD SDK binary/header directory in git.
struct D3DMATRIX {
  float _11, _12, _13, _14;
  float _21, _22, _23, _24;
  float _31, _32, _33, _34;
  float _41, _42, _43, _44;
};

extern "C" {
float ExpGetFrameTime();
int ExpGetPmdNum();
char* ExpGetPmdFilename(int);
int ExpGetPmdBoneNum(int);
char* ExpGetPmdBoneName(int, int);
D3DMATRIX ExpGetPmdBoneWorldMat(int, int);
int ExpGetPmdMorphNum(int);
char* ExpGetPmdMorphName(int, int);
float ExpGetPmdMorphValue(int, int);
bool ExpGetPmdDisp(int);
}

namespace mmd_oracle {
namespace {

constexpr int kMaxCameraKeyframes = 10000;

struct Float3 {
  float x;
  float y;
  float z;
};

struct CameraKeyFrameData {
  int frame_no;
  int pre_index;
  int next_index;
  float length;
  Float3 xyz;
  Float3 rxyz;
  char hokan1_x[6];
  char hokan1_y[6];
  char hokan2_x[6];
  char hokan2_y[6];
  int is_perspective;
  int view_angle;
  int is_selected;
  int looking_model_index;
  int looking_bone_index;
};

struct MMDMainData {
  int __unknown10[2];
  int mouse_x, mouse_y;
  int pre_mouse_x, pre_mouse_y;
  int key_up;
  int key_down;
  int key_left;
  int key_right;
  int key_shift;
  int key_space;
  int key_f9;
  int key_x_or_f11;
  int key_z;
  int key_c;
  int key_v;
  int key_d;
  int key_a;
  int key_b;
  int key_g;
  int key_s;
  int key_i;
  int key_h;
  int key_k;
  int key_p;
  int key_u;
  int key_j;
  int key_f;
  int key_r;
  int key_l;
  int key_close_bracket;
  int key_backslash;
  int key_tab;
  int __unknown20[14];
  int key_enter;
  int key_ctrl;
  int key_alt;
  int __unknown30;
  void* __unknown_pointer;
  int __unknown40[155];
  int __unknown48;
  Float3 rxyz;
  int __unknown49[2];
  float counter_f;
  int counter;
  int __unknown50[2];
  Float3 xyz;
  int __unknown60[22];
  CameraKeyFrameData* camera_key_frame;
  void* __unknown_pointer20[258];
  void* model_data[255];
  int select_model;
  int select_bone_type;
  int __unknown70[4];
  float __unknown71;
  int mouse_over_move;
  int __unknown80[17];
  int left_frame;
  int __unknown90;
  int pre_left_frame;
  int now_frame;
  int __unknown100[160800];
  char __unknown101;
  unsigned char edit_interpolation_curve[4];
  int __unknown103[2983];
  char is_camera_select;
  char is_model_bone_select[127];
  int __unknown110[318];
  int output_size_x;
  int output_size_y;
  int __unknown119[2];
  float length;
  unsigned char __unknown120[24];
  wchar_t pmm_path[256];
};

static_assert(sizeof(CameraKeyFrameData) == 84, "Unexpected MMD camera keyframe ABI size");
static_assert(offsetof(MMDMainData, now_frame) == 5200, "Unexpected MMDMainData.now_frame offset");
static_assert(offsetof(MMDMainData, edit_interpolation_curve) == 648405, "Unexpected MMDMainData.edit_interpolation_curve offset");
static_assert(offsetof(MMDMainData, is_camera_select) == 660344, "Unexpected MMDMainData.is_camera_select offset");
static_assert(offsetof(MMDMainData, rxyz) == 840, "Unexpected MMDMainData.rxyz offset");
static_assert(offsetof(MMDMainData, xyz) == 876, "Unexpected MMDMainData.xyz offset");
static_assert(offsetof(MMDMainData, length) == 661760, "Unexpected MMDMainData.length offset");
static_assert(offsetof(MMDMainData, pmm_path) == 661788, "Unexpected MMDMainData.pmm_path offset");

std::string safeString(char* value) {
  if (value == nullptr || value[0] == '\0') {
    return std::string();
  }

  const int wideLength = MultiByteToWideChar(932, 0, value, -1, nullptr, 0);
  if (wideLength <= 0) {
    return std::string(value);
  }

  std::wstring wide(static_cast<size_t>(wideLength), L'\0');
  MultiByteToWideChar(932, 0, value, -1, wide.data(), wideLength);

  const int utf8Length = WideCharToMultiByte(CP_UTF8, 0, wide.c_str(), -1, nullptr, 0, nullptr, nullptr);
  if (utf8Length <= 0) {
    return std::string(value);
  }

  std::string utf8(static_cast<size_t>(utf8Length), '\0');
  WideCharToMultiByte(CP_UTF8, 0, wide.c_str(), -1, utf8.data(), utf8Length, nullptr, nullptr);
  utf8.resize(static_cast<size_t>(utf8Length - 1));
  return utf8;
}

std::string wideStringToUtf8(const wchar_t* value) {
  if (value == nullptr || value[0] == L'\0') {
    return std::string();
  }
  const int utf8Length = WideCharToMultiByte(CP_UTF8, 0, value, -1, nullptr, 0, nullptr, nullptr);
  if (utf8Length <= 0) {
    return std::string();
  }
  std::string utf8(static_cast<size_t>(utf8Length), '\0');
  WideCharToMultiByte(CP_UTF8, 0, value, -1, utf8.data(), utf8Length, nullptr, nullptr);
  utf8.resize(static_cast<size_t>(utf8Length - 1));
  return utf8;
}

std::string environmentString(const char* name) {
  const DWORD requiredLength = GetEnvironmentVariableA(name, nullptr, 0);
  if (requiredLength <= 1) {
    return std::string();
  }
  std::string value(requiredLength, '\0');
  const DWORD writtenLength = GetEnvironmentVariableA(name, value.data(), requiredLength);
  if (writtenLength == 0 || writtenLength >= requiredLength) {
    return std::string();
  }
  value.resize(writtenLength);
  return value;
}

bool environmentFlagEnabled(const char* name, bool fallback) {
  const std::string value = environmentString(name);
  if (value.empty()) {
    return fallback;
  }
  return value != "0" && value != "false" && value != "FALSE";
}

int plausibleNonNegativeDimension(int value) {
  return value > 0 && value < 100000 ? value : 0;
}

bool nearlyEqual(float left, float right) {
  return std::abs(left - right) <= 0.00001f;
}

float readFloatAt(const BYTE* bytes, size_t offset) {
  float value = 0.0f;
  std::memcpy(&value, bytes + offset, sizeof(value));
  return value;
}

int readIntAt(const BYTE* bytes, size_t offset) {
  int value = 0;
  std::memcpy(&value, bytes + offset, sizeof(value));
  return value;
}

void writeFloatArray(std::ostream& out, const std::array<float, 3>& values) {
  out << '[' << values[0] << ',' << values[1] << ',' << values[2] << ']';
}

void writeMemoryProbeMatches(std::ostream& out, const char* label, const std::vector<size_t>& offsets) {
  out << ",\"" << label << "\":[";
  for (size_t index = 0; index < offsets.size(); ++index) {
    if (index != 0) {
      out << ',';
    }
    out << offsets[index];
  }
  out << ']';
}

void writeIntProbeMatches(std::ostream& out, const char* label, const std::vector<size_t>& offsets) {
  writeMemoryProbeMatches(out, label, offsets);
}

void writeCameraMemoryProbe(const MMDMainData* data, double frame, const CameraKeyframeSnapshot& target) {
  const std::string path = environmentString("MMD_ORACLE_MEMORY_PROBE_PATH");
  if (path.empty()) {
    return;
  }

  const auto* bytes = reinterpret_cast<const BYTE*>(data);
  constexpr size_t byteLength = sizeof(MMDMainData);
  std::vector<size_t> distanceOffsets;
  std::vector<size_t> positionOffsets;
  std::vector<size_t> rotationOffsets;
  std::vector<size_t> fovOffsets;
  std::vector<size_t> perspectiveOffsets;

  for (size_t offset = 0; offset + sizeof(float) <= byteLength; offset += sizeof(float)) {
    if (distanceOffsets.size() < 128 && nearlyEqual(readFloatAt(bytes, offset), target.distance)) {
      distanceOffsets.push_back(offset);
    }
    if (offset + sizeof(float) * 3 <= byteLength) {
      const std::array<float, 3> triple = {
          readFloatAt(bytes, offset),
          readFloatAt(bytes, offset + sizeof(float)),
          readFloatAt(bytes, offset + sizeof(float) * 2),
      };
      if (positionOffsets.size() < 64 && nearlyEqual(triple[0], target.position[0]) &&
          nearlyEqual(triple[1], target.position[1]) && nearlyEqual(triple[2], target.position[2])) {
        positionOffsets.push_back(offset);
      }
      if (rotationOffsets.size() < 64 && nearlyEqual(triple[0], target.rotation[0]) &&
          nearlyEqual(triple[1], target.rotation[1]) && nearlyEqual(triple[2], target.rotation[2])) {
        rotationOffsets.push_back(offset);
      }
    }
    if (fovOffsets.size() < 128 && readIntAt(bytes, offset) == target.fov) {
      fovOffsets.push_back(offset);
    }
    const int perspectiveValue = target.perspective ? 0 : 1;
    if (perspectiveOffsets.size() < 128 && readIntAt(bytes, offset) == perspectiveValue) {
      perspectiveOffsets.push_back(offset);
    }
  }

  std::ofstream out(path, std::ios::app);
  if (!out) {
    return;
  }
  out << "{\"frame\":" << frame << ",\"targetFrame\":" << target.frame;
  out << ",\"target\":{\"distance\":" << target.distance << ",\"position\":";
  writeFloatArray(out, target.position);
  out << ",\"rotation\":";
  writeFloatArray(out, target.rotation);
  out << '}';
  writeMemoryProbeMatches(out, "distanceOffsets", distanceOffsets);
  writeMemoryProbeMatches(out, "positionOffsets", positionOffsets);
  writeMemoryProbeMatches(out, "rotationOffsets", rotationOffsets);
  writeIntProbeMatches(out, "fovOffsets", fovOffsets);
  writeIntProbeMatches(out, "perspectiveOffsets", perspectiveOffsets);
  out << "}\n";
}

void writeRawCameraOffsetProbe(const MMDMainData* data, double frame) {
  const std::string path = environmentString("MMD_ORACLE_MEMORY_RAW_PROBE_PATH");
  if (path.empty()) {
    return;
  }
  const auto* bytes = reinterpret_cast<const BYTE*>(data);
  const auto writeTripletAt = [&bytes](std::ostream& out, size_t offset) {
    out << '[' << readFloatAt(bytes, offset) << ',' << readFloatAt(bytes, offset + 4) << ','
        << readFloatAt(bytes, offset + 8) << ']';
  };
  const auto writeFloatWindow = [&bytes](std::ostream& out, size_t offset, size_t count) {
    out << '[';
    for (size_t index = 0; index < count; ++index) {
      if (index != 0) {
        out << ',';
      }
      out << readFloatAt(bytes, offset + index * 4);
    }
    out << ']';
  };
  const auto writeIntWindow = [&bytes](std::ostream& out, size_t offset, size_t count) {
    out << '[';
    for (size_t index = 0; index < count; ++index) {
      if (index != 0) {
        out << ',';
      }
      out << readIntAt(bytes, offset + index * 4);
    }
    out << ']';
  };
  std::ofstream out(path, std::ios::app);
  if (!out) {
    return;
  }
  out << "{\"frame\":" << frame;
  out << ",\"offset820\":";
  writeFloatWindow(out, 820, 32);
  out << ",\"offset820i\":";
  writeIntWindow(out, 820, 32);
  out << ",\"offset840\":";
  writeTripletAt(out, 840);
  out << ",\"offset864\":";
  writeTripletAt(out, 864);
  out << ",\"offset661720\":";
  writeFloatWindow(out, 661720, 24);
  out << ",\"offset661720i\":";
  writeIntWindow(out, 661720, 24);
  out << ",\"knownOffsets\":{\"rxyz\":840,\"xyz\":876,\"length\":661760}}\n";
}

std::array<float, 16> toArray(const D3DMATRIX& matrix) {
  return {
      matrix._11, matrix._12, matrix._13, matrix._14,
      matrix._21, matrix._22, matrix._23, matrix._24,
      matrix._31, matrix._32, matrix._33, matrix._34,
      matrix._41, matrix._42, matrix._43, matrix._44,
  };
}

std::array<float, 3> toArray3(const Float3& value) {
  return {value.x, value.y, value.z};
}

int interpolationByte(char value) {
  return static_cast<int>(static_cast<unsigned char>(value));
}

std::array<int, 4> interpolationChannel(const CameraKeyFrameData& keyframe, int channel) {
  return {
      interpolationByte(keyframe.hokan1_x[channel]),
      interpolationByte(keyframe.hokan1_y[channel]),
      interpolationByte(keyframe.hokan2_x[channel]),
      interpolationByte(keyframe.hokan2_y[channel]),
  };
}

CameraInterpolationSnapshot captureCameraInterpolation(const CameraKeyFrameData& keyframe) {
  CameraInterpolationSnapshot interpolation;
  interpolation.x = interpolationChannel(keyframe, 0);
  interpolation.y = interpolationChannel(keyframe, 1);
  interpolation.z = interpolationChannel(keyframe, 2);
  interpolation.rotation = interpolationChannel(keyframe, 3);
  interpolation.distance = interpolationChannel(keyframe, 4);
  interpolation.fov = interpolationChannel(keyframe, 5);
  return interpolation;
}

CameraKeyframeSnapshot captureCameraKeyframe(const CameraKeyFrameData& keyframe, int index) {
  CameraKeyframeSnapshot snapshot;
  snapshot.index = index;
  snapshot.frame = keyframe.frame_no;
  snapshot.previousKeyframeIndex = keyframe.pre_index;
  snapshot.nextKeyframeIndex = keyframe.next_index;
  snapshot.distance = keyframe.length;
  snapshot.position = toArray3(keyframe.xyz);
  snapshot.rotation = toArray3(keyframe.rxyz);
  snapshot.fov = keyframe.view_angle;
  snapshot.perspective = keyframe.is_perspective == 0;
  snapshot.selected = keyframe.is_selected != 0;
  snapshot.followModelIndex = keyframe.looking_model_index;
  snapshot.followBoneIndex = keyframe.looking_bone_index;
  snapshot.interpolation = captureCameraInterpolation(keyframe);
  return snapshot;
}

MMDMainData* getMmdMainData() {
  const HMODULE mainModule = GetModuleHandleW(nullptr);
  if (mainModule == nullptr) {
    return nullptr;
  }
  auto pointer = reinterpret_cast<BYTE**>(reinterpret_cast<BYTE*>(mainModule) + 0x1445F8);
  if (IsBadReadPtr(pointer, sizeof(INT_PTR)) != 0) {
    return nullptr;
  }
  auto data = reinterpret_cast<MMDMainData*>(*pointer);
  if (data == nullptr || IsBadReadPtr(data, sizeof(MMDMainData)) != 0) {
    return nullptr;
  }
  return data;
}

void captureMmdMainDataSnapshot(OracleRecord& record, double frame) {
  const MMDMainData* data = getMmdMainData();
  if (data == nullptr) {
    return;
  }
  writeRawCameraOffsetProbe(data, frame);

  record.camera.available = true;
  record.camera.distance = data->length;
  record.camera.position = toArray3(data->xyz);
  record.camera.rotation = toArray3(data->rxyz);

  if (environmentFlagEnabled("MMD_ORACLE_CAMERA_KEYFRAMES", true)) {
    const auto* frames = data->camera_key_frame;
    if (frames == nullptr || IsBadReadPtr(frames, sizeof(CameraKeyFrameData)) != 0) {
      return;
    }
    int index = 0;
    int guard = 0;
    while (index >= 0 && index < kMaxCameraKeyframes && guard < kMaxCameraKeyframes) {
      record.camera.keyframes.push_back(captureCameraKeyframe(frames[index], index));
      const int nextIndex = frames[index].next_index;
      ++guard;
      if (nextIndex == 0 || nextIndex == index) {
        break;
      }
      index = nextIndex;
    }

    for (const auto& keyframe : record.camera.keyframes) {
      if (std::abs(static_cast<double>(keyframe.frame) - frame) < 0.01) {
        writeCameraMemoryProbe(data, frame, keyframe);
        record.camera.distance = keyframe.distance;
        record.camera.position = keyframe.position;
        record.camera.rotation = keyframe.rotation;
        break;
      }
    }
  }

  record.sceneParameters.available = true;
  record.sceneParameters.outputWidth = plausibleNonNegativeDimension(data->output_size_x);
  record.sceneParameters.outputHeight = plausibleNonNegativeDimension(data->output_size_y);
  record.sceneParameters.pmmPath = wideStringToUtf8(data->pmm_path);
  if (record.sceneParameters.pmmPath.empty()) {
    record.sceneParameters.pmmPath = environmentString("MMD_ORACLE_PROJECT_PATH");
  }
}

} // namespace

OracleRecord captureMmdExportSnapshot(double frame, const char* projectPath) {
  OracleRecord record;
  record.frame = frame;
  record.project = projectPath == nullptr ? environmentString("MMD_ORACLE_PROJECT_PATH") : std::string(projectPath);
  captureMmdMainDataSnapshot(record, frame);

  if (!environmentFlagEnabled("MMD_ORACLE_MODEL_CHANNELS", true)) {
    return record;
  }

  const int modelCount = ExpGetPmdNum();
  record.models.reserve(modelCount);
  for (int modelIndex = 0; modelIndex < modelCount; ++modelIndex) {
    ModelSnapshot model;
    model.index = modelIndex;
    model.filename = safeString(ExpGetPmdFilename(modelIndex));
    model.name = model.filename;
    model.visible = ExpGetPmdDisp(modelIndex);

    const int boneCount = ExpGetPmdBoneNum(modelIndex);
    model.bones.reserve(boneCount);
    for (int boneIndex = 0; boneIndex < boneCount; ++boneIndex) {
      BoneSnapshot bone;
      bone.index = boneIndex;
      bone.name = safeString(ExpGetPmdBoneName(modelIndex, boneIndex));
      bone.worldMatrix = toArray(ExpGetPmdBoneWorldMat(modelIndex, boneIndex));
      model.bones.push_back(bone);
    }

    const int morphCount = ExpGetPmdMorphNum(modelIndex);
    model.morphs.reserve(morphCount);
    for (int morphIndex = 0; morphIndex < morphCount; ++morphIndex) {
      MorphSnapshot morph;
      morph.index = morphIndex;
      morph.name = safeString(ExpGetPmdMorphName(modelIndex, morphIndex));
      morph.weight = ExpGetPmdMorphValue(modelIndex, morphIndex);
      model.morphs.push_back(morph);
    }

    record.models.push_back(model);
  }

  return record;
}

double getMmdExportFrame() {
  return static_cast<double>(ExpGetFrameTime()) * 30.0;
}

} // namespace mmd_oracle
