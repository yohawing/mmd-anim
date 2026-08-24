#include <windows.h>
#include <d3d9.h>

#include <algorithm>
#include <cmath>
#include <fstream>
#include <filesystem>
#include <mutex>
#include <set>
#include <sstream>
#include <string>
#include <vector>

namespace {

using Direct3DCreate9Fn = IDirect3D9*(WINAPI*)(UINT);
using CreateDeviceFn = HRESULT(WINAPI*)(
    IDirect3D9*,
    UINT,
    D3DDEVTYPE,
    HWND,
    DWORD,
    D3DPRESENT_PARAMETERS*,
    IDirect3DDevice9**);
using PresentFn = HRESULT(WINAPI*)(IDirect3DDevice9*, const RECT*, const RECT*, HWND, const RGNDATA*);
using EndSceneFn = HRESULT(WINAPI*)(IDirect3DDevice9*);
using MmdOracleDumpFrameChangedFn = int(*)();
using ExpGetFrameTimeFn = float(*)();

HMODULE g_realD3d9 = nullptr;
HMODULE g_oracleDumper = nullptr;
Direct3DCreate9Fn g_realDirect3DCreate9 = nullptr;
CreateDeviceFn g_realCreateDevice = nullptr;
PresentFn g_realPresent = nullptr;
EndSceneFn g_realEndScene = nullptr;
std::once_flag g_loadOnce;
std::once_flag g_dumperOnce;
std::once_flag g_createDevicePatchOnce;
std::once_flag g_devicePatchOnce;
std::once_flag g_samplerOnce;
std::once_flag g_captureConfigOnce;
wchar_t g_proxyPath[MAX_PATH] = {};
std::mutex g_logMutex;
std::mutex g_captureMutex;
std::vector<int> g_captureFrames;
std::set<int> g_capturedFrames;
std::string g_captureDir;
ExpGetFrameTimeFn g_expGetFrameTime = nullptr;
IDirect3DDevice9* g_device = nullptr;

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

void logEvent(const std::string& event) {
  logEvent(event.c_str());
}

void captureBackbufferIfRequested(IDirect3DDevice9* device);

void loadRealD3d9() {
  logEvent("d3d9:load-real:start");
  wchar_t systemDirectory[MAX_PATH] = {};
  const UINT systemLength = GetSystemDirectoryW(systemDirectory, MAX_PATH);
  if (systemLength == 0 || systemLength >= MAX_PATH) {
    logEvent("d3d9:load-real:no-system-dir");
    return;
  }

  const std::filesystem::path realPath = std::filesystem::path(systemDirectory) / L"d3d9.dll";
  g_realD3d9 = LoadLibraryW(realPath.c_str());
  if (g_realD3d9 != nullptr) {
    g_realDirect3DCreate9 = reinterpret_cast<Direct3DCreate9Fn>(GetProcAddress(g_realD3d9, "Direct3DCreate9"));
    logEvent(g_realDirect3DCreate9 == nullptr ? "d3d9:load-real:no-Direct3DCreate9" : "d3d9:load-real:ok");
  } else {
    logEvent("d3d9:load-real:failed");
  }
}

void loadOracleDumper() {
  logEvent("d3d9:load-dumper:start");
  if (g_proxyPath[0] == L'\0') {
    logEvent("d3d9:load-dumper:no-proxy-path");
    return;
  }
  const std::filesystem::path dumperPath = std::filesystem::path(g_proxyPath).parent_path() / L"mmd_oracle_dumper.dll";
  g_oracleDumper = LoadLibraryW(dumperPath.c_str());
  logEvent(g_oracleDumper == nullptr ? "d3d9:load-dumper:failed" : "d3d9:load-dumper:ok");
}

std::vector<int> parseCaptureFrames(const std::string& value) {
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

void loadCaptureConfig() {
  g_captureDir = readEnvironmentString("MMD_ORACLE_CAPTURE_DIR");
  g_captureFrames = parseCaptureFrames(readEnvironmentString("MMD_ORACLE_CAPTURE_FRAMES"));
  if (g_captureDir.empty() || g_captureFrames.empty()) {
    return;
  }
  CreateDirectoryA(g_captureDir.c_str(), nullptr);
  const HMODULE mainModule = GetModuleHandleW(nullptr);
  if (mainModule != nullptr) {
    g_expGetFrameTime = reinterpret_cast<ExpGetFrameTimeFn>(GetProcAddress(mainModule, "ExpGetFrameTime"));
  }
  logEvent(g_expGetFrameTime == nullptr ? "d3d9:capture:no-frame-export" : "d3d9:capture:configured");
}

int currentMmdFrame() {
  std::call_once(g_captureConfigOnce, loadCaptureConfig);
  if (g_expGetFrameTime == nullptr) {
    return -1;
  }
  return static_cast<int>(std::lround(static_cast<double>(g_expGetFrameTime()) * 30.0));
}

bool shouldCaptureFrame(int frame) {
  if (frame < 0 || g_captureDir.empty() || g_captureFrames.empty()) {
    return false;
  }
  if (!std::binary_search(g_captureFrames.begin(), g_captureFrames.end(), frame)) {
    return false;
  }
  return g_capturedFrames.find(frame) == g_capturedFrames.end();
}

std::filesystem::path capturePathForFrame(int frame) {
  char filename[64] = {};
  std::snprintf(filename, sizeof(filename), "frame_%06d.bmp", frame);
  return std::filesystem::path(g_captureDir) / filename;
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
  const int frame = currentMmdFrame();
  if (!shouldCaptureFrame(frame)) {
    return;
  }

  IDirect3DSurface9* backbuffer = nullptr;
  HRESULT hr = device->GetBackBuffer(0, 0, D3DBACKBUFFER_TYPE_MONO, &backbuffer);
  if (FAILED(hr) || backbuffer == nullptr) {
    logEvent("d3d9:capture:GetBackBuffer:failed");
    return;
  }

  D3DSURFACE_DESC desc = {};
  hr = backbuffer->GetDesc(&desc);
  if (FAILED(hr)) {
    backbuffer->Release();
    logEvent("d3d9:capture:GetDesc:failed");
    return;
  }
  if (desc.Format != D3DFMT_A8R8G8B8 && desc.Format != D3DFMT_X8R8G8B8) {
    backbuffer->Release();
    logEvent("d3d9:capture:unsupported-format");
    return;
  }

  IDirect3DSurface9* renderTargetCopy = nullptr;
  hr = device->CreateRenderTarget(desc.Width, desc.Height, desc.Format, D3DMULTISAMPLE_NONE, 0, FALSE, &renderTargetCopy, nullptr);
  if (FAILED(hr) || renderTargetCopy == nullptr) {
    backbuffer->Release();
    logEvent("d3d9:capture:CreateRenderTarget:failed");
    return;
  }

  hr = device->StretchRect(backbuffer, nullptr, renderTargetCopy, nullptr, D3DTEXF_NONE);
  backbuffer->Release();
  if (FAILED(hr)) {
    renderTargetCopy->Release();
    logEvent("d3d9:capture:StretchRect:failed");
    return;
  }

  IDirect3DSurface9* systemSurface = nullptr;
  hr = device->CreateOffscreenPlainSurface(desc.Width, desc.Height, desc.Format, D3DPOOL_SYSTEMMEM, &systemSurface, nullptr);
  if (FAILED(hr) || systemSurface == nullptr) {
    renderTargetCopy->Release();
    logEvent("d3d9:capture:CreateOffscreenPlainSurface:failed");
    return;
  }

  hr = device->GetRenderTargetData(renderTargetCopy, systemSurface);
  renderTargetCopy->Release();
  if (FAILED(hr)) {
    systemSurface->Release();
    logEvent("d3d9:capture:GetRenderTargetData:failed");
    return;
  }

  D3DLOCKED_RECT locked = {};
  hr = systemSurface->LockRect(&locked, nullptr, D3DLOCK_READONLY);
  if (FAILED(hr)) {
    systemSurface->Release();
    logEvent("d3d9:capture:LockRect:failed");
    return;
  }
  const std::filesystem::path path = capturePathForFrame(frame);
  const bool ok = writeBmp32(path, desc, locked);
  systemSurface->UnlockRect();
  systemSurface->Release();
  if (ok) {
    g_capturedFrames.insert(frame);
  }
  logEvent(ok ? "d3d9:capture:write:ok" : "d3d9:capture:write:failed");
}

template <typename T>
void patchVtableSlot(void** vtable, size_t index, T replacement, T& original) {
  DWORD oldProtect = 0;
  if (!VirtualProtect(&vtable[index], sizeof(void*), PAGE_EXECUTE_READWRITE, &oldProtect)) {
    logEvent("d3d9:patch:VirtualProtect-failed");
    return;
  }
  original = reinterpret_cast<T>(vtable[index]);
  vtable[index] = reinterpret_cast<void*>(replacement);
  DWORD ignored = 0;
  VirtualProtect(&vtable[index], sizeof(void*), oldProtect, &ignored);
}

void dumpFrameChanged() {
  if (GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_D3D9", nullptr, 0) == 0) {
    return;
  }
  logEvent("d3d9:dump-frame-changed");
  std::call_once(g_dumperOnce, loadOracleDumper);
  if (g_oracleDumper == nullptr) {
    return;
  }
  const auto dumpFrameChanged = reinterpret_cast<MmdOracleDumpFrameChangedFn>(GetProcAddress(g_oracleDumper, "MmdOracleDumpFrameChanged"));
  if (dumpFrameChanged != nullptr) {
    dumpFrameChanged();
  } else {
    logEvent("d3d9:dump-frame-changed:no-export");
  }
}

DWORD readSamplerDurationMs() {
  const std::string value = readEnvironmentString("MMD_ORACLE_D3D9_SAMPLER_MS");
  if (value.empty()) {
    return 20000;
  }
  const int parsed = std::atoi(value.c_str());
  return parsed <= 0 ? 20000 : static_cast<DWORD>(parsed);
}

DWORD readSamplerIntervalMs() {
  const std::string value = readEnvironmentString("MMD_ORACLE_D3D9_SAMPLER_INTERVAL_MS");
  if (value.empty()) {
    return 250;
  }
  const int parsed = std::atoi(value.c_str());
  return parsed <= 0 ? 250 : static_cast<DWORD>(parsed);
}

DWORD WINAPI d3d9SamplerThread(LPVOID) {
  logEvent("d3d9:sampler:start");
  const DWORD durationMs = readSamplerDurationMs();
  const DWORD intervalMs = readSamplerIntervalMs();
  const DWORD startedAt = GetTickCount();
  while (GetTickCount() - startedAt < durationMs) {
    dumpFrameChanged();
    captureBackbufferIfRequested(g_device);
    Sleep(intervalMs);
  }
  logEvent("d3d9:sampler:stop");
  return 0;
}

void startSamplerOnce() {
  if (GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_D3D9", nullptr, 0) == 0) {
    return;
  }
  std::call_once(g_samplerOnce, []() {
    HANDLE thread = CreateThread(nullptr, 0, d3d9SamplerThread, nullptr, 0, nullptr);
    if (thread != nullptr) {
      CloseHandle(thread);
    } else {
      logEvent("d3d9:sampler:create-thread-failed");
    }
  });
}

HRESULT WINAPI hookedPresent(IDirect3DDevice9* device, const RECT* sourceRect, const RECT* destRect, HWND destWindowOverride, const RGNDATA* dirtyRegion) {
  logEvent("d3d9:Present");
  captureBackbufferIfRequested(device);
  const HRESULT result = g_realPresent == nullptr ? D3DERR_INVALIDCALL : g_realPresent(device, sourceRect, destRect, destWindowOverride, dirtyRegion);
  dumpFrameChanged();
  return result;
}

HRESULT WINAPI hookedEndScene(IDirect3DDevice9* device) {
  logEvent("d3d9:EndScene");
  captureBackbufferIfRequested(device);
  const HRESULT result = g_realEndScene == nullptr ? D3DERR_INVALIDCALL : g_realEndScene(device);
  dumpFrameChanged();
  return result;
}

void patchDevice(IDirect3DDevice9* device) {
  if (device == nullptr) {
    return;
  }
  void** vtable = *reinterpret_cast<void***>(device);
  std::call_once(g_devicePatchOnce, [device, vtable]() {
    logEvent("d3d9:patch-device");
    g_device = device;
    g_device->AddRef();
    patchVtableSlot(vtable, 17, hookedPresent, g_realPresent);
    patchVtableSlot(vtable, 42, hookedEndScene, g_realEndScene);
  });
}

HRESULT WINAPI hookedCreateDevice(
    IDirect3D9* self,
    UINT adapter,
    D3DDEVTYPE deviceType,
    HWND focusWindow,
    DWORD behaviorFlags,
    D3DPRESENT_PARAMETERS* presentationParameters,
    IDirect3DDevice9** returnedDeviceInterface) {
  const HRESULT result = g_realCreateDevice == nullptr
      ? D3DERR_INVALIDCALL
      : g_realCreateDevice(self, adapter, deviceType, focusWindow, behaviorFlags, presentationParameters, returnedDeviceInterface);
  logEvent(SUCCEEDED(result) ? "d3d9:CreateDevice:ok" : "d3d9:CreateDevice:failed");
  if (SUCCEEDED(result) && returnedDeviceInterface != nullptr) {
    patchDevice(*returnedDeviceInterface);
    startSamplerOnce();
  }
  return result;
}

void patchCreateDevice(IDirect3D9* direct3d) {
  if (direct3d == nullptr) {
    return;
  }
  void** vtable = *reinterpret_cast<void***>(direct3d);
  std::call_once(g_createDevicePatchOnce, [vtable]() {
    logEvent("d3d9:patch-CreateDevice");
    patchVtableSlot(vtable, 16, hookedCreateDevice, g_realCreateDevice);
  });
}

} // namespace

extern "C" IDirect3D9* WINAPI ProxyDirect3DCreate9(UINT sdkVersion) {
  logEvent("d3d9:Direct3DCreate9");
  std::call_once(g_loadOnce, loadRealD3d9);
  if (g_realDirect3DCreate9 == nullptr) {
    return nullptr;
  }

  IDirect3D9* direct3d = g_realDirect3DCreate9(sdkVersion);
  logEvent(direct3d == nullptr ? "d3d9:Direct3DCreate9:null" : "d3d9:Direct3DCreate9:ok");
  patchCreateDevice(direct3d);
  return direct3d;
}

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(module);
    logEvent("d3d9:DllMain:attach");
    const DWORD proxyLength = GetModuleFileNameW(module, g_proxyPath, MAX_PATH);
    if (proxyLength == 0 || proxyLength >= MAX_PATH) {
      g_proxyPath[0] = L'\0';
    }
  } else if (reason == DLL_PROCESS_DETACH) {
    if (g_oracleDumper != nullptr) {
      FreeLibrary(g_oracleDumper);
      g_oracleDumper = nullptr;
    }
    if (g_realD3d9 != nullptr) {
      FreeLibrary(g_realD3d9);
      g_realD3d9 = nullptr;
    }
    if (g_device != nullptr) {
      g_device->Release();
      g_device = nullptr;
    }
  }
  return TRUE;
}
