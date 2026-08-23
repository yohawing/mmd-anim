#pragma once

#include <array>
#include <string>
#include <vector>

namespace mmd_oracle {

struct BoneSnapshot {
  int index = 0;
  std::string name;
  std::array<float, 16> worldMatrix{};
};

struct MorphSnapshot {
  int index = 0;
  std::string name;
  float weight = 0.0f;
};

struct CameraInterpolationSnapshot {
  std::array<int, 4> x{};
  std::array<int, 4> y{};
  std::array<int, 4> z{};
  std::array<int, 4> rotation{};
  std::array<int, 4> distance{};
  std::array<int, 4> fov{};
};

struct CameraKeyframeSnapshot {
  int index = 0;
  int frame = 0;
  int previousKeyframeIndex = 0;
  int nextKeyframeIndex = 0;
  float distance = 0.0f;
  std::array<float, 3> position{};
  std::array<float, 3> rotation{};
  int fov = 0;
  bool perspective = true;
  bool selected = false;
  int followModelIndex = -1;
  int followBoneIndex = 0;
  CameraInterpolationSnapshot interpolation;
};

struct CameraSnapshot {
  bool available = false;
  float distance = 0.0f;
  std::array<float, 3> position{};
  std::array<float, 3> rotation{};
  std::vector<CameraKeyframeSnapshot> keyframes;
};

struct SceneParametersSnapshot {
  bool available = false;
  int outputWidth = 0;
  int outputHeight = 0;
  std::string pmmPath;
};

struct ModelSnapshot {
  int index = 0;
  std::string name;
  std::string filename;
  bool visible = true;
  std::vector<BoneSnapshot> bones;
  std::vector<MorphSnapshot> morphs;
};

struct OracleRecord {
  int schemaVersion = 1;
  std::string mmdVersion = "9.32-x64";
  std::string dumperVersion = "0.1.0";
  std::string project;
  double frame = 0.0;
  CameraSnapshot camera;
  SceneParametersSnapshot sceneParameters;
  std::vector<ModelSnapshot> models;
};

} // namespace mmd_oracle
