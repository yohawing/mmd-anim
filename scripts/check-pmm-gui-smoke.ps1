<#
.SYNOPSIS
  MMDDumper-independent non-GUI contract check for scripts/pmm-gui-smoke.ps1.
  - Forces invalid/nonexistent -DumperRoot on every invocation of pmm-gui-smoke.
  - Clears/ignores MMD_DUMPER_ALLOW_MMD_LAUNCH (never allows GUI launch).
  - Runs default-safe path (no -LaunchGui) and -LaunchGui path while env unset.
  - Never launches MMD under any circumstances.
  - Parses emitted JSON and asserts the guarded harness contract using only repo-local PMM existence.
  - Also asserts missing PMM path yields failure status (no dumper dependency).
#>
param()

$ErrorActionPreference = "Stop"

$scriptDir = $null
if ($PSScriptRoot) {
    $scriptDir = $PSScriptRoot
} else {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
}
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path

$pmmFixture = Join-Path $repoRoot "crates/mmd-anim-format/fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm"
$invalidDumper = "C:\__NO_MMDDUMPER_FOR_CONTRACT_CHECK__"
$artDir = Join-Path $repoRoot ".ai/grok-subagent/check-pmm-gui-smoke"
New-Item -ItemType Directory -Force -Path $artDir | Out-Null

function Invoke-SmokeWithArtifact {
    param(
        [Parameter(Mandatory=$true)][string]$Pmm,
        [string]$DumperRoot = $invalidDumper,
        [switch]$LaunchGui,
        [string]$PatchModelPath,
        [int]$PatchDocumentModelIndex = 0,
        [string]$PatchedPmmPath,
        [int]$PatchCurrentFrame,
        [int]$PatchCurrentFrameText,
        [int]$PatchBeginFrame,
        [int]$PatchEndFrame,
        [string]$PatchBeginFrameEnabled,
        [string]$PatchEndFrameEnabled,
        [string]$FrameRangePatchedPmmPath,
        [Parameter(Mandatory=$true)][string]$Name
    )
    # Force clear/ignore the launch guard env for this verification (and sub process)
    $env:MMD_DUMPER_ALLOW_MMD_LAUNCH = $null
    Remove-Item Env:\MMD_DUMPER_ALLOW_MMD_LAUNCH -ErrorAction SilentlyContinue | Out-Null

    $smokePath = Join-Path $repoRoot "scripts/pmm-gui-smoke.ps1"
    $psArgs = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $smokePath,
        $Pmm,
        "-DumperRoot", $DumperRoot,
        "-Json"
    )
    if ($LaunchGui) { $psArgs += "-LaunchGui" }
    if ($PatchModelPath) {
        $psArgs += "-PatchModelPath", $PatchModelPath
        if ($PatchDocumentModelIndex -ne 0) {
            $psArgs += "-PatchDocumentModelIndex", [string]$PatchDocumentModelIndex
        }
        if ($PatchedPmmPath) {
            $psArgs += "-PatchedPmmPath", $PatchedPmmPath
        }
    }

    # Forward frame range patch params (use ContainsKey on Invoke bound params so explicit 0 can be passed through to prove top-level $PSBoundParameters handling in pmm-gui-smoke)
    if ($PSBoundParameters.ContainsKey('PatchCurrentFrame')) {
        $psArgs += "-PatchCurrentFrame", [string]$PatchCurrentFrame
    }
    if ($PSBoundParameters.ContainsKey('PatchCurrentFrameText')) {
        $psArgs += "-PatchCurrentFrameText", [string]$PatchCurrentFrameText
    }
    if ($PSBoundParameters.ContainsKey('PatchBeginFrame')) {
        $psArgs += "-PatchBeginFrame", [string]$PatchBeginFrame
    }
    if ($PSBoundParameters.ContainsKey('PatchEndFrame')) {
        $psArgs += "-PatchEndFrame", [string]$PatchEndFrame
    }
    if ($PSBoundParameters.ContainsKey('PatchBeginFrameEnabled')) {
        $psArgs += "-PatchBeginFrameEnabled", $PatchBeginFrameEnabled
    }
    if ($PSBoundParameters.ContainsKey('PatchEndFrameEnabled')) {
        $psArgs += "-PatchEndFrameEnabled", $PatchEndFrameEnabled
    }
    if ($PSBoundParameters.ContainsKey('FrameRangePatchedPmmPath') -and -not [string]::IsNullOrWhiteSpace($FrameRangePatchedPmmPath)) {
        $psArgs += "-FrameRangePatchedPmmPath", $FrameRangePatchedPmmPath
    }

    $raw = & pwsh @psArgs 2>&1
    $text = ($raw | Out-String)
    $artPath = Join-Path $artDir ("$Name.json")
    # Save raw output for traceability (even if json)
    $text | Set-Content -LiteralPath $artPath -Encoding utf8

    $trimmed = $text.Trim()
    try {
        $report = $trimmed | ConvertFrom-Json -ErrorAction Stop
        return [PSCustomObject]@{ report = $report; artifact = $artPath }
    } catch {
        # Attempt to extract a JSON object blob if extra noise present
        if ($trimmed -match '(?s)(\{.*\})$') {
            $report = $matches[1] | ConvertFrom-Json
            return [PSCustomObject]@{ report = $report; artifact = $artPath }
        }
        throw "Failed to parse JSON output from pmm-gui-smoke for ${Name}: `n$trimmed"
    }
}

Write-Host "== pmm-gui-smoke contract check (MMDDumper-independent, non-GUI only) =="
Write-Host "repoRoot     : $repoRoot"
Write-Host "pmmFixture   : $pmmFixture"
Write-Host "invalidDumper: $invalidDumper"
Write-Host "artifacts    : $artDir"
Write-Host ""

$failures = @()

function Assert {
    param([bool]$Condition, [string]$Message)
    if ($Condition) {
        Write-Host "PASS: $Message"
    } else {
        Write-Host "FAIL: $Message" -ForegroundColor Red
        $script:failures += $Message
    }
}

# --- Case 1: repo-local PMM, default-safe (no -LaunchGui), invalid dumper ---
Write-Host "--- Case 1: safe default (no -LaunchGui) with repo PMM ---"
$c1 = Invoke-SmokeWithArtifact -Pmm $pmmFixture -Name "case1-safe-no-launch"
$r1 = $c1.report
Write-Host "  status=$($r1.status) pmmExists=$($r1.pmmExists) dumperAvailable=$($r1.dumperAvailable)"
Write-Host "  gui: attempted=$($r1.gui.attempted) launched=$($r1.gui.launched) reason=$($r1.gui.reason)"
Assert ($r1.pmmExists -eq $true) "pmmExists is true for the repo-local PMM fixture"
Assert ($r1.dumperAvailable -eq $false) "dumperAvailable is false when -DumperRoot is invalid"
Assert ($r1.status -eq "skipped") "status is skipped for safe runs (no -LaunchGui)"
Assert ($r1.gui.attempted -eq $false) "gui.attempted is false"
Assert ($r1.gui.launched -eq $false) "gui.launched is false"
Assert ($r1.gui.reason -like "*no -LaunchGui switch*") "gui.reason distinguishes default no-launch"

# --- Case 2: repo-local PMM, -LaunchGui supplied but env unset (forced), invalid dumper ---
Write-Host ""
Write-Host "--- Case 2: -LaunchGui with env unset (should still skip) ---"
$c2 = Invoke-SmokeWithArtifact -Pmm $pmmFixture -LaunchGui -Name "case2-launchgui-env-unset"
$r2 = $c2.report
Write-Host "  status=$($r2.status) pmmExists=$($r2.pmmExists) dumperAvailable=$($r2.dumperAvailable)"
Write-Host "  gui: attempted=$($r2.gui.attempted) launched=$($r2.gui.launched) reason=$($r2.gui.reason)"
Assert ($r2.pmmExists -eq $true) "pmmExists is true for the repo-local PMM fixture (LaunchGui case)"
Assert ($r2.dumperAvailable -eq $false) "dumperAvailable is false when -DumperRoot is invalid (LaunchGui case)"
Assert ($r2.status -eq "skipped") "status is skipped for -LaunchGui when env guard active"
Assert ($r2.gui.attempted -eq $false) "gui.attempted is false (LaunchGui + env unset)"
Assert ($r2.gui.launched -eq $false) "gui.launched is false (LaunchGui + env unset)"
Assert ($r2.gui.reason -like "*MMD_DUMPER_ALLOW_MMD_LAUNCH != 1*") "gui.reason distinguishes env guard vs default no-launch"

# --- Case 3: missing PMM path returns failure JSON (no dumper used) ---
Write-Host ""
Write-Host "--- Case 3: missing PMM path (failure without MMDDumper) ---"
$missing = Join-Path $repoRoot "this-pmm-does-not-exist-for-check.pmm"
$c3 = Invoke-SmokeWithArtifact -Pmm $missing -Name "case3-missing-pmm-failure"
$r3 = $c3.report
Write-Host "  status=$($r3.status) pmmExists=$($r3.pmmExists) dumperAvailable=$($r3.dumperAvailable)"
Assert ($r3.pmmExists -eq $false) "pmmExists is false for missing PMM path"
Assert ($r3.status -eq "failure") "missing PMM path returns a failure JSON status"
Assert ($r3.dumperAvailable -eq $false) "dumperAvailable is false for missing-PMM case (contract independent of MMDDumper)"
# Also ensure gui not attempted on early failure path
Assert ($r3.gui.attempted -eq $false) "gui.attempted is false for missing PMM failure"
Assert ($r3.gui.launched -eq $false) "gui.launched is false for missing PMM failure"

# --- Case 4: repo-local PMM + repo-local PMX patch, invalid dumper (MMDDumper-independent, no GUI) ---
Write-Host ""
Write-Host "--- Case 4: -PatchModelPath with repo PMM/PMX + invalid dumper (patch only, effective PMM) ---"
$pmxFixture = Join-Path $repoRoot "crates/mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx"
$patchedForCase4 = Join-Path $artDir "case4-ik_multi_axis_limit_patched.pmm"
$c4 = Invoke-SmokeWithArtifact -Pmm $pmmFixture -PatchModelPath $pmxFixture -PatchedPmmPath $patchedForCase4 -Name "case4-patch-with-invalid-dumper"
$r4 = $c4.report
Write-Host "  status=$($r4.status) pmmExists=$($r4.pmmExists) dumperAvailable=$($r4.dumperAvailable)"
Write-Host "  patch: requested=$($r4.patch.requested) success=$($r4.patch.success) attempted=$($r4.patch.attempted) modelResolved=$($r4.patch.modelPathResolved)"
Write-Host "  effectivePmmResolved=$($r4.effectivePmmResolved)"
Write-Host "  gui: attempted=$($r4.gui.attempted) launched=$($r4.gui.launched) reason=$($r4.gui.reason)"
Assert ($r4.pmmExists -eq $true) "pmmExists is true for the repo-local PMM fixture (patch case)"
Assert ($r4.dumperAvailable -eq $false) "dumperAvailable is false when -DumperRoot is invalid (patch case)"
Assert ($r4.patch -ne $null) "patch object present in report"
Assert ($r4.patch.requested -eq $true) "patch.requested is true when -PatchModelPath supplied"
Assert ($r4.patch.attempted -eq $true) "patch.attempted is true"
Assert ($r4.patch.success -eq $true) "patch.success is true (CLI patch applied)"
Assert ($r4.patch.modelPathResolved -like "*ik_multi_axis_limit.pmx") "patch.modelPathResolved resolves to the PMX fixture"
Assert ($r4.patch.outputPathResolved -ne $null -and (Test-Path -LiteralPath $r4.patch.outputPathResolved -PathType Leaf)) "patch.outputPathResolved exists as file"
Assert ($r4.status -eq "skipped") "status is skipped for safe (no -LaunchGui) run after patch"
Assert ($r4.gui.attempted -eq $false) "gui.attempted is false (patch path, no -LaunchGui)"
Assert ($r4.gui.launched -eq $false) "gui.launched is false"
Assert ($r4.effectivePmmResolved -ne $null) "effectivePmmResolved is present"
Assert ($r4.effectivePmmResolved -ne $r4.pmmResolved) "effectivePmmResolved differs from input pmmResolved (patched was used)"
Assert (Test-Path -LiteralPath $r4.effectivePmmResolved -PathType Leaf) "effectivePmmResolved exists as a file on disk"
Assert ($r4.effectivePmmResolved -like "*case4-ik_multi_axis_limit_patched.pmm*") "effectivePmmResolved matches the requested patched output"

# --- Case 5: repo-local PMM + frame range patch options (incl explicit 0 for int to prove PSBoundParameters), invalid dumper (MMDDumper-independent, non-GUI) ---
Write-Host ""
Write-Host "--- Case 5: frame range patch (explicit 0 for one int field) + invalid dumper (non-GUI, parse effective via cargo) ---"
$patchedForCase5 = Join-Path $artDir "case5-frame-range-patched.pmm"
$c5 = Invoke-SmokeWithArtifact -Pmm $pmmFixture `
    -PatchCurrentFrame 0 `
    -PatchCurrentFrameText 0 `
    -PatchBeginFrame 3 `
    -PatchEndFrame 99 `
    -PatchBeginFrameEnabled "true" `
    -PatchEndFrameEnabled "false" `
    -FrameRangePatchedPmmPath $patchedForCase5 `
    -Name "case5-frame-range-patch-with-invalid-dumper"
$r5 = $c5.report
Write-Host "  status=$($r5.status) pmmExists=$($r5.pmmExists) dumperAvailable=$($r5.dumperAvailable)"
Write-Host "  frameRangePatch: requested=$($r5.frameRangePatch.requested) attempted=$($r5.frameRangePatch.attempted) success=$($r5.frameRangePatch.success) input=$($r5.frameRangePatch.inputPath)"
Write-Host "  effectivePmmResolved=$($r5.effectivePmmResolved)"
Write-Host "  gui: attempted=$($r5.gui.attempted) launched=$($r5.gui.launched) reason=$($r5.gui.reason)"
Assert ($r5.pmmExists -eq $true) "pmmExists is true for the repo-local PMM fixture (frame range patch case)"
Assert ($r5.dumperAvailable -eq $false) "dumperAvailable is false when -DumperRoot is invalid (frame range patch case)"
Assert ($r5.frameRangePatch -ne $null) "frameRangePatch object present in report"
Assert ($r5.frameRangePatch.requested -eq $true) "frameRangePatch.requested is true when frame range params supplied (incl explicit 0)"
Assert ($r5.frameRangePatch.attempted -eq $true) "frameRangePatch.attempted is true"
Assert ($r5.frameRangePatch.success -eq $true) "frameRangePatch.success is true (CLI patch applied)"
Assert ($r5.status -eq "skipped") "status is skipped for safe (no -LaunchGui) run after frame range patch"
Assert ($r5.gui.attempted -eq $false) "gui.attempted is false (frame range patch path, no -LaunchGui)"
Assert ($r5.gui.launched -eq $false) "gui.launched is false"
Assert ($r5.effectivePmmResolved -ne $null) "effectivePmmResolved is present"
Assert ($r5.effectivePmmResolved -ne $r5.pmmResolved) "effectivePmmResolved differs from input pmmResolved (frame-range-patched was used)"
Assert (Test-Path -LiteralPath $r5.effectivePmmResolved -PathType Leaf) "effectivePmmResolved exists as a file on disk"
Assert ($r5.effectivePmmResolved -like "*case5-frame-range-patched.pmm*") "effectivePmmResolved matches the requested frame range patched output"

# MMDDumper-independent: parse the effective PMM via local cargo CLI and assert the patched frame values (projectGraph.sceneSettings)
$effForParse = $r5.effectivePmmResolved
$oldErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    $parseAll = & cargo run --quiet -p mmd-anim-cli -- parse-format-json $effForParse 2>&1
    $parseExitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $oldErrorActionPreference
}
Assert ($parseExitCode -eq 0) "parse-format-json succeeds for effective PMM"
$parseAllStr = $parseAll | ForEach-Object { [string]$_ }
$parseJoined = $parseAllStr -join "`n"
$parseJsonText = $null
if ($parseJoined -match "(?s)(\{.*\})$") { $parseJsonText = $matches[1] }
if (-not $parseJsonText) {
    $idx = $parseJoined.IndexOf("{")
    if ($idx -ge 0) { $parseJsonText = $parseJoined.Substring($idx) }
}
$parsed5 = $parseJsonText | ConvertFrom-Json
$scene5 = if ($parsed5.projectGraph -and $parsed5.projectGraph.sceneSettings) { $parsed5.projectGraph.sceneSettings } else { $null }
Assert ($scene5 -ne $null) "parsed projectGraph.sceneSettings present for effective PMM"
Assert ($scene5.currentFrameIndex -eq 0) "currentFrameIndex is patched to explicit 0"
Assert ($scene5.currentFrameIndexInTextField -eq 0) "currentFrameIndexInTextField is patched to explicit 0"
Assert ($scene5.beginFrameIndex -eq 3) "beginFrameIndex is patched to 3"
Assert ($scene5.endFrameIndex -eq 99) "endFrameIndex is patched to 99"
Assert ($scene5.beginFrameIndexEnabled -eq $true) "beginFrameIndexEnabled is patched to true"
Assert ($scene5.endFrameIndexEnabled -eq $false) "endFrameIndexEnabled is patched to false"

Write-Host ""
if ($failures.Count -eq 0) {
    Write-Host "ALL CONTRACT ASSERTS PASSED (non-GUI, MMDDumper-independent)" -ForegroundColor Green
    Write-Host "Raw JSON artifacts written under: $artDir"
    exit 0
} else {
    Write-Host "CONTRACT CHECK FAILED ($($failures.Count) assert(s))" -ForegroundColor Red
    $failures | ForEach-Object { Write-Host "  - $_" }
    exit 1
}
