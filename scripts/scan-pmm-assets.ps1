<#
.SYNOPSIS
  Lightweight PMM corpus scan gate. Uses parse-format-summary for fast aggregate checks in release gates.
  Compact mode omits per-file success records.
#>
param(
    [string]$Root = ".",
    [switch]$Json,
    [switch]$Full,
    [string]$ManifestPath
)

$ErrorActionPreference = "Stop"

$scriptDir = $null
if ($PSScriptRoot) {
    $scriptDir = $PSScriptRoot
} else {
    $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
}
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $scriptDir "..")).Path

if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
    $manifest = Join-Path $repoRoot "Cargo.toml"
} else {
    $manifest = $ManifestPath
}
if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
    $resolvedManifest = $null
    try {
        $resolvedManifest = (Resolve-Path -LiteralPath $manifest).Path
    } catch {}
    if ($resolvedManifest -and (Test-Path -LiteralPath $resolvedManifest -PathType Leaf)) {
        $manifest = $resolvedManifest
    } else {
        throw "Manifest not found: $manifest"
    }
} else {
    $manifest = (Resolve-Path -LiteralPath $manifest).Path
}

$scanRoot = $null
$rootMissing = $false
try {
    if (Test-Path -LiteralPath $Root -PathType Container) {
        $scanRoot = (Resolve-Path -LiteralPath $Root).Path
    } else {
        $rootMissing = $true
    }
} catch {
    $rootMissing = $true
}

if ($rootMissing) {
    if ($Json) {
        [PSCustomObject]@{
            root = $Root
            filesScanned = 0
            filesOk = 0
            filesFailed = 0
            error = "root missing"
        } | ConvertTo-Json -Depth 3
    } else {
        Write-Host "PMM scan: root missing: $Root"
    }
    exit 2
}

$pmmFiles = @(Get-ChildItem -LiteralPath $scanRoot -Recurse -File -Filter *.pmm -ErrorAction SilentlyContinue |
    Where-Object {
        $_.Name -notlike '._*' -and
        $_.FullName -notmatch '\\__MACOSX\\'
    } |
    Sort-Object FullName
)
$filesScanned = $pmmFiles.Count

$filesOk = 0
$filesFailed = 0
$diagnosticsTotal = 0
$referencesTotal = 0
$documentModelsTotal = 0
$documentBoneKeyframesTotal = 0
$documentMorphKeyframesTotal = 0
$byVersion = @{}
$failures = @()
$records = @()

function Invoke-ParseSummary([string]$pmmPath, [string]$manifestPath) {
    $cargoArgs = @("run", "--quiet", "--manifest-path", $manifestPath, "-p", "mmd-anim-cli", "--", "parse-format-summary", $pmmPath)
    $allOutput = @()
    $exit = 0
    try {
        $allOutput = & cargo @cargoArgs 2>&1
        $exit = $LASTEXITCODE
    } catch {
        $exit = 1
        $allOutput = @($_.ToString())
    }
    [PSCustomObject]@{
        ExitCode = $exit
        Output = ($allOutput | Out-String).Trim()
        Lines = $allOutput
    }
}

function Parse-PmmSummaryLine([string]$line) {
    $map = @{}
    if ([string]::IsNullOrWhiteSpace($line)) { return $map }
    $re = [regex]'([A-Za-z0-9_]+)=([^\s]+)'
    foreach ($m in $re.Matches($line)) {
        $k = $m.Groups[1].Value
        $v = $m.Groups[2].Value
        if (-not $map.ContainsKey($k)) {
            $map[$k] = $v
        }
    }
    $map
}

foreach ($f in $pmmFiles) {
    $res = Invoke-ParseSummary $f.FullName $manifest
    $ok = $false
    $parsed = $null
    $message = ""
    if ($res.ExitCode -ne 0) {
        $message = "exit=$($res.ExitCode)"
        if ($res.Output) { $message += " output=" + ($res.Output -replace "`r?`n", " ") }
    } else {
        $pmmLine = $null
        foreach ($ln in $res.Lines) {
            if ($ln -match "PMM parser:") {
                $pmmLine = $ln
                break
            }
        }
        if ($pmmLine) {
            $parsed = Parse-PmmSummaryLine $pmmLine
            $ok = $true
        } else {
            $message = "no PMM parser line"
            if ($res.Output) { $message += ": " + ($res.Output -replace "`r?`n", " ") }
        }
    }

    if ($ok -and $parsed) {
        $filesOk++
        $refs = 0
        $diags = 0
        $dModels = 0
        $dBoneKf = 0
        $dMorphKf = 0
        if ($parsed.ContainsKey("references")) { [void][int]::TryParse($parsed["references"], [ref]$refs) }
        if ($parsed.ContainsKey("diagnostics")) { [void][int]::TryParse($parsed["diagnostics"], [ref]$diags) }
        $dmStr = if ($parsed.ContainsKey("documentModels")) { $parsed["documentModels"] } else { "" }
        if ($dmStr -and $dmStr -ne "unknown") { [void][int]::TryParse($dmStr, [ref]$dModels) }
        $dbStr = if ($parsed.ContainsKey("documentBoneKeyframes")) { $parsed["documentBoneKeyframes"] } else { "" }
        if ($dbStr -and $dbStr -ne "unknown") { [void][int]::TryParse($dbStr, [ref]$dBoneKf) }
        $dmfStr = if ($parsed.ContainsKey("documentMorphKeyframes")) { $parsed["documentMorphKeyframes"] } else { "" }
        if ($dmfStr -and $dmfStr -ne "unknown") { [void][int]::TryParse($dmfStr, [ref]$dMorphKf) }

        $referencesTotal += $refs
        $diagnosticsTotal += $diags
        $documentModelsTotal += $dModels
        $documentBoneKeyframesTotal += $dBoneKf
        $documentMorphKeyframesTotal += $dMorphKf

        $ver = if ($parsed.ContainsKey("version") -and $parsed["version"]) { $parsed["version"] } else { "unknown" }
        if ($byVersion.ContainsKey($ver)) {
            $byVersion[$ver] = $byVersion[$ver] + 1
        } else {
            $byVersion[$ver] = 1
        }

        if ($Full) {
            $records += [PSCustomObject]@{
                path = $f.FullName
                version = $ver
                parsedVersion = if ($parsed.ContainsKey("parsedVersion")) { $parsed["parsedVersion"] } else { $null }
                references = $refs
                diagnostics = $diags
                documentModels = $dModels
                documentBoneKeyframes = $dBoneKf
                documentMorphKeyframes = $dMorphKf
                ok = $true
            }
        }
    } else {
        $filesFailed++
        $failures += [PSCustomObject]@{
            path = $f.FullName
            message = $message
        }
        if ($Full) {
            $records += [PSCustomObject]@{
                path = $f.FullName
                ok = $false
                message = $message
            }
        }
    }
}

$report = [PSCustomObject]@{
    root = $scanRoot
    filesScanned = $filesScanned
    filesOk = $filesOk
    filesFailed = $filesFailed
    diagnosticsTotal = $diagnosticsTotal
    referencesTotal = $referencesTotal
    documentModelsTotal = $documentModelsTotal
    documentBoneKeyframesTotal = $documentBoneKeyframesTotal
    documentMorphKeyframesTotal = $documentMorphKeyframesTotal
    byVersion = $byVersion
    failures = $failures
}
if ($Full) {
    $report | Add-Member -NotePropertyName records -NotePropertyValue $records
}

if ($Json) {
    $report | ConvertTo-Json -Depth 5
} else {
    Write-Host ("PMM scan root={0} scanned={1} ok={2} failed={3}" -f $report.root, $filesScanned, $filesOk, $filesFailed)
    Write-Host ("  diagnosticsTotal={0} referencesTotal={1}" -f $diagnosticsTotal, $referencesTotal)
    Write-Host ("  documentModelsTotal={0} documentBoneKeyframesTotal={1} documentMorphKeyframesTotal={2}" -f $documentModelsTotal, $documentBoneKeyframesTotal, $documentMorphKeyframesTotal)
    Write-Host "  byVersion:"
    foreach ($k in ($byVersion.Keys | Sort-Object)) {
        Write-Host ("    {0}: {1}" -f $k, $byVersion[$k])
    }
    if ($failures.Count -gt 0) {
        Write-Host "  failures:"
        foreach ($f in $failures) {
            Write-Host ("    {0}: {1}" -f $f.path, $f.message)
        }
    } else {
        Write-Host "  failures: none"
    }
}

if ($filesFailed -gt 0) {
    exit 1
}
exit 0
