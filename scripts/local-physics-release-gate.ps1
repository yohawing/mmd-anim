param(
    [string[]]$Config,
    [string]$ConfigGlob = "physics-*.local.json",
    [string]$MmdAnimBin,
    [switch]$SkipBuild,
    [switch]$List,
    [switch]$AllowMissing
)

$ErrorActionPreference = "Stop"

function Resolve-RepoPath {
    param([string]$Path)
    return (Resolve-Path -LiteralPath $Path).Path
}

function Resolve-GateConfig {
    param(
        [string]$Path,
        [string]$GateRoot
    )
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return Resolve-RepoPath $Path
    }
    return Resolve-RepoPath (Join-Path $GateRoot $Path)
}

$repoRoot = Resolve-RepoPath (Join-Path $PSScriptRoot "..")
$gateRoot = Join-Path $repoRoot "tools\golden-gate"

if (-not (Test-Path -LiteralPath $gateRoot -PathType Container)) {
    throw "golden-gate directory not found: $gateRoot"
}

if ($Config -and $Config.Count -gt 0) {
    $configs = @($Config | ForEach-Object { Resolve-GateConfig $_ $gateRoot })
} else {
    $configs = @(
        Get-ChildItem -LiteralPath $gateRoot -Filter $ConfigGlob -File |
            Sort-Object Name |
            ForEach-Object { $_.FullName }
    )
}

if ($configs.Count -eq 0) {
    $message = "No local physics gate configs matched '$ConfigGlob' under $gateRoot."
    if ($AllowMissing) {
        Write-Host "[local-physics-release-gate] $message"
        exit 0
    }
    throw "$message Pass -AllowMissing only for machines without local GoldenOracle assets."
}

$resolvedBin = $null
if ($MmdAnimBin) {
    $resolvedBin = Resolve-RepoPath $MmdAnimBin
} else {
    $resolvedBin = Join-Path $repoRoot "target\debug\mmd-anim.exe"
}

Write-Host "[local-physics-release-gate] configs:"
foreach ($configPath in $configs) {
    $configJson = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
    $caseName = if ($configJson.diagnose_case) { $configJson.diagnose_case } else { "(no diagnose_case)" }
    $frame = if ($configJson.diagnose_frame) { $configJson.diagnose_frame } else { "(no frame)" }
    $bone = if ($configJson.diagnose_bone) { $configJson.diagnose_bone } else { "(all bones)" }
    Write-Host "  - $([System.IO.Path]::GetFileName($configPath)): case=$caseName frame=$frame bone=$bone"
}

if ($List) {
    exit 0
}

if (-not $SkipBuild) {
    Push-Location $repoRoot
    try {
        Write-Host "[local-physics-release-gate] building mmd-anim CLI with physics-bullet-native"
        & cargo build -p mmd-anim-cli --features physics-bullet-native
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path -LiteralPath $resolvedBin -PathType Leaf)) {
    throw "mmd-anim binary not found: $resolvedBin"
}

$failures = @()
Push-Location $gateRoot
try {
    foreach ($configPath in $configs) {
        $name = [System.IO.Path]::GetFileName($configPath)
        Write-Host "[local-physics-release-gate] gate $name"
        & uv run golden-gate gate --config $configPath --mmd-anim-bin $resolvedBin
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            $failures += [pscustomobject]@{
                Config = $name
                ExitCode = $exitCode
            }
        }
    }
} finally {
    Pop-Location
}

if ($failures.Count -gt 0) {
    Write-Host "[local-physics-release-gate] failed configs:"
    foreach ($failure in $failures) {
        Write-Host "  - $($failure.Config): exit $($failure.ExitCode)"
    }
    exit 1
}

Write-Host "[local-physics-release-gate] passed $($configs.Count) physics GoldenOracle gate(s)."
