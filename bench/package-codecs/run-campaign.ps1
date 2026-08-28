param(
  [string]$Manifest = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack\campaign.json'),
  [string]$RawDirectory = (Join-Path $PSScriptRoot '..\..\.ai\mmdpack'),
  [string]$Report = (Join-Path $PSScriptRoot '..\..\docs\mmdpack-benchmark.md'),
  [switch]$ForceCommandFailure
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$manifestFull = [System.IO.Path]::GetFullPath($Manifest)
$rawFull = [System.IO.Path]::GetFullPath($RawDirectory)
$reportFull = [System.IO.Path]::GetFullPath($Report)
$runId = [Guid]::NewGuid().ToString('N')
$measuredAt = [DateTime]::UtcNow.ToString('o')
$runDirectory = Join-Path $repoRoot ('.ai\mmdpack\runs\' + $runId)
$candidateNative = Join-Path $runDirectory 'native.json'
$candidateWasm = Join-Path $runDirectory 'wasm.json'
$candidateReport = Join-Path $runDirectory 'mmdpack-benchmark.md'
$cargoLock = Join-Path $PSScriptRoot 'Cargo.lock'

function Get-Sha256 {
  param([string]$Path)
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-HarnessSourceDigest {
  $entries = Get-ChildItem -LiteralPath $PSScriptRoot -Recurse -File |
    Where-Object { $_.FullName -notmatch '\\(pkg|target)\\' } |
    Sort-Object FullName |
    ForEach-Object {
      $relative = $_.FullName.Substring($PSScriptRoot.Length + 1).Replace('\', '/')
      $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
      "$relative`:$hash"
    }
  $bytes = [Text.Encoding]::UTF8.GetBytes([String]::Join("`n", $entries))
  $digest = [Security.Cryptography.SHA256]::Create()
  try {
    return ([BitConverter]::ToString($digest.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
  } finally {
    $digest.Dispose()
  }
}

function Publish-Atomic {
  param([string]$Source, [string]$Destination, [string]$RunId)
  $destinationDirectory = [System.IO.Path]::GetDirectoryName($Destination)
  New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null
  $temporary = Join-Path $destinationDirectory ('.' + [System.IO.Path]::GetFileName($Destination) + '.' + $RunId + '.tmp')
  try {
    Copy-Item -LiteralPath $Source -Destination $temporary -Force
    Move-Item -LiteralPath $temporary -Destination $Destination -Force
  } finally {
    if (Test-Path -LiteralPath $temporary) {
      Remove-Item -LiteralPath $temporary -Force
    }
  }
}

function Invoke-Checked {
  param([string]$Label, [scriptblock]$Command)
  & $Command
  if ($LASTEXITCODE -ne 0) {
    throw "$Label failed with exit code $LASTEXITCODE"
  }
}

function Assert-RunInputsUnchanged {
  if ((Get-Sha256 $manifestFull) -ne $manifestHash) {
    throw 'campaign manifest changed during run; refusing publication'
  }
  if ((Get-Sha256 $cargoLock) -ne $cargoLockHash) {
    throw 'standalone Cargo.lock changed during run; refusing publication'
  }
  if ((Get-HarnessSourceDigest) -ne $harnessSourceDigest) {
    throw 'harness source changed during run; refusing publication'
  }
}

New-Item -ItemType Directory -Force -Path $runDirectory | Out-Null
$manifestHash = Get-Sha256 $manifestFull
$cargoLockHash = Get-Sha256 $cargoLock
$wasmPackOutput = & wasm-pack --version
if ($LASTEXITCODE -ne 0) {
  throw "wasm-pack --version failed with exit code $LASTEXITCODE"
}
$wasmPackVersion = ($wasmPackOutput -join "`n").Trim()
$harnessSourceDigest = Get-HarnessSourceDigest

if ($ForceCommandFailure) {
  Invoke-Checked 'forced command failure self-test' { & cmd.exe /d /c exit 37 }
}

Push-Location $PSScriptRoot
try {
  Invoke-Checked 'npm run build:wasm' { & npm run build:wasm }
  Invoke-Checked 'native measurement' { & cargo run --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --release -p package-codecs-native -- `
    --manifest $manifestFull --raw-output $candidateNative --run-id $runId --measured-at $measuredAt `
    --cargo-lock $cargoLock --manifest-sha256 $manifestHash --cargo-lock-sha256 $cargoLockHash `
    --wasm-pack $wasmPackVersion --harness-source-digest $harnessSourceDigest }
  Invoke-Checked 'WASM measurement' { & node run-wasm.mjs --manifest $manifestFull --output $candidateWasm --cargo-lock $cargoLock `
    --manifest-sha256 $manifestHash --cargo-lock-sha256 $cargoLockHash `
    --run-id $runId --measured-at $measuredAt --harness-source-digest $harnessSourceDigest }
  Assert-RunInputsUnchanged
  Invoke-Checked 'report validation/render' { & cargo run --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml') --release -p package-codecs-native -- `
    --render-only --native-json $candidateNative --wasm-json $candidateWasm --report $candidateReport }
  Assert-RunInputsUnchanged
  foreach ($candidate in @($candidateNative, $candidateWasm, $candidateReport)) {
    if (!(Test-Path -LiteralPath $candidate) -or (Get-Item -LiteralPath $candidate).Length -eq 0) {
      throw "empty campaign candidate: $candidate"
    }
  }
  Publish-Atomic $candidateNative (Join-Path $rawFull 'native.json') $runId
  Publish-Atomic $candidateWasm (Join-Path $rawFull 'wasm.json') $runId
  Publish-Atomic $candidateReport $reportFull $runId
  Write-Output "published run=$runId report=$reportFull"
} finally {
  Pop-Location
}
