#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const buildDir = resolve(root, "out", "native-smoke");
const dllBuildDir = resolve(root, "out", "native-dll-smoke");
const proxyBuildDir = resolve(root, "out", "native-proxy-smoke");
const d3d9BuildDir = resolve(root, "out", "native-d3d9-smoke");
const pluginBuildDir = resolve(root, "out", "native-mmdplugin-smoke");
const mmdExportLib = resolve(root, "lib", "mmd", "MMDExport.lib");
const mmdPluginIncludeDir = resolve(root, "lib", "mmdplugin");
const vsDevCmd = findVsDevCmd();

const cmakeVersion = spawnSync("cmake", ["--version"], { encoding: "utf8" });
if (cmakeVersion.status !== 0) {
  console.log(JSON.stringify({ ok: true, skipped: true, reason: "cmake not found" }, null, 2));
  process.exit(0);
}

const compiler = findCompiler();
if (!compiler && !vsDevCmd) {
  console.log(JSON.stringify({ ok: true, skipped: true, reason: "no C++ compiler found in PATH" }, null, 2));
  process.exit(0);
}

cmakeConfigure(buildDir, ["-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=OFF"]);
cmakeBuild(buildDir, "mmd_oracle_jsonl_writer_test");
ctest(buildDir);
requireFile(resolve(buildDir, "mmd_oracle_jsonl_writer_test.exe"));

let dllBuilt = false;
if (existsSync(mmdExportLib)) {
  cmakeConfigure(dllBuildDir, ["-DMMD_ORACLE_BUILD_DLL=ON", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=OFF", `-DMMD_EXPORT_LIB=${mmdExportLib}`]);
  cmakeBuild(dllBuildDir, "mmd_oracle_dumper");
  requireFile(resolve(dllBuildDir, "mmd_oracle_dumper.dll"));
  dllBuilt = true;
}

let pluginBuilt = false;
if (existsSync(mmdExportLib) && existsSync(resolve(mmdPluginIncludeDir, "mmd_plugin.h"))) {
  cmakeConfigure(pluginBuildDir, [
    "-DMMD_ORACLE_BUILD_MMDPLUGIN=ON",
    `-DMMD_EXPORT_LIB=${mmdExportLib}`,
    `-DMMD_PLUGIN_INCLUDE_DIR=${mmdPluginIncludeDir}`,
  ]);
  cmakeBuild(pluginBuildDir, "mmd_oracle_plugin");
  requireFile(resolve(pluginBuildDir, "mmd_oracle_plugin.dll"));
  pluginBuilt = true;
}

cmakeConfigure(proxyBuildDir, ["-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=ON"]);
cmakeBuild(proxyBuildDir, "msimg32");
requireFile(resolve(proxyBuildDir, "MSIMG32.dll"));

cmakeConfigure(d3d9BuildDir, ["-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_D3D9_PROXY=ON"]);
cmakeBuild(d3d9BuildDir, "d3d9");
requireFile(resolve(d3d9BuildDir, "d3d9.dll"));

console.log(
  JSON.stringify(
    {
      ok: true,
      buildDir,
      dllBuildDir,
      proxyBuildDir,
      d3d9BuildDir,
      pluginBuildDir,
      dllBuilt,
      dllSkippedReason: dllBuilt ? undefined : "local MMDExport.lib not found",
      pluginBuilt,
      pluginSkippedReason: pluginBuilt ? undefined : "local MMDExport.lib or mmd_plugin.h not found",
      compiler: compiler ?? "Visual Studio Developer Command Prompt",
      compilerCacheExists: existsSync(buildDir),
    },
    null,
    2,
  ),
);

function cmakeConfigure(directory, definitions) {
  run("cmake", ["-S", resolve(root, "native"), "-B", directory, "-G", "Ninja", ...compilerFlag(), ...definitions]);
}

function cmakeBuild(directory, target) {
  run("cmake", ["--build", directory, "--target", target, "--config", "Release"]);
}

function ctest(directory) {
  run("ctest", ["--test-dir", directory, "--output-on-failure", "-C", "Release"]);
}

function run(command, args) {
  const result = vsDevCmd && !compiler ? runInVsDevCmd(command, args) : spawnSync(command, args, { cwd: root, stdio: "inherit", shell: false, timeout: 120000 });
  if (result.error?.code === "ETIMEDOUT") {
    console.error(`${command} timed out`);
    process.exit(124);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function requireFile(path) {
  if (!existsSync(path)) {
    console.error(`Expected build artifact was not created: ${path}`);
    process.exit(1);
  }
}

function runInVsDevCmd(command, args) {
  mkdirSync(buildDir, { recursive: true });
  const commandLine = [`@echo off`, `call ${quoteCmd(vsDevCmd)} -arch=x64 -host_arch=x64 >nul`, [quoteCmd(command), ...args.map(quoteCmd)].join(" ")].join("\r\n");
  const scriptPath = resolve(buildDir, "run-vs-command.cmd");
  writeFileSync(scriptPath, `${commandLine}\r\n`, "utf8");
  return spawnSync("cmd.exe", ["/d", "/c", scriptPath], {
    cwd: root,
    stdio: "inherit",
    shell: false,
    timeout: 120000,
  });
}

function compilerFlag() {
  return compiler ? ["-DCMAKE_CXX_COMPILER=" + compiler] : [];
}

function findCompiler() {
  for (const command of ["cl", "clang++", "g++", "c++"]) {
    if (commandExists(command)) {
      return command;
    }
  }
  return null;
}

function commandExists(command) {
  const probe = process.platform === "win32" ? spawnSync("where.exe", [command], { encoding: "utf8" }) : spawnSync("command", ["-v", command], { encoding: "utf8", shell: true });
  return probe.status === 0;
}

function findVsDevCmd() {
  if (process.platform !== "win32") {
    return null;
  }
  const candidates = [
    process.env.ProgramFiles ? resolve(process.env.ProgramFiles, "Microsoft Visual Studio", "Installer", "vswhere.exe") : null,
    process.env["ProgramFiles(x86)"] ? resolve(process.env["ProgramFiles(x86)"], "Microsoft Visual Studio", "Installer", "vswhere.exe") : null,
  ].filter(Boolean);

  for (const vswhere of candidates) {
    if (!existsSync(vswhere)) {
      continue;
    }
    const result = spawnSync(vswhere, ["-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"], { encoding: "utf8" });
    if (result.status !== 0) {
      continue;
    }
    const installationPath = result.stdout.trim().split(/\r?\n/)[0];
    if (!installationPath) {
      continue;
    }
    const candidate = resolve(installationPath, "Common7", "Tools", "VsDevCmd.bat");
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

function quoteCmd(value) {
  return `"${String(value).replaceAll('"', '""')}"`;
}
