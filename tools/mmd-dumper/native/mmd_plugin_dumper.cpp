#include <windows.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <filesystem>
#include <fstream>
#include <mutex>
#include <string>
#include <vector>

#include "mmd_plugin.h"

extern "C" __declspec(dllexport) int MmdOracleDumpFrameChanged();
extern "C" float ExpGetFrameTime();

namespace {

std::mutex g_logMutex;
std::mutex g_captureMutex;
std::mutex g_d3dStateMutex;
std::string g_captureDir;
double g_lastCapturedFrame = std::numeric_limits<double>::quiet_NaN();
int g_captureEveryNFrames = 1;
int g_captureMaxFrame = -1;
bool g_captureConfigLoaded = false;
std::string g_d3dStatePath;
bool g_d3dStateConfigLoaded = false;
double g_lastD3dStateFrame = std::numeric_limits<double>::quiet_NaN();
bool g_hasViewTransform = false;
bool g_hasProjectionTransform = false;
bool g_hasLight0 = false;
D3DMATRIX g_lastViewTransform = {};
D3DMATRIX g_lastProjectionTransform = {};
D3DLIGHT9 g_lastLight0 = {};

std::string readEnvironmentString(const char* name) {
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

void logEvent(const char* event) {
  const std::string logPath = readEnvironmentString("MMD_ORACLE_PROXY_LOG_PATH");
  if (logPath.empty()) {
    return;
  }
  std::lock_guard<std::mutex> lock(g_logMutex);
  std::ofstream out(logPath, std::ios::app);
  if (out) {
    out << event << '\n';
  }
}

int readEnvironmentInt(const char* name, int fallback) {
  const std::string value = readEnvironmentString(name);
  if (value.empty()) {
    return fallback;
  }
  const int parsed = std::atoi(value.c_str());
  return parsed;
}

void loadCaptureConfig() {
  if (g_captureConfigLoaded) {
    return;
  }
  g_captureConfigLoaded = true;
  if (GetEnvironmentVariableA("MMD_ORACLE_CAPTURE_ON_MMDPLUGIN", nullptr, 0) == 0) {
    return;
  }
  g_captureDir = readEnvironmentString("MMD_ORACLE_CAPTURE_DIR");
  if (g_captureDir.empty()) {
    logEvent("mmdplugin:capture:no-dir");
    return;
  }
  g_captureEveryNFrames = std::max(1, readEnvironmentInt("MMD_ORACLE_CAPTURE_EVERY_N_FRAMES", 1));
  g_captureMaxFrame = readEnvironmentInt("MMD_ORACLE_CAPTURE_MAX_FRAME", -1);
  CreateDirectoryA(g_captureDir.c_str(), nullptr);
  logEvent("mmdplugin:capture:configured");
}

void loadD3dStateConfig() {
  if (g_d3dStateConfigLoaded) {
    return;
  }
  g_d3dStateConfigLoaded = true;
  g_d3dStatePath = readEnvironmentString("MMD_ORACLE_D3D_STATE_PATH");
  if (!g_d3dStatePath.empty()) {
    logEvent("mmdplugin:d3d-state:configured");
  }
}

double currentMmdFrame() {
  return static_cast<double>(ExpGetFrameTime()) * 30.0;
}

bool shouldCaptureFrame(double frame) {
  if (g_captureDir.empty() || !std::isfinite(frame)) {
    return false;
  }
  const int roundedFrame = static_cast<int>(std::lround(frame));
  if (roundedFrame < 0) {
    return false;
  }
  if (g_captureMaxFrame >= 0 && roundedFrame > g_captureMaxFrame) {
    return false;
  }
  if (roundedFrame % g_captureEveryNFrames != 0) {
    return false;
  }
  if (std::isfinite(g_lastCapturedFrame) && std::abs(frame - g_lastCapturedFrame) < 0.0001) {
    return false;
  }
  return true;
}

std::filesystem::path capturePathForFrame(double frame) {
  const int roundedFrame = static_cast<int>(std::lround(frame));
  char filename[96] = {};
  std::snprintf(filename, sizeof(filename), "frame_%06d.bmp", roundedFrame);
  return std::filesystem::path(g_captureDir) / filename;
}

void writeJsonMatrix(std::ostream& out, const D3DMATRIX& matrix) {
  const float values[] = {
      matrix._11, matrix._12, matrix._13, matrix._14,
      matrix._21, matrix._22, matrix._23, matrix._24,
      matrix._31, matrix._32, matrix._33, matrix._34,
      matrix._41, matrix._42, matrix._43, matrix._44,
  };
  out << '[';
  for (size_t index = 0; index < std::size(values); ++index) {
    if (index != 0) {
      out << ',';
    }
    out << values[index];
  }
  out << ']';
}

void writeProjectionDerived(std::ostream& out, const D3DMATRIX& matrix) {
  const bool perspective = std::abs(matrix._34) > 0.000001f && std::abs(matrix._44) < 0.000001f;
  out << ",\"derived\":{\"perspective\":" << (perspective ? "true" : "false");
  if (perspective && std::abs(matrix._33) > 0.000001f) {
    out << ",\"nearClip\":" << (-static_cast<double>(matrix._43) / static_cast<double>(matrix._33));
    if (std::abs(1.0f - matrix._33) > 0.000001f) {
      out << ",\"farClip\":" << (static_cast<double>(matrix._43) / (1.0 - static_cast<double>(matrix._33)));
    }
  } else if (!perspective && std::abs(matrix._33) > 0.000001f) {
    out << ",\"nearClip\":" << (-static_cast<double>(matrix._43) / static_cast<double>(matrix._33));
    out << ",\"farClip\":" << ((1.0 - static_cast<double>(matrix._43)) / static_cast<double>(matrix._33));
  }
  if (perspective && std::abs(matrix._11) > 0.000001f && std::abs(matrix._22) > 0.000001f) {
    constexpr double radiansToDegrees = 57.29577951308232;
    const double horizontalFov = 2.0 * std::atan(1.0 / static_cast<double>(matrix._11)) * radiansToDegrees;
    const double verticalFov = 2.0 * std::atan(1.0 / static_cast<double>(matrix._22)) * radiansToDegrees;
    out << ",\"horizontalFov\":" << horizontalFov;
    out << ",\"verticalFov\":" << verticalFov;
  } else if (!perspective && std::abs(matrix._11) > 0.000001f && std::abs(matrix._22) > 0.000001f) {
    out << ",\"orthographicWidth\":" << (2.0 / static_cast<double>(matrix._11));
    out << ",\"orthographicHeight\":" << (2.0 / static_cast<double>(matrix._22));
  }
  out << '}';
}

void writeJsonFloat3(std::ostream& out, float x, float y, float z) {
  out << '[' << x << ',' << y << ',' << z << ']';
}

void writeD3dStateIfRequested() {
  std::lock_guard<std::mutex> lock(g_d3dStateMutex);
  loadD3dStateConfig();
  if (g_d3dStatePath.empty()) {
    return;
  }
  const double frame = currentMmdFrame();
  if (std::isfinite(g_lastD3dStateFrame) && std::abs(frame - g_lastD3dStateFrame) < 0.0001) {
    return;
  }
  std::ofstream out(g_d3dStatePath, std::ios::app);
  if (!out) {
    logEvent("mmdplugin:d3d-state:open:failed");
    return;
  }
  out << "{\"frame\":" << frame;
  out << ",\"view\":{\"available\":" << (g_hasViewTransform ? "true" : "false");
  if (g_hasViewTransform) {
    out << ",\"matrix\":";
    writeJsonMatrix(out, g_lastViewTransform);
  }
  out << "},\"projection\":{\"available\":" << (g_hasProjectionTransform ? "true" : "false");
  if (g_hasProjectionTransform) {
    out << ",\"matrix\":";
    writeJsonMatrix(out, g_lastProjectionTransform);
    writeProjectionDerived(out, g_lastProjectionTransform);
  }
  out << "},\"light0\":{\"available\":" << (g_hasLight0 ? "true" : "false");
  if (g_hasLight0) {
    out << ",\"type\":" << g_lastLight0.Type;
    out << ",\"diffuse\":";
    writeJsonFloat3(out, g_lastLight0.Diffuse.r, g_lastLight0.Diffuse.g, g_lastLight0.Diffuse.b);
    out << ",\"ambient\":";
    writeJsonFloat3(out, g_lastLight0.Ambient.r, g_lastLight0.Ambient.g, g_lastLight0.Ambient.b);
    out << ",\"specular\":";
    writeJsonFloat3(out, g_lastLight0.Specular.r, g_lastLight0.Specular.g, g_lastLight0.Specular.b);
    out << ",\"position\":";
    writeJsonFloat3(out, g_lastLight0.Position.x, g_lastLight0.Position.y, g_lastLight0.Position.z);
    out << ",\"direction\":";
    writeJsonFloat3(out, g_lastLight0.Direction.x, g_lastLight0.Direction.y, g_lastLight0.Direction.z);
  }
  out << "}}\n";
  g_lastD3dStateFrame = frame;
}

bool writeBmp32(const std::filesystem::path& path, const D3DSURFACE_DESC& desc, const D3DLOCKED_RECT& locked) {
  const int32_t width = static_cast<int32_t>(desc.Width);
  const int32_t height = static_cast<int32_t>(desc.Height);
  const uint32_t rowBytes = static_cast<uint32_t>(width * 4);
  const uint32_t pixelBytes = rowBytes * static_cast<uint32_t>(height);
  const uint32_t fileHeaderSize = 14;
  const uint32_t infoHeaderSize = 40;
  const uint32_t pixelOffset = fileHeaderSize + infoHeaderSize;
  const uint32_t fileSize = pixelOffset + pixelBytes;

  std::ofstream out(path, std::ios::binary);
  if (!out) {
    return false;
  }

  const auto writeU16 = [&out](uint16_t value) {
    out.put(static_cast<char>(value & 0xff));
    out.put(static_cast<char>((value >> 8) & 0xff));
  };
  const auto writeU32 = [&out](uint32_t value) {
    out.put(static_cast<char>(value & 0xff));
    out.put(static_cast<char>((value >> 8) & 0xff));
    out.put(static_cast<char>((value >> 16) & 0xff));
    out.put(static_cast<char>((value >> 24) & 0xff));
  };
  const auto writeI32 = [&writeU32](int32_t value) {
    writeU32(static_cast<uint32_t>(value));
  };

  writeU16(0x4d42);
  writeU32(fileSize);
  writeU16(0);
  writeU16(0);
  writeU32(pixelOffset);
  writeU32(infoHeaderSize);
  writeI32(width);
  writeI32(-height);
  writeU16(1);
  writeU16(32);
  writeU32(0);
  writeU32(pixelBytes);
  writeI32(2835);
  writeI32(2835);
  writeU32(0);
  writeU32(0);

  const auto* source = static_cast<const unsigned char*>(locked.pBits);
  unsigned char pixel[4] = {};
  for (int32_t y = 0; y < height; ++y) {
    const auto* row = source + static_cast<size_t>(y) * locked.Pitch;
    for (int32_t x = 0; x < width; ++x) {
      pixel[0] = row[static_cast<size_t>(x) * 4];
      pixel[1] = row[static_cast<size_t>(x) * 4 + 1];
      pixel[2] = row[static_cast<size_t>(x) * 4 + 2];
      pixel[3] = 0xff;
      out.write(reinterpret_cast<const char*>(pixel), sizeof(pixel));
    }
  }
  return static_cast<bool>(out);
}

void captureBackbufferIfRequested(IDirect3DDevice9* device) {
  if (device == nullptr) {
    return;
  }
  std::lock_guard<std::mutex> lock(g_captureMutex);
  loadCaptureConfig();
  const double frame = currentMmdFrame();
  if (!shouldCaptureFrame(frame)) {
    return;
  }

  IDirect3DSurface9* backbuffer = nullptr;
  HRESULT hr = device->GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO, &backbuffer);
  if (FAILED(hr) || backbuffer == nullptr) {
    logEvent("mmdplugin:capture:GetBackBuffer:failed");
    return;
  }

  D3DSURFACE_DESC desc = {};
  hr = backbuffer->GetDesc(&desc);
  if (FAILED(hr)) {
    backbuffer->Release();
    logEvent("mmdplugin:capture:GetDesc:failed");
    return;
  }
  if (desc.Format != D3DFMT_A8R8G8B8 && desc.Format != D3DFMT_X8R8G8B8) {
    backbuffer->Release();
    logEvent("mmdplugin:capture:unsupported-format");
    return;
  }

  IDirect3DSurface9* renderTargetCopy = nullptr;
  hr = device->CreateRenderTarget(desc.Width, desc.Height, desc.Format, D3DMULTISAMPLE_NONE, 0, FALSE, &renderTargetCopy, nullptr);
  if (FAILED(hr) || renderTargetCopy == nullptr) {
    backbuffer->Release();
    logEvent("mmdplugin:capture:CreateRenderTarget:failed");
    return;
  }

  hr = device->StretchRect(backbuffer, nullptr, renderTargetCopy, nullptr, D3DTEXF_NONE);
  backbuffer->Release();
  if (FAILED(hr)) {
    renderTargetCopy->Release();
    logEvent("mmdplugin:capture:StretchRect:failed");
    return;
  }

  IDirect3DSurface9* systemSurface = nullptr;
  hr = device->CreateOffscreenPlainSurface(desc.Width, desc.Height, desc.Format, D3DPOOL_SYSTEMMEM, &systemSurface, nullptr);
  if (FAILED(hr) || systemSurface == nullptr) {
    renderTargetCopy->Release();
    logEvent("mmdplugin:capture:CreateOffscreenPlainSurface:failed");
    return;
  }

  hr = device->GetRenderTargetData(renderTargetCopy, systemSurface);
  renderTargetCopy->Release();
  if (FAILED(hr)) {
    systemSurface->Release();
    logEvent("mmdplugin:capture:GetRenderTargetData:failed");
    return;
  }

  D3DLOCKED_RECT locked = {};
  hr = systemSurface->LockRect(&locked, nullptr, D3DLOCK_READONLY);
  if (FAILED(hr)) {
    systemSurface->Release();
    logEvent("mmdplugin:capture:LockRect:failed");
    return;
  }
  const std::filesystem::path path = capturePathForFrame(frame);
  const bool ok = writeBmp32(path, desc, locked);
  systemSurface->UnlockRect();
  systemSurface->Release();
  if (ok) {
    g_lastCapturedFrame = frame;
  }
  logEvent(ok ? "mmdplugin:capture:write:ok" : "mmdplugin:capture:write:failed");
}

void dumpFrameChangedIfEnabled() {
  if (GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_MMDPLUGIN", nullptr, 0) == 0) {
    return;
  }
  const int result = MmdOracleDumpFrameChanged();
  if (result != 0) {
    logEvent(result == 1 ? "mmdplugin:dump-frame-changed:no-record" : "mmdplugin:dump-frame-changed:error");
  }
}

class MmdOraclePlugin final : public MMDPluginDLL3 {
public:
  explicit MmdOraclePlugin(IDirect3DDevice9* device) : device_(device) {
    if (device_ != nullptr) {
      device_->AddRef();
    }
    logEvent("mmdplugin:create3");
  }

  ~MmdOraclePlugin() override {
    if (device_ != nullptr) {
      device_->Release();
      device_ = nullptr;
    }
    logEvent("mmdplugin:destroy3");
  }

  const char* getPluginTitle() const override {
    return "MMDDumper Oracle Plugin";
  }

  void PostEndScene(HRESULT&) override {
    logEvent("mmdplugin:PostEndScene");
    writeD3dStateIfRequested();
    dumpFrameChangedIfEnabled();
  }

  void PostPresent(CONST RECT*, CONST RECT*, HWND, CONST RGNDATA*, HRESULT&) override {
    logEvent("mmdplugin:PostPresent");
    captureBackbufferIfRequested(device_);
    dumpFrameChangedIfEnabled();
  }

  void PostSetTransform(D3DTRANSFORMSTATETYPE state, CONST D3DMATRIX* matrix, HRESULT&) override {
    if (matrix == nullptr) {
      return;
    }
    std::lock_guard<std::mutex> lock(g_d3dStateMutex);
    if (state == D3DTS_VIEW) {
      g_lastViewTransform = *matrix;
      g_hasViewTransform = true;
    } else if (state == D3DTS_PROJECTION) {
      g_lastProjectionTransform = *matrix;
      g_hasProjectionTransform = true;
    }
  }

  void PostSetLight(DWORD index, CONST D3DLIGHT9* light, HRESULT&) override {
    if (index != 0 || light == nullptr) {
      return;
    }
    std::lock_guard<std::mutex> lock(g_d3dStateMutex);
    g_lastLight0 = *light;
    g_hasLight0 = true;
  }

private:
  IDirect3DDevice9* device_ = nullptr;
};

} // namespace

extern "C" MMD_PLUGIN_API int version() {
  logEvent("mmdplugin:version");
  return 3;
}

extern "C" MMD_PLUGIN_API MMDPluginDLL1* create1(IDirect3DDevice9*) {
  return nullptr;
}

extern "C" MMD_PLUGIN_API void destroy1(MMDPluginDLL1*) {}

extern "C" MMD_PLUGIN_API MMDPluginDLL2* create2(IDirect3DDevice9*) {
  return nullptr;
}

extern "C" MMD_PLUGIN_API void destroy2(MMDPluginDLL2*) {}

extern "C" MMD_PLUGIN_API MMDPluginDLL3* create3(IDirect3DDevice9* device) {
  return new MmdOraclePlugin(device);
}

extern "C" MMD_PLUGIN_API void destroy3(MMDPluginDLL3* plugin) {
  delete plugin;
}

extern "C" MMD_PLUGIN_API MMDPluginDLL4* create4(IDirect3DDevice9*) {
  return nullptr;
}

extern "C" MMD_PLUGIN_API void destroy4(MMDPluginDLL4*) {}

extern "C" MMD_PLUGIN_API MMDPluginDLL3** createArray3(IDirect3DDevice9*, int* outArraySize) {
  if (outArraySize != nullptr) {
    *outArraySize = 0;
  }
  return nullptr;
}

extern "C" MMD_PLUGIN_API void destroyArray3(MMDPluginDLL3**) {}

extern "C" MMD_PLUGIN_API MMDPluginDLL4** createArray4(IDirect3DDevice9*, int* outArraySize) {
  if (outArraySize != nullptr) {
    *outArraySize = 0;
  }
  return nullptr;
}

extern "C" MMD_PLUGIN_API void destroyArray4(MMDPluginDLL4**) {}
