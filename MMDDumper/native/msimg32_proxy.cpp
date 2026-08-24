#include <windows.h>

#include <cstdlib>
#include <filesystem>
#include <mutex>

namespace {

HMODULE g_realMsimg32 = nullptr;
HMODULE g_oracleDumper = nullptr;
std::once_flag g_loadOnce;
wchar_t g_proxyPath[MAX_PATH] = {};

using AlphaBlendFn = BOOL(WINAPI*)(HDC, int, int, int, int, HDC, int, int, int, int, BLENDFUNCTION);
using TransparentBltFn = BOOL(WINAPI*)(HDC, int, int, int, int, HDC, int, int, int, int, UINT);
using GradientFillFn = BOOL(WINAPI*)(HDC, PTRIVERTEX, ULONG, PVOID, ULONG, ULONG);
using MmdOracleDumpOnceFn = int(*)();
using MmdOracleDumpFrameChangedFn = int(*)();

void loadProxyTargets() {
  wchar_t systemDirectory[MAX_PATH] = {};
  const UINT systemLength = GetSystemDirectoryW(systemDirectory, MAX_PATH);
  if (systemLength == 0 || systemLength >= MAX_PATH) {
    return;
  }

  const std::filesystem::path realPath = std::filesystem::path(systemDirectory) / L"msimg32.dll";
  g_realMsimg32 = LoadLibraryW(realPath.c_str());

  if (g_proxyPath[0] == L'\0') {
    return;
  }

  const std::filesystem::path dumperPath = std::filesystem::path(g_proxyPath).parent_path() / L"mmd_oracle_dumper.dll";
  g_oracleDumper = LoadLibraryW(dumperPath.c_str());
  if (g_oracleDumper != nullptr && GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_PROXY_LOAD", nullptr, 0) > 0) {
    const auto dumpOnce = reinterpret_cast<MmdOracleDumpOnceFn>(GetProcAddress(g_oracleDumper, "MmdOracleDumpOnce"));
    if (dumpOnce != nullptr) {
      dumpOnce();
    }
  }
}

template <typename T>
T getRealProc(const char* name) {
  std::call_once(g_loadOnce, loadProxyTargets);
  if (g_realMsimg32 == nullptr) {
    return nullptr;
  }
  return reinterpret_cast<T>(GetProcAddress(g_realMsimg32, name));
}

DWORD readProxyLoadDelayMs() {
  char buffer[32] = {};
  const DWORD length = GetEnvironmentVariableA("MMD_ORACLE_PROXY_LOAD_DELAY_MS", buffer, static_cast<DWORD>(sizeof(buffer)));
  if (length == 0 || length >= sizeof(buffer)) {
    return 2000;
  }
  const int value = std::atoi(buffer);
  return value < 0 ? 2000 : static_cast<DWORD>(value);
}

DWORD WINAPI proxyLoadSmokeThread(LPVOID) {
  Sleep(readProxyLoadDelayMs());
  std::call_once(g_loadOnce, loadProxyTargets);
  return 0;
}

void dumpFrameChangedFromGradientFill() {
  if (GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_GRADIENTFILL", nullptr, 0) == 0) {
    return;
  }
  std::call_once(g_loadOnce, loadProxyTargets);
  if (g_oracleDumper == nullptr) {
    return;
  }
  const auto dumpFrameChanged = reinterpret_cast<MmdOracleDumpFrameChangedFn>(GetProcAddress(g_oracleDumper, "MmdOracleDumpFrameChanged"));
  if (dumpFrameChanged != nullptr) {
    dumpFrameChanged();
  }
}

} // namespace

extern "C" BOOL WINAPI ProxyAlphaBlend(
    HDC destination,
    int destinationX,
    int destinationY,
    int destinationWidth,
    int destinationHeight,
    HDC source,
    int sourceX,
    int sourceY,
    int sourceWidth,
    int sourceHeight,
    BLENDFUNCTION blendFunction) {
  const auto real = getRealProc<AlphaBlendFn>("AlphaBlend");
  return real == nullptr ? FALSE : real(destination, destinationX, destinationY, destinationWidth, destinationHeight, source, sourceX, sourceY, sourceWidth, sourceHeight, blendFunction);
}

extern "C" BOOL WINAPI ProxyTransparentBlt(
    HDC destination,
    int destinationX,
    int destinationY,
    int destinationWidth,
    int destinationHeight,
    HDC source,
    int sourceX,
    int sourceY,
    int sourceWidth,
    int sourceHeight,
    UINT transparentColor) {
  const auto real = getRealProc<TransparentBltFn>("TransparentBlt");
  return real == nullptr ? FALSE : real(destination, destinationX, destinationY, destinationWidth, destinationHeight, source, sourceX, sourceY, sourceWidth, sourceHeight, transparentColor);
}

extern "C" BOOL WINAPI ProxyGradientFill(
    HDC hdc,
    PTRIVERTEX vertex,
    ULONG vertexCount,
    PVOID mesh,
    ULONG meshCount,
    ULONG mode) {
  const auto real = getRealProc<GradientFillFn>("GradientFill");
  const BOOL result = real == nullptr ? FALSE : real(hdc, vertex, vertexCount, mesh, meshCount, mode);
  dumpFrameChangedFromGradientFill();
  return result;
}

BOOL APIENTRY DllMain(HMODULE module, DWORD reason, LPVOID) {
  if (reason == DLL_PROCESS_ATTACH) {
    DisableThreadLibraryCalls(module);
    const DWORD proxyLength = GetModuleFileNameW(module, g_proxyPath, MAX_PATH);
    if (proxyLength == 0 || proxyLength >= MAX_PATH) {
      g_proxyPath[0] = L'\0';
    }
    if (GetEnvironmentVariableA("MMD_ORACLE_DUMP_ON_PROXY_LOAD", nullptr, 0) > 0) {
      HANDLE thread = CreateThread(nullptr, 0, proxyLoadSmokeThread, nullptr, 0, nullptr);
      if (thread != nullptr) {
        CloseHandle(thread);
      }
    }
  } else if (reason == DLL_PROCESS_DETACH) {
    if (g_oracleDumper != nullptr) {
      FreeLibrary(g_oracleDumper);
      g_oracleDumper = nullptr;
    }
    if (g_realMsimg32 != nullptr) {
      FreeLibrary(g_realMsimg32);
      g_realMsimg32 = nullptr;
    }
  }
  return TRUE;
}
