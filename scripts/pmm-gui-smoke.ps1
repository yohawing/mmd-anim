<#
.SYNOPSIS
  Guarded PowerShell harness for PMM GUI smoke preparation and optional execution.
  - Defaults to NO GUI / MMD launch.
  - Always validates that the target PMM path exists (repo-local or user-supplied).
  - If MMDDumper is available, uses its non-GUI CLI (inspect-pmm-model-slots) to
    validate/print embedded model path policy, and optionally runs compare-pmm-document-vmd-keyframes
    against a repo-local VMD fixture (or explicit -VmdPath) when both are present.
  - GUI launch (delegation to MMDDumper mmd-first-load-smoke.mjs with a synthesized fixture)
    is refused unless BOTH an explicit -LaunchGui switch is supplied AND the env var
    MMD_DUMPER_ALLOW_MMD_LAUNCH=1 is present. This keeps MMD 9.32 GUI smoke manual/explicit.
  - Always emits a machine-readable JSON status object (use -Json). Status values include
    "dry-run", "skipped", "success", "failure".
  - MMDDumper independent where possible: PMM existence is always checked locally; dumper
    steps are best-effort and skipped gracefully when the dumper tree is absent.
#>
param(
    [Parameter(Mandatory = $true, Position = 0, HelpMessage = "Path to the PMM to validate / smoke.")]
    [string]$PmmPath,

    [string]$VmdPath,
    [switch]$LaunchGui,
    [switch]$Json,
    [string]$DumperRoot = "F:\Develop\MMDDev\MMDDumper",
    [int]$GuiTimeoutMs = 45000,
    [int]$PatchDocumentModelIndex = 0,
    [string]$PatchModelPath,
    [string]$PatchedPmmPath,
    [int]$PatchCurrentFrame,
    [int]$PatchCurrentFrameText,
    [int]$PatchBeginFrame,
    [int]$PatchEndFrame,
    [string]$PatchBeginFrameEnabled,
    [string]$PatchEndFrameEnabled,
    [string]$FrameRangePatchedPmmPath
)

$ErrorActionPreference = "Stop"

$scriptDir = $null
if ($PSScriptRoot) {
    $scriptDir = $PSScriptRoot
} else {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
}
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path

function Resolve-Safe([string]$p) {
    try {
        if ([string]::IsNullOrWhiteSpace($p)) { return $null }
        if (Test-Path -LiteralPath $p) {
            return (Resolve-Path -LiteralPath $p).Path
        }
    } catch {}
    return $null
}

$pmm = Resolve-Safe $PmmPath
$pmmExists = $false
if ($pmm) { $pmmExists = Test-Path -LiteralPath $pmm -PathType Leaf }

# Detect frame range patch request using ContainsKey so explicit 0 for ints is distinguished from omitted.
$frameRangePatchRequested = $PSBoundParameters.ContainsKey('PatchCurrentFrame') -or
                            $PSBoundParameters.ContainsKey('PatchCurrentFrameText') -or
                            $PSBoundParameters.ContainsKey('PatchBeginFrame') -or
                            $PSBoundParameters.ContainsKey('PatchEndFrame') -or
                            (-not [string]::IsNullOrWhiteSpace($PatchBeginFrameEnabled)) -or
                            (-not [string]::IsNullOrWhiteSpace($PatchEndFrameEnabled))

$report = [PSCustomObject]@{
    script          = "pmm-gui-smoke"
    pmm             = $PmmPath
    pmmResolved     = $pmm
    pmmExists       = $pmmExists
    vmd             = $VmdPath
    vmdResolved     = $null
    dumperRoot      = $null
    dumperAvailable = $false
    nonGuiSteps     = @()
    patch           = [PSCustomObject]@{
        requested            = (-not [string]::IsNullOrWhiteSpace($PatchModelPath))
        documentModelIndex   = $PatchDocumentModelIndex
        modelPath            = $PatchModelPath
        modelPathResolved    = $null
        outputPath           = $null
        outputPathResolved   = $null
        attempted            = $false
        exit                 = $null
        output               = $null
        success              = $false
    }
    frameRangePatch = [PSCustomObject]@{
        requested                  = $frameRangePatchRequested
        currentFrame               = $null
        currentFrameText           = $null
        beginFrame                 = $null
        endFrame                   = $null
        beginFrameEnabled          = $null
        endFrameEnabled            = $null
        frameRangePatchedPmmPath   = $FrameRangePatchedPmmPath
        inputPath                  = $null
        outputPath                 = $null
        outputPathResolved         = $null
        attempted                  = $false
        exit                       = $null
        output                     = $null
        success                    = $false
        options                    = $null
    }
    effectivePmm         = $PmmPath
    effectivePmmResolved = $pmm
    gui             = [PSCustomObject]@{
        switchSupplied   = [bool]$LaunchGui
        envAllowPresent  = ($env:MMD_DUMPER_ALLOW_MMD_LAUNCH -eq "1")
        attempted        = $false
        launched         = $false
        reason           = "default-no-launch"
        preparedFixture  = $null
        exit             = $null
        outputTail       = $null
    }
    status          = "dry-run"
    summary         = ""
}
$nonGuiFailed = $false

if (-not $pmmExists) {
    $report.status = "failure"
    $report.summary = "PMM path does not exist or is not a file"
    if ($Json) {
        $report | ConvertTo-Json -Depth 6
    } else {
        Write-Host "PMM GUI smoke: PMM not found: $PmmPath"
    }
    exit 2
}

# Optional PMM document model path patch via CLI (before non-GUI/GUI steps; MMDDumper-independent).
# If -PatchModelPath supplied: require file exists (else JSON failure, no GUI), resolve to absolute
# (strip \\?\ verbatim prefix for MMD compatibility), invoke cargo CLI patch, capture to report.patch,
# and on success use patched file as effectivePmmResolved for inspect/compare/GUI fixture.
$effectivePmmResolved = $pmm
$pmmToUse = $pmm
$patchRequested = (-not [string]::IsNullOrWhiteSpace($PatchModelPath))
if ($patchRequested) {
    $report.patch.attempted = $true
    if (-not (Test-Path -LiteralPath $PatchModelPath -PathType Leaf)) {
        $report.patch.success = $false
        $report.status = "failure"
        $report.summary = "PatchModelPath does not exist or is not a file: $PatchModelPath"
        $report.patch.output = "model path not found for patch"
        if ($Json) {
            $report | ConvertTo-Json -Depth 6
        } else {
            Write-Host "PMM GUI smoke: patch model not found: $PatchModelPath"
        }
        exit 2
    }
    $modelResolved = $null
    try {
        $rp = Resolve-Path -LiteralPath $PatchModelPath
        $modelResolved = $rp.Path
        if ($modelResolved -like '\\?\*') {
            $modelResolved = $modelResolved.Substring(4)
        }
    } catch {}
    $report.patch.modelPathResolved = $modelResolved
    if ([string]::IsNullOrWhiteSpace($modelResolved)) {
        $report.status = "failure"
        $report.summary = "Failed to resolve PatchModelPath to absolute: $PatchModelPath"
        if ($Json) { $report | ConvertTo-Json -Depth 6 }
        exit 2
    }
    # Compute output location (default under target/pmm-gui-smoke/)
    $patchOutDir = Join-Path $repoRoot "target/pmm-gui-smoke"
    New-Item -ItemType Directory -Force -Path $patchOutDir | Out-Null
    $patchOutPath = $PatchedPmmPath
    if ([string]::IsNullOrWhiteSpace($patchOutPath)) {
        $baseName = [IO.Path]::GetFileNameWithoutExtension($PmmPath)
        $patchOutPath = Join-Path $patchOutDir ("{0}_doc{1}_patched.pmm" -f $baseName, $PatchDocumentModelIndex)
    } else {
        if (-not [IO.Path]::IsPathRooted($patchOutPath)) {
            $patchOutPath = Join-Path $repoRoot $patchOutPath
        }
        $parent = Split-Path -Parent $patchOutPath
        if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    }
    $report.patch.outputPath = $patchOutPath
    # Invoke the CLI patch (stdout/stderr + exit captured under patch)
    $cargoArgs = @(
        "run", "-p", "mmd-anim-cli", "--",
        "patch-pmm-document-model-path",
        $pmm,
        [string]$PatchDocumentModelIndex,
        $modelResolved,
        $patchOutPath
    )
    if (-not $Json) {
        Write-Host "Patching PMM document model path (CLI): cargo run -p mmd-anim-cli -- patch-pmm-document-model-path <pmm> <index> <model> <out>"
    }
    try {
        Push-Location $repoRoot
        try {
            $patchRaw = & cargo @cargoArgs 2>&1
            $patchEx = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        $patchText = ($patchRaw | Out-String).Trim()
        $report.patch.exit = $patchEx
        $report.patch.output = $patchText
        $report.patch.success = ($patchEx -eq 0)
        if (-not $Json) {
            Write-Host "== patch-pmm-document-model-path (exit=$patchEx) =="
            $patchRaw | ForEach-Object { Write-Host $_ }
        }
        if ($patchEx -eq 0) {
            $patchedResolved = Resolve-Safe $patchOutPath
            if ($patchedResolved -and (Test-Path -LiteralPath $patchedResolved -PathType Leaf)) {
                $report.patch.outputPathResolved = $patchedResolved
                $effectivePmmResolved = $patchedResolved
                $pmmToUse = $patchedResolved
                $report.effectivePmm = $patchOutPath
                $report.effectivePmmResolved = $patchedResolved
            } else {
                $report.patch.success = $false
                $report.status = "failure"
                $report.summary = "Patch CLI reported success but patched file missing at $patchOutPath"
                if ($Json) { $report | ConvertTo-Json -Depth 6 }
                exit 2
            }
        } else {
            $report.status = "failure"
            $report.summary = "patch-pmm-document-model-path failed (exit=$patchEx)"
            if ($Json) {
                $report | ConvertTo-Json -Depth 6
            }
            exit 2
        }
    } catch {
        $report.patch.exit = 1
        $report.patch.output = $_.ToString()
        $report.patch.success = $false
        $report.status = "failure"
        $report.summary = "patch invocation threw: $($_.Exception.Message)"
        if ($Json) { $report | ConvertTo-Json -Depth 6 }
        exit 2
    }
} else {
    $pmmToUse = $effectivePmmResolved
}

# Optional PMM scene frame range patch via CLI (after any model path patch so it chains on effective PMM;
# MMDDumper-independent). If any frame range param supplied (ContainsKey for int fields to support explicit 0):
# build only supplied CLI options, default output under target/pmm-gui-smoke/*_frame_range_patched.pmm (or user -FrameRangePatchedPmmPath),
# run cargo, capture to frameRangePatch.*, on success update effectivePmmResolved/pmmToUse for downstream non-GUI inspect/compare/GUI.
# On failure: JSON failure + exit (no GUI). No params supplied => unchanged behavior (and model patch unaffected).
if ($frameRangePatchRequested) {
    $report.frameRangePatch.attempted = $true
    $frameInput = $effectivePmmResolved
    $report.frameRangePatch.inputPath = $frameInput
    $cargoFrameArgs = @(
        "run", "-p", "mmd-anim-cli", "--",
        "patch-pmm-scene-frame-range",
        $frameInput
    )
    $cargoFrameOptionArgs = @()
    $suppliedOpts = @()
    if ($PSBoundParameters.ContainsKey('PatchCurrentFrame')) {
        $report.frameRangePatch.currentFrame = $PatchCurrentFrame
        $cargoFrameOptionArgs += "--current-frame", [string]$PatchCurrentFrame
        $suppliedOpts += "currentFrame=$PatchCurrentFrame"
    }
    if ($PSBoundParameters.ContainsKey('PatchCurrentFrameText')) {
        $report.frameRangePatch.currentFrameText = $PatchCurrentFrameText
        $cargoFrameOptionArgs += "--current-frame-text", [string]$PatchCurrentFrameText
        $suppliedOpts += "currentFrameText=$PatchCurrentFrameText"
    }
    if ($PSBoundParameters.ContainsKey('PatchBeginFrame')) {
        $report.frameRangePatch.beginFrame = $PatchBeginFrame
        $cargoFrameOptionArgs += "--begin-frame", [string]$PatchBeginFrame
        $suppliedOpts += "beginFrame=$PatchBeginFrame"
    }
    if ($PSBoundParameters.ContainsKey('PatchEndFrame')) {
        $report.frameRangePatch.endFrame = $PatchEndFrame
        $cargoFrameOptionArgs += "--end-frame", [string]$PatchEndFrame
        $suppliedOpts += "endFrame=$PatchEndFrame"
    }
    if (-not [string]::IsNullOrWhiteSpace($PatchBeginFrameEnabled)) {
        $report.frameRangePatch.beginFrameEnabled = $PatchBeginFrameEnabled
        $cargoFrameOptionArgs += "--begin-frame-enabled", $PatchBeginFrameEnabled
        $suppliedOpts += "beginFrameEnabled=$PatchBeginFrameEnabled"
    }
    if (-not [string]::IsNullOrWhiteSpace($PatchEndFrameEnabled)) {
        $report.frameRangePatch.endFrameEnabled = $PatchEndFrameEnabled
        $cargoFrameOptionArgs += "--end-frame-enabled", $PatchEndFrameEnabled
        $suppliedOpts += "endFrameEnabled=$PatchEndFrameEnabled"
    }
    $report.frameRangePatch.options = if ($suppliedOpts.Count -gt 0) { ($suppliedOpts -join ";") } else { $null }

    # Compute output (default under target/pmm-gui-smoke/ with clear suffix; relative resolved to repo root)
    $framePatchOutDir = Join-Path $repoRoot "target/pmm-gui-smoke"
    New-Item -ItemType Directory -Force -Path $framePatchOutDir | Out-Null
    $frameOutPath = $FrameRangePatchedPmmPath
    if ([string]::IsNullOrWhiteSpace($frameOutPath)) {
        $baseName = [IO.Path]::GetFileNameWithoutExtension($PmmPath)
        $frameOutPath = Join-Path $framePatchOutDir ("{0}_frame_range_patched.pmm" -f $baseName)
    } else {
        if (-not [IO.Path]::IsPathRooted($frameOutPath)) {
            $frameOutPath = Join-Path $repoRoot $frameOutPath
        }
        $parent = Split-Path -Parent $frameOutPath
        if ($parent) { New-Item -ItemType Directory -Force -Path $parent | Out-Null }
    }
    $report.frameRangePatch.outputPath = $frameOutPath
    $cargoFrameArgs += $frameOutPath
    $cargoFrameArgs += $cargoFrameOptionArgs

    if (-not $Json) {
        Write-Host "Patching PMM scene frame range (CLI): cargo run -p mmd-anim-cli -- patch-pmm-scene-frame-range <effective> <out> [opts]"
    }
    try {
        Push-Location $repoRoot
        try {
            $frameRaw = & cargo @cargoFrameArgs 2>&1
            $frameEx = $LASTEXITCODE
        } finally {
            Pop-Location
        }
        $frameText = ($frameRaw | Out-String).Trim()
        $report.frameRangePatch.exit = $frameEx
        $report.frameRangePatch.output = $frameText
        $report.frameRangePatch.success = ($frameEx -eq 0)
        if (-not $Json) {
            Write-Host "== patch-pmm-scene-frame-range (exit=$frameEx) =="
            $frameRaw | ForEach-Object { Write-Host $_ }
        }
        if ($frameEx -eq 0) {
            $patchedFrameResolved = Resolve-Safe $frameOutPath
            if ($patchedFrameResolved -and (Test-Path -LiteralPath $patchedFrameResolved -PathType Leaf)) {
                $report.frameRangePatch.outputPathResolved = $patchedFrameResolved
                $effectivePmmResolved = $patchedFrameResolved
                $pmmToUse = $patchedFrameResolved
                $report.effectivePmm = $frameOutPath
                $report.effectivePmmResolved = $patchedFrameResolved
            } else {
                $report.frameRangePatch.success = $false
                $report.status = "failure"
                $report.summary = "Patch CLI (frame range) reported success but patched file missing at $frameOutPath"
                if ($Json) { $report | ConvertTo-Json -Depth 6 }
                exit 2
            }
        } else {
            $report.status = "failure"
            $report.summary = "patch-pmm-scene-frame-range failed (exit=$frameEx)"
            if ($Json) {
                $report | ConvertTo-Json -Depth 6
            }
            exit 2
        }
    } catch {
        $report.frameRangePatch.exit = 1
        $report.frameRangePatch.output = $_.ToString()
        $report.frameRangePatch.success = $false
        $report.status = "failure"
        $report.summary = "frame range patch invocation threw: $($_.Exception.Message)"
        if ($Json) { $report | ConvertTo-Json -Depth 6 }
        exit 2
    }
}

$report.effectivePmmResolved = $effectivePmmResolved

# Resolve dumper (best effort; independent of MMDDumper when not present)
$droot = Resolve-Safe $DumperRoot
$cliMjs = $null
$smokeMjs = $null
if ($droot) {
    $cliMjs   = Join-Path $droot "src/cli.mjs"
    $smokeMjs = Join-Path $droot "scripts/mmd-first-load-smoke.mjs"
    if ((Test-Path -LiteralPath $cliMjs -PathType Leaf) -and (Test-Path -LiteralPath $smokeMjs -PathType Leaf)) {
        $report.dumperRoot      = $droot
        $report.dumperAvailable = $true
    } else {
        $cliMjs = $null
        $smokeMjs = $null
    }
}

# Non-GUI: print/validate embedded model path policy via inspect (if dumper feasible)
if ($report.dumperAvailable) {
    try {
        $nodeArgs = @($cliMjs, "inspect-pmm-model-slots", $pmmToUse)
        $out = & node @nodeArgs 2>&1
        $ex = $LASTEXITCODE
        $report.nonGuiSteps += [PSCustomObject]@{
            step   = "inspect-pmm-model-slots"
            exit   = $ex
            output = ($out | Out-String).Trim()
        }
        if ($ex -ne 0) {
            $nonGuiFailed = $true
        }
        if (-not $Json) {
            Write-Host "== MMDDumper inspect-pmm-model-slots (embedded model path policy) =="
            $out | ForEach-Object { Write-Host $_ }
        }
    } catch {
        $report.nonGuiSteps += [PSCustomObject]@{
            step  = "inspect-pmm-model-slots"
            exit  = 1
            error = $_.ToString()
        }
        $nonGuiFailed = $true
    }
} else {
    $report.nonGuiSteps += [PSCustomObject]@{
        step    = "inspect-pmm-model-slots"
        skipped = "dumper not available at $DumperRoot (MMDDumper-independent path)"
    }
    if (-not $Json) {
        Write-Host "MMDDumper not available; skipping non-GUI model path inspect (PMM existence already validated)."
    }
}

# Optionally run MMDDumper compare against repo-local (or supplied) VMD fixture when both present
$vmdResolved = $null
if ($VmdPath) {
    $vmdResolved = Resolve-Safe $VmdPath
} else {
    $candidate = Join-Path $repoRoot "crates/mmd-anim-format/fixtures/vmd/ik_multi_bone_nondefault.vmd"
    $vmdResolved = Resolve-Safe $candidate
}
$report.vmdResolved = $vmdResolved

if ($vmdResolved -and $report.dumperAvailable) {
    try {
        $nodeArgs = @($cliMjs, "compare-pmm-document-vmd-keyframes", "--pmm", $pmmToUse, "--vmd", $vmdResolved)
        $out = & node @nodeArgs 2>&1
        $ex = $LASTEXITCODE
        $report.nonGuiSteps += [PSCustomObject]@{
            step   = "compare-pmm-document-vmd-keyframes"
            exit   = $ex
            vmd    = $vmdResolved
            output = ($out | Out-String).Trim()
        }
        if ($ex -ne 0) {
            $nonGuiFailed = $true
        }
        if (-not $Json) {
            Write-Host "== MMDDumper compare-pmm-document-vmd-keyframes =="
            $out | ForEach-Object { Write-Host $_ }
        }
    } catch {
        $report.nonGuiSteps += [PSCustomObject]@{
            step  = "compare-pmm-document-vmd-keyframes"
            exit  = 1
            vmd   = $vmdResolved
            error = $_.ToString()
        }
        $nonGuiFailed = $true
    }
} elseif ($vmdResolved) {
    $report.nonGuiSteps += [PSCustomObject]@{
        step    = "compare-pmm-document-vmd-keyframes"
        skipped = "dumper not available (VMD present at $vmdResolved)"
    }
}

# GUI decision: explicit switch + env guard required. Never launch by default.
$guiReason = "default-no-launch"
$doLaunch = $false
if ($LaunchGui) {
    if ($env:MMD_DUMPER_ALLOW_MMD_LAUNCH -eq "1") {
        if ($report.dumperAvailable) {
            $doLaunch = $true
        } else {
            $guiReason = "LaunchGui supplied but dumper not available"
        }
    } else {
        $guiReason = "LaunchGui supplied but MMD_DUMPER_ALLOW_MMD_LAUNCH != 1"
    }
} else {
    $guiReason = "no -LaunchGui switch (explicit manual step required)"
}
$report.gui.reason = $guiReason

if ($doLaunch) {
    $report.gui.attempted = $true
    # Prepare a minimal fixture that points project at the (resolved absolute) PMM.
    # This delegates to the existing MMDDumper first-load smoke without modifying MMDDumper.
    $artDir = Join-Path $repoRoot ".ai/grok-subagent/pmm-gui-smoke"
    New-Item -ItemType Directory -Force -Path $artDir | Out-Null
    $baseName = [IO.Path]::GetFileNameWithoutExtension($pmmToUse)
    $fxPath = Join-Path $artDir ("fixture-" + $baseName + "-gui-smoke.json")
    $fxObj = [PSCustomObject]@{
        name        = ("pmm-gui-smoke-" + $baseName)
        mmdVersion  = "9.32-x64"
        project     = $pmmToUse
        frames      = @(0, 30)
        dump        = @{ bones = $true; morphs = $true; rigidBodies = $false }
        output      = "oracle.actual.jsonl"
        timeoutMs   = $GuiTimeoutMs
    }
    $fxJson = $fxObj | ConvertTo-Json -Depth 5
    [System.IO.File]::WriteAllText(
        $fxPath,
        $fxJson,
        (New-Object System.Text.UTF8Encoding($false))
    )
    $report.gui.preparedFixture = $fxPath

    if (-not $Json) {
        Write-Host "MMD_DUMPER_ALLOW_MMD_LAUNCH=1 and -LaunchGui: delegating to MMDDumper GUI-capable command..."
        Write-Host "  fixture: $fxPath"
        Write-Host "  project: $pmmToUse"
    }

    try {
        $nodeArgs = @($smokeMjs, "--fixture", $fxPath, "--timeout-ms", [string]$GuiTimeoutMs)
        $out = & node @nodeArgs 2>&1
        $ex = $LASTEXITCODE
        $report.gui.launched = $true
        $report.gui.exit = $ex
        $tail = ($out | Out-String).Trim()
        if ($tail.Length -gt 4000) { $tail = $tail.Substring(0, 4000) + "`n... (truncated)" }
        $report.gui.outputTail = $tail
        $report.status = if ($ex -eq 0) { "success" } else { "failure" }
        $report.summary = "GUI smoke delegated; exit=$ex"
        if (-not $Json) {
            Write-Host "== MMDDumper mmd-first-load-smoke (GUI) output tail =="
            Write-Host $tail
        }
    } catch {
        $report.gui.launched = $true
        $report.gui.exit = 1
        $report.gui.outputTail = $_.ToString()
        $report.status = "failure"
        $report.summary = "GUI delegation threw: $($_.Exception.Message)"
    }
} else {
    $report.gui.attempted = $false
    $report.gui.launched = $false
    if ($nonGuiFailed) {
        $report.status = "failure"
        $report.summary = "GUI skipped: $guiReason; non-GUI prep failed"
    } else {
        $report.status = if ($report.status -eq "failure") { "failure" } else { "skipped" }
        $report.summary = "GUI skipped: $guiReason (non-GUI prep completed)"
    }
}

if (-not $Json) {
    $pmmLabel = if ($report.effectivePmmResolved -and ($report.effectivePmmResolved -ne $pmm)) { "$pmm -> effective=$($report.effectivePmmResolved)" } else { [string]$pmm }
    Write-Host ("PMM GUI smoke: pmm={0} exists={1} dumper={2} status={3}" -f $pmmLabel, $pmmExists, $report.dumperAvailable, $report.status)
    Write-Host ("  gui: attempted={0} launched={1} reason={2}" -f $report.gui.attempted, $report.gui.launched, $report.gui.reason)
    if ($report.gui.preparedFixture) {
        Write-Host "  preparedFixture: $($report.gui.preparedFixture)"
    }
    if ($report.patch -and $report.patch.requested) {
        Write-Host ("  patch: requested={0} success={1} out={2}" -f $report.patch.requested, $report.patch.success, $report.patch.outputPathResolved)
    }
    if ($report.frameRangePatch -and $report.frameRangePatch.requested) {
        Write-Host ("  frameRangePatch: requested={0} success={1} out={2}" -f $report.frameRangePatch.requested, $report.frameRangePatch.success, $report.frameRangePatch.outputPathResolved)
    }
    Write-Host ("  summary: {0}" -f $report.summary)
}

if ($Json) {
    $report | ConvertTo-Json -Depth 6
}

if ($report.status -eq "failure") {
    exit 1
}
exit 0
