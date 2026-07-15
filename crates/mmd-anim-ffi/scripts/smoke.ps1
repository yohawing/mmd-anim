<#
.SYNOPSIS
  mmd-anim-ffi smoke test: build cdylib and validate header symbol coverage.
#>
$ErrorActionPreference = "Stop"
# Cargo writes normal progress messages to stderr. Treat that stream as output
# and rely on its exit code below, otherwise PowerShell 7 can stop here before
# the release build completes.
$PSNativeCommandUseErrorActionPreference = $false
$ProjectRoot = Resolve-Path "$PSScriptRoot\..\..\.."
$FfiDir     = Resolve-Path "$PSScriptRoot\.."

Write-Host "=== mmd-anim-ffi smoke ===" -ForegroundColor Cyan

# --- 1. Build cdylib -------------------------------------------------
Write-Host "`n[1/4] Building mmd-anim-ffi (release)..." -ForegroundColor Yellow
& cargo build -p mmd-anim-ffi --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: cargo build exited $LASTEXITCODE" -ForegroundColor Red
    exit 1
}
Write-Host "  OK" -ForegroundColor Green

# --- 2. Verify cdylib exists -----------------------------------------
Write-Host "`n[2/4] Locating cdylib..." -ForegroundColor Yellow
$cdylib = Get-ChildItem -Recurse "$ProjectRoot\target\release" |
    Where-Object { $_.Name -in @("mmd_runtime_ffi.dll", "libmmd_runtime_ffi.so", "libmmd_runtime_ffi.dylib") } |
    Select-Object -First 1
if (-not $cdylib) {
    Write-Host "FAIL: mmd-anim-ffi cdylib not found under target/release" -ForegroundColor Red
    exit 1
}
Write-Host "  Found: $($cdylib.FullName) ($($cdylib.Length) bytes)" -ForegroundColor Green

# --- 3. Deterministic Rust/header drift check ------------------------
Write-Host "`n[3/4] Checking Rust/header ABI drift..." -ForegroundColor Yellow
& python "$ProjectRoot\tools\check_ffi_header_symbols.py"
if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: header drift check exited $LASTEXITCODE" -ForegroundColor Red
    exit 1
}
Write-Host "  OK" -ForegroundColor Green

# --- 4. Dumpbin symbol check (optional) ------------------------------
Write-Host "`n[4/4] Cross-referencing exports against header..." -ForegroundColor Yellow
$header = Get-Content "$FfiDir\include\mmd_runtime.h" -Raw
if ($header -match "\bmmd_runtime_reduced_pose_sample\b") {
    Write-Host "  FAIL: dense reduced-pose sampling remains in the public header." -ForegroundColor Red
    exit 1
}

# Expected function export names (no-mangle C symbols from lib.rs)
$expectedExports = @(
    "mmd_runtime_abi_version"
    "mmd_runtime_byte_buffer_free"
    "mmd_runtime_model_create"
    "mmd_runtime_model_create_with_inverse_bind"
    "mmd_runtime_model_create_with_append"
    "mmd_runtime_model_create_with_append_and_inverse_bind"
    "mmd_runtime_model_create_full"
    "mmd_runtime_model_create_full_with_transform_order"
    "mmd_runtime_model_create_full_with_morphs"
    "mmd_runtime_model_create_from_pmx_bytes"
    "mmd_runtime_export_pmx_from_parts"
    "mmd_runtime_model_bone_count"
    "mmd_runtime_model_morph_count"
    "mmd_runtime_model_ik_count"
    "mmd_runtime_model_free"
    "mmd_runtime_instance_create"
    "mmd_runtime_instance_create_for_model"
    "mmd_runtime_instance_create_with_counts"
    "mmd_runtime_instance_free"
    "mmd_runtime_instance_evaluate_rest_pose"
    "mmd_runtime_instance_evaluate_clip_frame"
    "mmd_runtime_instance_evaluate_clip_frame_without_ik"
    "mmd_runtime_instance_world_matrix_f32_len"
    "mmd_runtime_instance_copy_world_matrices"
    "mmd_runtime_instance_skinning_matrix_f32_len"
    "mmd_runtime_instance_copy_skinning_matrices"
    "mmd_runtime_instance_morph_weight_len"
    "mmd_runtime_instance_copy_morph_weights"
    "mmd_runtime_instance_morph_weights"
    "mmd_runtime_instance_ik_enabled_len"
    "mmd_runtime_instance_copy_ik_enabled"
    "mmd_runtime_instance_ik_enabled"
    "mmd_runtime_clip_create"
    "mmd_runtime_clip_create_from_vmd_bytes_for_model"
    "mmd_runtime_clip_frame_range"
    "mmd_runtime_clip_free"
    "mmd_runtime_reduced_pose_create_from_dense"
    "mmd_runtime_reduced_pose_free"
    "mmd_runtime_reduced_pose_bone_count"
    "mmd_runtime_reduced_pose_morph_count"
    "mmd_runtime_reduced_pose_report"
    "mmd_runtime_reduced_pose_unity_curve_count"
    "mmd_runtime_reduced_pose_unity_curve_descriptor"
    "mmd_runtime_reduced_pose_unity_curve_keys"
)

# Check each expected export name appears in the header
$missingFromHeader = @()
foreach ($fn in $expectedExports) {
    if ($header -notmatch "\b$fn\b") {
        $missingFromHeader += $fn
    }
}
if ($missingFromHeader.Count -gt 0) {
    Write-Host "  FAIL: $($missingFromHeader.Count) export(s) missing from header:" -ForegroundColor Red
    $missingFromHeader | ForEach-Object { Write-Host "    - $_" }
    exit 1
} else {
    Write-Host "  All $($expectedExports.Count) exports present in header." -ForegroundColor Green
}

# Try dumpbin if available on PATH
$dumpbin = Get-Command "dumpbin" -ErrorAction SilentlyContinue
if (-not $dumpbin) {
    Write-Host "  dumpbin not on PATH; skipping binary export verification." -ForegroundColor DarkYellow
} else {
    Write-Host "  dumpbin found; checking binary exports..." -ForegroundColor Yellow
    $exports = @(& dumpbin /EXPORTS $cdylib.FullName 2>&1 | Where-Object { $_ -match '^\s+\d+\s+\w+\s+\w+\s+(\w+)' } | ForEach-Object { $matches[1] })
    if ("mmd_runtime_reduced_pose_sample" -in $exports) {
        Write-Host "  FAIL: dense reduced-pose sampling remains in the binary exports." -ForegroundColor Red
        exit 1
    }
    $missingFromBinary = $expectedExports | Where-Object { $_ -notin $exports }
    if ($missingFromBinary.Count -gt 0) {
        Write-Host "  FAIL: $($missingFromBinary.Count) export(s) missing from binary:" -ForegroundColor Red
        $missingFromBinary | ForEach-Object { Write-Host "    - $_" }
        exit 1
    } else {
        Write-Host "  All $($expectedExports.Count) exports confirmed in binary." -ForegroundColor Green
    }
}

Write-Host "`n=== smoke PASS ===" -ForegroundColor Cyan
exit 0
