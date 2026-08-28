[CmdletBinding()]
param(
    [string]$Manifest = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\textures\manifest.json'),
    [string]$Basisu = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\tools\basis_universal-v2_50\bin\basisu.exe'),
    [string]$TranscoderJs = (Join-Path $PSScriptRoot '..\..\..\references\three.js\examples\jsm\libs\basis\basis_transcoder.js'),
    [string]$TranscoderWasm = (Join-Path $PSScriptRoot '..\..\..\references\three.js\examples\jsm\libs\basis\basis_transcoder.wasm'),
    [string]$ControlInput = (Join-Path $PSScriptRoot '..\..\..\references\three.js\examples\textures\ktx2\2d_uastc.ktx2'),
    [string]$RawOutput = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\textures\latest.json'),
    [string]$ReportOutput = (Join-Path $PSScriptRoot '..\..\docs\mmdpack-texture-decision.md'),
    [string]$ControlReportOutput = (Join-Path $PSScriptRoot '..\..\docs\mmdpack-texture-wasm-control.md')
)
$ErrorActionPreference = 'Stop'
$PinnedControlSha256 = '21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$cargoManifest = (Resolve-Path (Join-Path $PSScriptRoot 'Cargo.toml')).Path
$cargoLock = Join-Path $PSScriptRoot 'Cargo.lock'

function Invoke-Checked([string]$FilePath, [string[]]$Arguments) {
    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) { throw "external command failed ($exitCode): $FilePath" }
}
function Get-Hash([string]$Path) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Get-HarnessDigest {
    $files = Get-ChildItem -LiteralPath $PSScriptRoot -Recurse -File |
        Where-Object { $_.FullName -notmatch '\\target\\' -and $_.Name -ne 'Cargo.lock' } |
        Sort-Object FullName
    $lines = foreach ($file in $files) {
        $relative = $file.FullName.Substring($repo.Length).Replace('\', '/')
        "$relative $(Get-Hash $file.FullName)"
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([Convert]::ToHexString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($lines -join "`n") + "`n")))).ToLowerInvariant() }
    finally { $sha.Dispose() }
}
function Publish-Atomic([string]$Candidate, [string]$Destination, [string]$RunId) {
    $parent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $temp = "$Destination.$RunId.tmp"
    Copy-Item -LiteralPath $Candidate -Destination $temp -Force
    Move-Item -LiteralPath $temp -Destination $Destination -Force
}

if (!(Test-Path -LiteralPath $Manifest) -or !(Test-Path -LiteralPath $Basisu) -or !(Test-Path -LiteralPath $TranscoderJs) -or !(Test-Path -LiteralPath $TranscoderWasm) -or !(Test-Path -LiteralPath $ControlInput) -or !(Test-Path -LiteralPath $cargoLock)) { throw 'manifest, tools, control input, or standalone Cargo.lock is missing' }
$manifestBytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Manifest).Path)
$expectedManifest = (Get-FileHash -InputStream ([IO.MemoryStream]::new($manifestBytes)) -Algorithm SHA256).Hash.ToLowerInvariant()
$expectedLock = Get-Hash $cargoLock
$expectedSource = Get-HarnessDigest
$expectedBasis = Get-Hash $Basisu
$expectedJs = Get-Hash $TranscoderJs
$expectedWasm = Get-Hash $TranscoderWasm
$expectedControlInput = Get-Hash $ControlInput
if ($expectedControlInput -ne $PinnedControlSha256) {
    throw 'control input does not match the pinned Three.js fixture SHA-256'
}
$runId = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssfffZ') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$runDir = Join-Path (Join-Path $repo '.ai\mmdpack\textures\runs') $runId
New-Item -ItemType Directory -Force -Path $runDir | Out-Null
$nativeJson = Join-Path $runDir 'native.json'
$wasmJson = Join-Path $runDir 'wasm.json'
$controlJson = Join-Path $runDir 'control.json'
$controlReportCandidate = Join-Path $runDir 'control.md'
$rawCandidate = Join-Path $runDir 'latest.json'
$reportCandidate = Join-Path $runDir 'report.md'

try {
    Invoke-Checked 'node' @((Join-Path $PSScriptRoot 'wasm\transcode.cjs'), '--control', '--input', (Resolve-Path -LiteralPath $ControlInput).Path, '--input-label', 'threejs-example-2d-uastc', '--run-id', $runId, '--output', $controlJson, '--report', $controlReportCandidate, '--transcoder-js', (Resolve-Path -LiteralPath $TranscoderJs).Path, '--transcoder-wasm', (Resolve-Path -LiteralPath $TranscoderWasm).Path, '--expected-input-sha256', $expectedControlInput, '--expected-transcoder-js-sha256', $expectedJs, '--expected-transcoder-wasm-sha256', $expectedWasm)
    $control = Get-Content -LiteralPath $controlJson -Raw | ConvertFrom-Json
    if ($control.status -ne 'ok' -or $control.result.ok -ne $true) { throw 'known-good WASM control failed; campaign not started' }
    Invoke-Checked 'cargo' @('run', '--manifest-path', $cargoManifest, '--quiet', '--', 'run', '--manifest', (Resolve-Path -LiteralPath $Manifest).Path, '--basisu', (Resolve-Path -LiteralPath $Basisu).Path, '--run-dir', $runDir, '--output', $nativeJson, '--run-id', $runId, '--expected-manifest-sha256', $expectedManifest, '--expected-lock-sha256', $expectedLock, '--source-digest', $expectedSource)
    Invoke-Checked 'node' @((Join-Path $PSScriptRoot 'wasm\transcode.cjs'), '--manifest', (Resolve-Path -LiteralPath $Manifest).Path, '--native', $nativeJson, '--run-dir', $runDir, '--output', $wasmJson, '--transcoder-js', (Resolve-Path -LiteralPath $TranscoderJs).Path, '--transcoder-wasm', (Resolve-Path -LiteralPath $TranscoderWasm).Path, '--lock', $cargoLock, '--expected-manifest-sha256', $expectedManifest, '--expected-lock-sha256', $expectedLock, '--source-digest', $expectedSource, '--expected-transcoder-js-sha256', $expectedJs, '--expected-transcoder-wasm-sha256', $expectedWasm)
    if ((Get-Hash $Manifest) -ne $expectedManifest -or (Get-Hash $cargoLock) -ne $expectedLock -or (Get-HarnessDigest) -ne $expectedSource -or (Get-Hash $Basisu) -ne $expectedBasis -or (Get-Hash $TranscoderJs) -ne $expectedJs -or (Get-Hash $TranscoderWasm) -ne $expectedWasm) { throw 'manifest, lock, harness, or tool drifted before publication' }
    $native = Get-Content -LiteralPath $nativeJson -Raw | ConvertFrom-Json
    $wasm = Get-Content -LiteralPath $wasmJson -Raw | ConvertFrom-Json
    [ordered]@{ schema = 1; run_id = $runId; manifest_sha256 = $expectedManifest; lock_sha256 = $expectedLock; source_digest = $expectedSource; control = $control; native = $native; wasm = $wasm } | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $rawCandidate -Encoding UTF8
    Invoke-Checked 'node' @((Join-Path $PSScriptRoot 'render-report.cjs'), '--native', $nativeJson, '--wasm', $wasmJson, '--control', $controlJson, '--output', $reportCandidate, '--expected-manifest-sha256', $expectedManifest, '--expected-lock-sha256', $expectedLock, '--source-digest', $expectedSource, '--expected-control-input-sha256', $expectedControlInput)
    if ((Get-Hash $Manifest) -ne $expectedManifest -or (Get-Hash $cargoLock) -ne $expectedLock -or (Get-HarnessDigest) -ne $expectedSource -or (Get-Hash $Basisu) -ne $expectedBasis -or (Get-Hash $TranscoderJs) -ne $expectedJs -or (Get-Hash $TranscoderWasm) -ne $expectedWasm -or (Get-Hash $ControlInput) -ne $expectedControlInput) { throw 'manifest, lock, harness, control input, or tool drifted after rendering' }
    Publish-Atomic $rawCandidate $RawOutput $runId
    if ((Get-Hash $Manifest) -ne $expectedManifest -or (Get-Hash $cargoLock) -ne $expectedLock -or (Get-HarnessDigest) -ne $expectedSource -or (Get-Hash $Basisu) -ne $expectedBasis -or (Get-Hash $TranscoderJs) -ne $expectedJs -or (Get-Hash $TranscoderWasm) -ne $expectedWasm -or (Get-Hash $ControlInput) -ne $expectedControlInput) { throw 'manifest, lock, harness, control input, or tool drifted before report publication' }
    Publish-Atomic $reportCandidate $ReportOutput $runId
    if ((Get-Hash $Manifest) -ne $expectedManifest -or (Get-Hash $cargoLock) -ne $expectedLock -or (Get-HarnessDigest) -ne $expectedSource -or (Get-Hash $Basisu) -ne $expectedBasis -or (Get-Hash $TranscoderJs) -ne $expectedJs -or (Get-Hash $TranscoderWasm) -ne $expectedWasm -or (Get-Hash $ControlInput) -ne $expectedControlInput) { throw 'manifest, lock, harness, control input, or tool drifted before control report publication' }
    Publish-Atomic $controlReportCandidate $ControlReportOutput $runId
    Write-Host "Published texture Phase 0 run $runId"
} catch {
    Write-Error $_
    exit 1
}
