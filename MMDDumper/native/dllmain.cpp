#include <windows.h>

#include <cmath>
#include <cstdlib>
#include <fstream>
#include <algorithm>
#include <limits>
#include <mutex>
#include <sstream>
#include <string>
#include <vector>

#include "jsonl_writer.h"
#include "mmd_export_provider.h"

namespace {

std::mutex g_dumpMutex;
double g_lastDumpedFrame = std::numeric_limits<double>::quiet_NaN();
bool g_sawNonZeroFrame = false;
int g_lastDumpedRoundedFrame = -1;
std::once_flag g_dumpFramesOnce;
std::vector<int> g_dumpFrames;
struct DumpFrameRange {
  bool enabled = false;
  int start = 0;
  int end = 0;
  int step = 1;
};
DumpFrameRange g_dumpFrameRange;

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

std::vector<int> parseFrameList(const std::string& value) {
  std::vector<int> frames;
  std::stringstream stream(value);
  std::string token;
  while (std::getline(stream, token, ',')) {
    if (token.empty()) {
      continue;
    }
    const int frame = std::atoi(token.c_str());
    if (frame >= 0) {
      frames.push_back(frame);
    }
  }
  std::sort(frames.begin(), frames.end());
  frames.erase(std::unique(frames.begin(), frames.end()), frames.end());
  return frames;
}

DumpFrameRange parseFrameRange(const std::string& value) {
  DumpFrameRange range;
  if (value.empty()) {
    return range;
  }
  std::stringstream stream(value);
  std::string start;
  std::string end;
  std::string step;
  if (!std::getline(stream, start, ':') || !std::getline(stream, end, ':') || !std::getline(stream, step, ':')) {
    return range;
  }
  range.start = std::atoi(start.c_str());
  range.end = std::atoi(end.c_str());
  range.step = std::atoi(step.c_str());
  range.enabled = range.start >= 0 && range.end >= range.start && range.step > 0;
  return range;
}

void loadDumpFramesConfig() {
  g_dumpFrames = parseFrameList(environmentString("MMD_ORACLE_DUMP_FRAMES"));
  g_dumpFrameRange = parseFrameRange(environmentString("MMD_ORACLE_DUMP_FRAME_RANGE"));
}

bool shouldDumpRequestedFrame(double frame) {
  std::call_once(g_dumpFramesOnce, loadDumpFramesConfig);
  if (g_dumpFrames.empty() && !g_dumpFrameRange.enabled) {
    return true;
  }
  const int roundedFrame = static_cast<int>(std::lround(frame));
  if (!g_dumpFrames.empty() && std::binary_search(g_dumpFrames.begin(), g_dumpFrames.end(), roundedFrame)) {
    return true;
  }
  if (!g_dumpFrameRange.enabled || roundedFrame < g_dumpFrameRange.start || roundedFrame > g_dumpFrameRange.end) {
    return false;
  }
  return ((roundedFrame - g_dumpFrameRange.start) % g_dumpFrameRange.step) == 0;
}

bool hasReadyModelSnapshot(const mmd_oracle::OracleRecord& record) {
  if (record.models.empty()) {
    return false;
  }
  bool hasNonZeroWorldMatrix = false;
  for (const auto& model : record.models) {
    if (model.bones.empty()) {
      return false;
    }
    for (const auto& bone : model.bones) {
      for (const float value : bone.worldMatrix) {
        if (std::abs(value) > 0.000001f) {
          hasNonZeroWorldMatrix = true;
          break;
        }
      }
      if (hasNonZeroWorldMatrix) {
        break;
      }
    }
  }
  return hasNonZeroWorldMatrix;
}

bool dumpSingleSnapshot(double frame, bool requireModel) {
  const std::string outputPath = environmentString("MMD_ORACLE_DUMP_PATH");
  if (outputPath.empty()) {
    return false;
  }

  mmd_oracle::OracleRecord record = mmd_oracle::captureMmdExportSnapshot(frame, nullptr);
  if (requireModel && !hasReadyModelSnapshot(record)) {
    return false;
  }

  std::ofstream out(outputPath, std::ios::app);
  if (!out) {
    return false;
  }

  mmd_oracle::writeOracleRecordJsonl(out, record);
  return true;
}

bool shouldRequireModelSnapshot() {
  char value[8] = {};
  const DWORD length = GetEnvironmentVariableA("MMD_ORACLE_REQUIRE_MODEL", value, sizeof(value));
  if (length > 0 && length < sizeof(value) && value[0] == '0') {
    return false;
  }
  return true;
}

bool shouldSkipInitialFrameZero(double frame) {
  if (g_sawNonZeroFrame) {
    return false;
  }
  if (std::abs(frame) >= 0.0001) {
    g_sawNonZeroFrame = true;
    return false;
  }
  return GetEnvironmentVariableA("MMD_ORACLE_SKIP_INITIAL_FRAME_ZERO", nullptr, 0) != 0;
}

} // namespace

extern "C" __declspec(dllexport) int MmdOracleDumpOnce() {
  try {
    const double frame = mmd_oracle::getMmdExportFrame();
    return dumpSingleSnapshot(frame, false) ? 0 : 1;
  } catch (...) {
    return 2;
  }
}

extern "C" __declspec(dllexport) int MmdOracleDumpFrameChanged() {
  try {
    std::lock_guard<std::mutex> lock(g_dumpMutex);
    const double frame = mmd_oracle::getMmdExportFrame();
    const int roundedFrame = static_cast<int>(std::lround(frame));
    if (shouldSkipInitialFrameZero(frame)) {
      return 0;
    }
    if ((std::isfinite(g_lastDumpedFrame) && std::abs(frame - g_lastDumpedFrame) < 0.0001) || roundedFrame == g_lastDumpedRoundedFrame) {
      return 0;
    }
    if (!shouldDumpRequestedFrame(frame)) {
      return 0;
    }
    if (!dumpSingleSnapshot(frame, shouldRequireModelSnapshot())) {
      return 1;
    }
    g_lastDumpedFrame = frame;
    g_lastDumpedRoundedFrame = roundedFrame;
    return 0;
  } catch (...) {
    return 2;
  }
}

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(module);
  }
  return TRUE;
}
