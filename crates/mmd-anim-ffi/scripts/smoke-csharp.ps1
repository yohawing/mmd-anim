<#
.SYNOPSIS
  mmd-anim-ffi C# P/Invoke smoke test: build cdylib, then run C# console smoke.
#>
$ErrorActionPreference = "Stop"
$ProjectRoot = Resolve-Path "$PSScriptRoot\..\..\.."
$FfiDir     = Resolve-Path "$PSScriptRoot\.."

Write-Host "=== mmd-anim-ffi C# smoke ===" -ForegroundColor Cyan

# --- 1. Build cdylib -------------------------------------------------
Write-Host "`n[1/3] Building mmd-anim-ffi (release)..." -ForegroundColor Yellow
$build = & cargo build -p mmd-anim-ffi --release 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: cargo build exited $LASTEXITCODE" -ForegroundColor Red
    exit 1
}
Write-Host "  OK" -ForegroundColor Green

# --- 2. Locate DLL ---------------------------------------------------
Write-Host "`n[2/3] Locating cdylib..." -ForegroundColor Yellow
$cdylib = Get-ChildItem -Recurse "$ProjectRoot\target\release" |
    Where-Object { $_.Name -in @("mmd_runtime_ffi.dll", "libmmd_runtime_ffi.so", "libmmd_runtime_ffi.dylib") } |
    Select-Object -First 1
if (-not $cdylib) {
    Write-Host "FAIL: mmd-anim-ffi cdylib not found under target/release" -ForegroundColor Red
    exit 1
}
Write-Host "  Found: $($cdylib.FullName) ($($cdylib.Length) bytes)" -ForegroundColor Green

# Set absolute path for the C# DllImportResolver
$Env:MMD_RUNTIME_FFI_PATH = $cdylib.FullName

# --- 3. Run C# smoke ------------------------------------------------
Write-Host "`n[3/3] Running C# smoke..." -ForegroundColor Yellow
$output = & dotnet run --project "$FfiDir\csharp-smoke" 2>&1
$output | ForEach-Object { Write-Host "$_" }
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: C# smoke exited $LASTEXITCODE" -ForegroundColor Red
    exit 1
}

Write-Host "`n=== C# smoke PASS ===" -ForegroundColor Cyan
exit 0
