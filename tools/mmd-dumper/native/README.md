# Native Dumper Skeleton

このディレクトリは MMD 9.32 x64 の `MMDExport` API を読むための最小 C++ skeleton です。

現段階でやること:

- `ExpGetPmdBoneWorldMat(...)` と `ExpGetPmdMorphValue(...)` 相当の snapshot を JSONL に整形する。
- fake provider で core の出力をテスト可能にする。
- 実 MMD への読み込み方法、hook 位置、frame seek は後続作業に分ける。
- DLL は `MmdOracleDumpOnce` を export し、`DllMain` では thread notification を切るだけにする。
- Optional `MSIMG32.dll` proxy は MMD フォルダ内でだけ使う。system `msimg32.dll` に主要 GDI call を委譲し、同じフォルダの `mmd_oracle_dumper.dll` を読む。

現段階でやらないこと:

- injector
- inline detour
- privilege escalation
- MMD 以外の process attach
- physics dump

## Build

Native smoke:

```powershell
pnpm -C MMDDumper native:test
```

This command auto-detects Visual Studio Build Tools via `vswhere` when `cl.exe` is not already in `PATH`. It builds:

- `out/native-smoke/mmd_oracle_jsonl_writer_test.exe`
- `out/native-dll-smoke/mmd_oracle_dumper.dll` when local `MMDExport.lib` exists
- `out/native-proxy-smoke/MSIMG32.dll`
- `out/native-mmdplugin-smoke/mmd_oracle_plugin.dll` when local `lib/mmdplugin/mmd_plugin.h` exists

DLL:

```powershell
cmake -S MMDDumper/native -B MMDDumper/out/native-dll -DMMD_ORACLE_BUILD_DLL=ON -DMMD_EXPORT_LIB="$PWD/MMDDumper/lib/mmd/MMDExport.lib"
cmake --build MMDDumper/out/native-dll --config Release
```

MMD-local proxy:

```powershell
cmake -S MMDDumper/native -B MMDDumper/out/native-proxy -DMMD_ORACLE_BUILD_MSIMG32_PROXY=ON
cmake --build MMDDumper/out/native-proxy --config Release
```

MMDPlugin adapter:

```powershell
cmake -S MMDDumper/native -B MMDDumper/out/native-mmdplugin -DMMD_ORACLE_BUILD_MMDPLUGIN=ON -DMMD_EXPORT_LIB="$PWD/MMDDumper/lib/mmd/MMDExport.lib" -DMMD_PLUGIN_INCLUDE_DIR="$PWD/MMDDumper/lib/mmdplugin"
cmake --build MMDDumper/out/native-mmdplugin --config Release
```

Install `mmd_oracle_plugin.dll` into the local MMD `Plugin/` folder and set `MMD_ORACLE_DUMP_ON_MMDPLUGIN=1` to dump from MMDPlugin `PostEndScene` / `PostPresent` callbacks.

`MMD_ORACLE_DUMP_ON_PROXY_LOAD=1` asks the proxy to call `MmdOracleDumpOnce` after the dumper DLL is loaded. This is only a first-load smoke path; real frame sampling still needs a later render-timing hook.
