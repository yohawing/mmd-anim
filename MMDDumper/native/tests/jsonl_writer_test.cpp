#include <sstream>
#include <stdexcept>
#include <string>
#include <limits>

#include "jsonl_writer.h"

namespace {

void require(bool condition, const char* message) {
  if (!condition) {
    throw std::runtime_error(message);
  }
}

std::string writeFixtureRecord() {
  mmd_oracle::OracleRecord record;
  record.project = "fixtures/sample-basic/scene.pmm";
  record.frame = 30.0;
  record.camera.available = true;
  record.camera.distance = -45.0f;
  record.camera.position = {1.0f, 2.0f, 3.0f};
  record.camera.rotation = {0.1f, 0.2f, 0.3f};

  mmd_oracle::CameraKeyframeSnapshot cameraKeyframe;
  cameraKeyframe.index = 0;
  cameraKeyframe.frame = 30;
  cameraKeyframe.previousKeyframeIndex = 0;
  cameraKeyframe.nextKeyframeIndex = 0;
  cameraKeyframe.distance = -45.0f;
  cameraKeyframe.position = {1.0f, 2.0f, 3.0f};
  cameraKeyframe.rotation = {0.1f, 0.2f, 0.3f};
  cameraKeyframe.fov = 30;
  cameraKeyframe.perspective = true;
  cameraKeyframe.selected = false;
  cameraKeyframe.followModelIndex = -1;
  cameraKeyframe.followBoneIndex = 0;
  cameraKeyframe.interpolation.x = {20, 20, 107, 107};
  cameraKeyframe.interpolation.y = {20, 20, 107, 107};
  cameraKeyframe.interpolation.z = {20, 20, 107, 107};
  cameraKeyframe.interpolation.rotation = {20, 20, 107, 107};
  cameraKeyframe.interpolation.distance = {20, 20, 107, 107};
  cameraKeyframe.interpolation.fov = {20, 20, 107, 107};
  record.camera.keyframes.push_back(cameraKeyframe);

  record.sceneParameters.available = true;
  record.sceneParameters.outputWidth = 1024;
  record.sceneParameters.outputHeight = 768;
  record.sceneParameters.pmmPath = "fixtures/sample-basic/scene.pmm";

  mmd_oracle::ModelSnapshot model;
  model.index = 0;
  model.name = "fake-model";
  model.filename = "fake-model.pmd";
  model.visible = true;

  mmd_oracle::BoneSnapshot bone;
  bone.index = 0;
  bone.name = "center";
  bone.worldMatrix = {1.0f, 0.0f, 0.0f, 0.0f,
                      0.0f, 1.0f, 0.0f, 0.0f,
                      0.0f, 0.0f, 1.0f, 0.0f,
                      1.0f, 2.0f, 3.0f, 1.0f};
  model.bones.push_back(bone);

  mmd_oracle::MorphSnapshot morph;
  morph.index = 0;
  morph.name = "blink";
  morph.weight = 0.5f;
  model.morphs.push_back(morph);

  record.models.push_back(model);

  std::ostringstream out;
  mmd_oracle::writeOracleRecordJsonl(out, record);
  return out.str();
}

void testWritesExpectedShape() {
  const std::string output = writeFixtureRecord();
  require(output.find("\"schemaVersion\":1") != std::string::npos, "missing schemaVersion");
  require(output.find("\"frame\":30") != std::string::npos, "missing frame");
  require(output.find("\"camera\":{\"available\":true") != std::string::npos, "missing camera");
  require(output.find("\"followModelIndex\":-1") != std::string::npos, "missing camera follow model");
  require(output.find("\"sceneParameters\":{\"available\":true,\"outputWidth\":1024,\"outputHeight\":768") != std::string::npos, "missing scene parameters");
  require(output.find("\"filename\":\"fake-model.pmd\"") != std::string::npos, "missing model filename");
  require(output.find("\"worldMatrix\":[1,0,0,0,0,1,0,0,0,0,1,0,1,2,3,1]") != std::string::npos, "missing matrix");
  require(output.find("\"weight\":0.5") != std::string::npos, "missing morph weight");
  require(output.back() == '\n', "JSONL record must end with newline");
}

void testRejectsNonFiniteNumbers() {
  mmd_oracle::OracleRecord record;
  record.frame = 0.0;

  mmd_oracle::ModelSnapshot model;
  model.index = 0;
  model.visible = true;

  mmd_oracle::BoneSnapshot bone;
  bone.index = 0;
  bone.worldMatrix = {1.0f, 0.0f, 0.0f, 0.0f,
                      0.0f, 1.0f, 0.0f, 0.0f,
                      0.0f, 0.0f, 1.0f, 0.0f,
                      0.0f, 0.0f, 0.0f, std::numeric_limits<float>::quiet_NaN()};
  model.bones.push_back(bone);
  record.models.push_back(model);

  std::ostringstream out;
  bool threw = false;
  try {
    mmd_oracle::writeOracleRecordJsonl(out, record);
  } catch (const std::runtime_error&) {
    threw = true;
  }
  require(threw, "non-finite matrix value should throw");
}

} // namespace

int main() {
  testWritesExpectedShape();
  testRejectsNonFiniteNumbers();
  return 0;
}
