param(
  [Parameter(Mandatory = $true)][string]$MmdExe,
  [Parameter(Mandatory = $true)][string]$Project,
  [Parameter(Mandatory = $true)][string]$Output,
  [int]$StartFrame = 0,
  [int]$EndFrame = 30,
  [int]$Fps = 30,
  [int]$LoadWaitMs = 5000,
  [int]$TimeoutMs = 120000
)

$ErrorActionPreference = "Stop"

$typeDef = @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class MmdAviExportWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr hwndCtl);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@
Add-Type -TypeDefinition $typeDef

function Get-WindowTextValue($handle) {
  $builder = [Text.StringBuilder]::new(512)
  [void][MmdAviExportWin32]::GetWindowText($handle, $builder, $builder.Capacity)
  $builder.ToString()
}

function Get-WindowClassValue($handle) {
  $builder = [Text.StringBuilder]::new(256)
  [void][MmdAviExportWin32]::GetClassName($handle, $builder, $builder.Capacity)
  $builder.ToString()
}

function Get-TopWindowsByPid([int]$targetProcessId) {
  $items = New-Object System.Collections.Generic.List[object]
  [MmdAviExportWin32]::EnumWindows({
    param($handle, $lParam)
    [uint32]$windowPid = 0
    [void][MmdAviExportWin32]::GetWindowThreadProcessId($handle, [ref]$windowPid)
    if ($windowPid -eq $targetProcessId -and [MmdAviExportWin32]::IsWindowVisible($handle)) {
      $items.Add([pscustomobject]@{
        Handle = $handle
        Class = Get-WindowClassValue $handle
        Text = Get-WindowTextValue $handle
      })
    }
    $true
  }, [IntPtr]::Zero) | Out-Null
  $items
}

function Get-ChildControls($parent) {
  $items = New-Object System.Collections.Generic.List[object]
  [MmdAviExportWin32]::EnumChildWindows($parent, {
    param($handle, $lParam)
    $rect = New-Object MmdAviExportWin32+RECT
    [void][MmdAviExportWin32]::GetWindowRect($handle, [ref]$rect)
    $items.Add([pscustomobject]@{
      Handle = $handle
      Class = Get-WindowClassValue $handle
      Text = Get-WindowTextValue $handle
      Id = [MmdAviExportWin32]::GetDlgCtrlID($handle)
      Top = $rect.Top
      Width = $rect.Right - $rect.Left
    })
    $true
  }, [IntPtr]::Zero) | Out-Null
  $items
}

function Wait-Until($timeoutMs, [scriptblock]$probe) {
  $deadline = (Get-Date).AddMilliseconds($timeoutMs)
  while ((Get-Date) -lt $deadline) {
    $value = & $probe
    if ($null -ne $value) {
      return $value
    }
    Start-Sleep -Milliseconds 200
  }
  return $null
}

function Set-ControlText($children, [int]$id, [string]$text) {
  $control = $children | Where-Object { $_.Id -eq $id } | Select-Object -First 1
  if (-not $control) {
    throw "MMD AVI settings control not found: $id"
  }
  [void][MmdAviExportWin32]::SendMessage($control.Handle, 0x000C, [IntPtr]::Zero, $text)
}

if ($env:MMD_DUMPER_ALLOW_MMD_LAUNCH -ne "1") {
  throw "Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run."
}
if ($StartFrame -lt 0 -or $EndFrame -lt $StartFrame) {
  throw "Invalid AVI frame range: $StartFrame..$EndFrame"
}

$mmdExePath = (Resolve-Path -LiteralPath $MmdExe).Path
$projectPath = (Resolve-Path -LiteralPath $Project).Path
$outputPath = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Output)
$outputDir = Split-Path -Parent $outputPath
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
Remove-Item -LiteralPath $outputPath -Force -ErrorAction SilentlyContinue

$process = Start-Process -FilePath $mmdExePath -ArgumentList @($projectPath) -WorkingDirectory (Split-Path -Parent $mmdExePath) -PassThru
try {
  $mainWindow = Wait-Until $TimeoutMs {
    Get-TopWindowsByPid $process.Id | Where-Object { $_.Class -eq "Polygon Movie Maker" } | Select-Object -First 1
  }
  if (-not $mainWindow) {
    throw "MMD main window not found."
  }

  [void][MmdAviExportWin32]::SetForegroundWindow($mainWindow.Handle)
  Start-Sleep -Milliseconds $LoadWaitMs

  [void][MmdAviExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]223, [IntPtr]::Zero)
  $saveDialog = Wait-Until 10000 {
    Get-TopWindowsByPid $process.Id |
      Where-Object { $_.Class -eq "#32770" -and $_.Text -like "*AVI出力" } |
      Select-Object -First 1
  }
  if (-not $saveDialog) {
    throw "MMD AVI save dialog not found."
  }

  $saveChildren = Get-ChildControls $saveDialog.Handle
  $fileNameEdit = $saveChildren |
    Where-Object { $_.Class -eq "Edit" -and $_.Width -gt 100 } |
    Sort-Object Top -Descending |
    Select-Object -First 1
  if (-not $fileNameEdit) {
    throw "MMD AVI filename edit was not found."
  }
  [void][MmdAviExportWin32]::SendMessage($fileNameEdit.Handle, 0x000C, [IntPtr]::Zero, $outputPath)

  $saveButton = $saveChildren |
    Where-Object { $_.Class -eq "Button" -and $_.Text -like "*保存*" } |
    Select-Object -First 1
  if (-not $saveButton) {
    throw "MMD AVI save button was not found."
  }
  [void][MmdAviExportWin32]::SendMessage($saveButton.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)

  $settingsDialog = Wait-Until 10000 {
    Get-TopWindowsByPid $process.Id |
      Where-Object { $_.Class -eq "#32770" -and $_.Text -eq "AVI出力設定" } |
      Select-Object -First 1
  }
  if (-not $settingsDialog) {
    throw "MMD AVI settings dialog not found."
  }

  $settingsChildren = Get-ChildControls $settingsDialog.Handle
  Set-ControlText $settingsChildren 611 $Fps
  Set-ControlText $settingsChildren 609 $StartFrame
  Set-ControlText $settingsChildren 610 $EndFrame
  $okButton = $settingsChildren |
    Where-Object { $_.Class -eq "Button" -and $_.Id -eq 1 } |
    Select-Object -First 1
  if (-not $okButton) {
    throw "MMD AVI settings OK button was not found."
  }
  [void][MmdAviExportWin32]::SendMessage($okButton.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)

  $stableTicks = 0
  $lastLength = -1
  $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
  $created = $null
  while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 1
    if (-not (Test-Path -LiteralPath $outputPath)) {
      continue
    }
    $item = Get-Item -LiteralPath $outputPath
    if ($item.Length -gt 1024 -and $item.Length -eq $lastLength) {
      $stableTicks += 1
    } else {
      $stableTicks = 0
      $lastLength = $item.Length
    }
    if ($stableTicks -ge 3) {
      $created = $item
      break
    }
  }
  if (-not $created) {
    throw "MMD AVI export did not finish: $outputPath"
  }

  [pscustomobject]@{
    ok = $true
    output = $created.FullName
    length = $created.Length
    startFrame = $StartFrame
    endFrame = $EndFrame
    fps = $Fps
  } | ConvertTo-Json -Compress
} finally {
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force
  }
}
