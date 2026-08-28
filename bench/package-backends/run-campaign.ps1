param(
  [string]$Manifest = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\campaign.json'),
  [string]$RawDirectory = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\backends'),
  [string]$Report = (Join-Path $PSScriptRoot '..\..\docs\mmdpack-backend-decision.md'),
  [switch]$ForceCommandFailure
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifestFull = [System.IO.Path]::GetFullPath($Manifest)
$rawFull = [System.IO.Path]::GetFullPath($RawDirectory)
$reportFull = [System.IO.Path]::GetFullPath($Report)
$cargoLock = Join-Path $PSScriptRoot 'Cargo.lock'
$runId = ('{0:yyyyMMddTHHmmssfffZ}-{1}' -f [DateTime]::UtcNow, ([Guid]::NewGuid().ToString('N').Substring(0, 8)))
$measuredAt = [DateTime]::UtcNow.ToString('o')
$runDirectory = Join-Path $rawFull ('runs\' + $runId)
$candidateNative = Join-Path $runDirectory 'native.json'
$candidateWasm = Join-Path $runDirectory 'wasm.json'
$candidateReport = Join-Path $runDirectory 'mmdpack-backend-decision.md'
$wasmPackage = Join-Path $PSScriptRoot 'pkg'

function Get-Sha256 {
  param([string]$Path)
  if (!(Test-Path -LiteralPath $Path -PathType Leaf)) { throw "missing hash input: $Path" }
  (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-HarnessSourceDigest {
  $entries = Get-ChildItem -LiteralPath $PSScriptRoot -Recurse -File |
    Where-Object { $_.FullName -notmatch '\\(pkg|target)\\' -and $_.Name -ne 'Cargo.lock' } |
    Sort-Object FullName |
    ForEach-Object {
      $relative = $_.FullName.Substring($PSScriptRoot.Length + 1).Replace('\', '/')
      "$relative`:$((Get-Sha256 $_.FullName))"
    }
  $bytes = [Text.Encoding]::UTF8.GetBytes([String]::Join("`n", $entries))
  $sha = [Security.Cryptography.SHA256]::Create()
  try { ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
}

function Invoke-Checked {
  param([string]$Label, [scriptblock]$Command)
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "$Label failed with exit code $LASTEXITCODE" }
}

function Publish-Atomic {
  param([string]$Source, [string]$Destination, [string]$RunId)
  $directory = [System.IO.Path]::GetDirectoryName($Destination)
  New-Item -ItemType Directory -Force -Path $directory | Out-Null
  $temporary = Join-Path $directory ('.' + [System.IO.Path]::GetFileName($Destination) + '.' + $RunId + '.tmp')
  try {
    Copy-Item -LiteralPath $Source -Destination $temporary -Force
    Move-Item -LiteralPath $temporary -Destination $Destination -Force
  } finally {
    if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
  }
}

function Assert-InputsUnchanged {
  if ((Get-Sha256 $manifestFull) -ne $manifestHash) { throw 'campaign manifest changed during run; refusing publication' }
  if ((Get-Sha256 $cargoLock) -ne $cargoLockHash) { throw 'standalone Cargo.lock changed during run; refusing publication' }
  if ((Get-HarnessSourceDigest) -ne $harnessSourceDigest) { throw 'harness source changed during run; refusing publication' }
}

New-Item -ItemType Directory -Force -Path $runDirectory | Out-Null
$manifestHash = Get-Sha256 $manifestFull
$cargoLockHash = Get-Sha256 $cargoLock
$harnessSourceDigest = Get-HarnessSourceDigest
$wasmPackVersion = (& wasm-pack --version | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "wasm-pack --version failed with exit code $LASTEXITCODE" }
if ([string]::IsNullOrWhiteSpace($env:CC_wasm32_unknown_unknown)) {
  $clang = Get-Command clang -ErrorAction SilentlyContinue
  if ($null -ne $clang) {
    $env:CC_wasm32_unknown_unknown = $clang.Source
  } elseif ($env:ProgramFiles) {
    $fallbackClang = Join-Path $env:ProgramFiles 'LLVM\bin\clang.exe'
    if (Test-Path -LiteralPath $fallbackClang -PathType Leaf) {
      $env:CC_wasm32_unknown_unknown = $fallbackClang
    }
  }
}
if ([string]::IsNullOrWhiteSpace($env:AR_wasm32_unknown_unknown)) {
  $llvmAr = Get-Command llvm-ar -ErrorAction SilentlyContinue
  if ($null -ne $llvmAr) {
    $env:AR_wasm32_unknown_unknown = $llvmAr.Source
  } elseif ($env:ProgramFiles) {
    $fallbackAr = Join-Path $env:ProgramFiles 'LLVM\bin\llvm-ar.exe'
    if (Test-Path -LiteralPath $fallbackAr -PathType Leaf) {
      $env:AR_wasm32_unknown_unknown = $fallbackAr
    }
  }
}

if ($ForceCommandFailure) {
  Invoke-Checked 'forced command failure self-test' { & cmd.exe /d /c exit 37 }
}

$campaignKeyBytes = New-Object byte[] 32
$random = [Security.Cryptography.RandomNumberGenerator]::Create()
try { $random.GetBytes($campaignKeyBytes) }
finally { $random.Dispose() }
$env:MMDPACK_BACKENDS_CAMPAIGN_KEY_HEX = ([BitConverter]::ToString($campaignKeyBytes)).Replace('-', '').ToLowerInvariant()

Push-Location $PSScriptRoot
try {
  if (Test-Path -LiteralPath $wasmPackage) { Remove-Item -LiteralPath $wasmPackage -Recurse -Force }
  Invoke-Checked 'wasm-pack build' {
    $wasmArgs = @('build', (Join-Path $PSScriptRoot 'wasm'), '--target', 'nodejs', '--release', '--out-dir', $wasmPackage, '--out-name', 'package_backends_wasm')
    & wasm-pack @wasmArgs
  }
  Invoke-Checked 'node runner syntax check' { & node --check (Join-Path $PSScriptRoot 'wasm\run-wasm.cjs') }
  Invoke-Checked 'report renderer syntax check' { & node --check (Join-Path $PSScriptRoot 'render-report.cjs') }
  Invoke-Checked 'native backend lane' {
    $nativeArgs = @('run', '--manifest-path', (Join-Path $PSScriptRoot 'Cargo.toml'), '--release', '-p', 'package-backends-native', '--',
      '--manifest', $manifestFull, '--cargo-lock', $cargoLock, '--run-dir', $runDirectory, '--output', $candidateNative,
      '--run-id', $runId, '--measured-at', $measuredAt, '--expected-manifest-sha256', $manifestHash,
      '--expected-lock-sha256', $cargoLockHash, '--harness-source-digest', $harnessSourceDigest, '--wasm-pack', $wasmPackVersion)
    & cargo @nativeArgs
  }
  Invoke-Checked 'WASM/Node backend lane' {
    $wasmRunnerArgs = @((Join-Path $PSScriptRoot 'wasm\run-wasm.cjs'), '--manifest', $manifestFull, '--cargo-lock', $cargoLock,
      '--run-dir', $runDirectory, '--output', $candidateWasm, '--wasm-wrapper', (Join-Path $wasmPackage 'package_backends_wasm.js'),
      '--wasm-module', (Join-Path $wasmPackage 'package_backends_wasm_bg.wasm'),
      '--run-id', $runId, '--measured-at', $measuredAt, '--expected-manifest-sha256', $manifestHash,
      '--expected-lock-sha256', $cargoLockHash, '--harness-source-digest', $harnessSourceDigest, '--wasm-pack', $wasmPackVersion)
    & node @wasmRunnerArgs
  }
  Assert-InputsUnchanged
  Invoke-Checked 'backend report validation/render' {
    $renderArgs = @((Join-Path $PSScriptRoot 'render-report.cjs'), '--native-json', $candidateNative, '--wasm-json', $candidateWasm,
      '--output', $candidateReport, '--expected-run-id', $runId, '--expected-manifest-sha256', $manifestHash,
      '--expected-lock-sha256', $cargoLockHash, '--expected-source-digest', $harnessSourceDigest)
    & node @renderArgs
  }
  Assert-InputsUnchanged
  foreach ($candidate in @($candidateNative, $candidateWasm, $candidateReport)) {
    if (!(Test-Path -LiteralPath $candidate -PathType Leaf) -or (Get-Item -LiteralPath $candidate).Length -eq 0) {
      throw "empty backend candidate: $candidate"
    }
  }
  Assert-InputsUnchanged
  Publish-Atomic $candidateNative (Join-Path $rawFull 'native.json') $runId
  Publish-Atomic $candidateWasm (Join-Path $rawFull 'wasm.json') $runId
  Publish-Atomic $candidateReport $reportFull $runId
  Write-Output "published backend run=$runId report=$reportFull"
} finally {
  Remove-Item Env:MMDPACK_BACKENDS_CAMPAIGN_KEY_HEX -ErrorAction SilentlyContinue
  Pop-Location
}
