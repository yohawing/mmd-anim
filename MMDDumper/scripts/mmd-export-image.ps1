param(
  [Parameter(Mandatory = $true)][string]$MmdExe,
  [Parameter(Mandatory = $true)][string]$Project,
  [string]$Output,
  [int]$Frame = 0,
  [string]$BatchFile,
  [switch]$HideAxis,
  [switch]$HideFloor,
  [switch]$BlackBackground,
  [int]$OutputWidth = 0,
  [int]$OutputHeight = 0,
  [int]$LoadWaitMs = 5000,
  [int]$TimeoutMs = 60000
)

$ErrorActionPreference = "Stop"

$typeDef = @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class MmdExportWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hWnd, UInt32 Msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint uCode, uint uMapType);
  [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr hwndCtl);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@
Add-Type -TypeDefinition $typeDef

function ConvertTo-SignedIntPtr([uint32]$value) {
  return [IntPtr]([BitConverter]::ToInt32([BitConverter]::GetBytes($value), 0))
}

function Send-MmdVirtualKey($handle, [int]$virtualKey, [int]$repeat) {
  if ($repeat -le 0) {
    return
  }
  [void][MmdExportWin32]::ShowWindowAsync($handle, 9)
  [void][MmdExportWin32]::SetForegroundWindow($handle)
  Start-Sleep -Milliseconds 200
  $scan = [MmdExportWin32]::MapVirtualKey([uint32]$virtualKey, 0)
  $downLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16)))
  $upLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16) -bor (1 -shl 30) -bor (1 -shl 31)))
  $vkPtr = [IntPtr]$virtualKey
  for ($i = 0; $i -lt $repeat; $i += 1) {
    [MmdExportWin32]::keybd_event([byte]$virtualKey, [byte]$scan, 0, [UIntPtr]::Zero)
    [MmdExportWin32]::keybd_event([byte]$virtualKey, [byte]$scan, 2, [UIntPtr]::Zero)
    [void][MmdExportWin32]::PostMessage($handle, 0x0100, $vkPtr, $downLParam)
    [void][MmdExportWin32]::PostMessage($handle, 0x0101, $vkPtr, $upLParam)
  }
}

function Send-MmdKeyToHandle($handle, [int]$virtualKey) {
  $scan = [MmdExportWin32]::MapVirtualKey([uint32]$virtualKey, 0)
  $downLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16)))
  $upLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16) -bor (1 -shl 30) -bor (1 -shl 31)))
  $vkPtr = [IntPtr]$virtualKey
  [void][MmdExportWin32]::PostMessage($handle, 0x0100, $vkPtr, $downLParam)
  [void][MmdExportWin32]::PostMessage($handle, 0x0101, $vkPtr, $upLParam)
}

function Get-WindowTextValue($handle) {
  $builder = [Text.StringBuilder]::new(512)
  [void][MmdExportWin32]::GetWindowText($handle, $builder, $builder.Capacity)
  $builder.ToString()
}

function Get-WindowClassValue($handle) {
  $builder = [Text.StringBuilder]::new(256)
  [void][MmdExportWin32]::GetClassName($handle, $builder, $builder.Capacity)
  $builder.ToString()
}

function Get-TopWindowsByPid([int]$targetPid) {
  $items = New-Object System.Collections.Generic.List[object]
  [MmdExportWin32]::EnumWindows({
    param($handle, $lParam)
    [uint32]$windowPid = 0
    [void][MmdExportWin32]::GetWindowThreadProcessId($handle, [ref]$windowPid)
    if ($windowPid -eq $targetPid) {
      $items.Add([pscustomobject]@{
        Handle = $handle
        Class = Get-WindowClassValue $handle
        Text = Get-WindowTextValue $handle
        Visible = [MmdExportWin32]::IsWindowVisible($handle)
      })
    }
    $true
  }, [IntPtr]::Zero) | Out-Null
  $items
}

function Get-ChildControls($parent) {
  $items = New-Object System.Collections.Generic.List[object]
  [MmdExportWin32]::EnumChildWindows($parent, {
    param($handle, $lParam)
    $rect = New-Object MmdExportWin32+RECT
    [void][MmdExportWin32]::GetWindowRect($handle, [ref]$rect)
    $items.Add([pscustomobject]@{
      Handle = $handle
      Class = Get-WindowClassValue $handle
      Text = Get-WindowTextValue $handle
      Id = [MmdExportWin32]::GetDlgCtrlID($handle)
      Top = $rect.Top
      Width = $rect.Right - $rect.Left
    })
    $true
  }, [IntPtr]::Zero) | Out-Null
  $items
}

function Get-ChildControlById($parent, [int]$id) {
  Get-ChildControls $parent | Where-Object { $_.Id -eq $id } | Select-Object -First 1
}

function Dismiss-MmdStartupDialogs([int]$targetPid) {
  $dialogs = Get-TopWindowsByPid $targetPid |
    Where-Object { $_.Visible -and $_.Class -eq "#32770" -and $_.Text -notlike "*画像ファイル出力*" }
  foreach ($dialog in $dialogs) {
    $children = Get-ChildControls $dialog.Handle
    $okButton = $children |
      Where-Object { $_.Class -eq "Button" -and ($_.Text -match "^(OK|はい|Yes|続行)") } |
      Select-Object -First 1
    if ($okButton) {
      [void][MmdExportWin32]::SendMessage($okButton.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
    } else {
      [void][MmdExportWin32]::SetForegroundWindow($dialog.Handle)
      Send-MmdKeyToHandle $dialog.Handle 0x0D
    }
    Start-Sleep -Milliseconds 300
  }
}

function Wait-MmdMainWindow([int]$targetPid, [int]$timeoutMs) {
  $deadline = (Get-Date).AddMilliseconds($timeoutMs)
  while ((Get-Date) -lt $deadline) {
    $process = Get-Process -Id $targetPid -ErrorAction SilentlyContinue
    if (-not $process) {
      throw "MMD process exited before main window was found."
    }
    Dismiss-MmdStartupDialogs $targetPid
    $mainWindow = Get-TopWindowsByPid $targetPid |
      Where-Object { $_.Class -eq "Polygon Movie Maker" } |
      Select-Object -First 1
    if ($mainWindow) {
      return $mainWindow
    }
    Start-Sleep -Milliseconds 200
  }
  return $null
}

function Set-MmdRenderDisplay($mainWindow) {
  if ($BlackBackground) {
    [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]282, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
  }
  if ($HideAxis) {
    [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]215, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
  }
  if ($HideFloor) {
    [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]285, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 100
  }
}

function Set-MmdOutputSize($mainWindow, [int]$width, [int]$height) {
  if ($width -le 0 -or $height -le 0) {
    return
  }
  [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]212, [IntPtr]::Zero)
  $dialog = Wait-Until 10000 {
    Get-TopWindowsByPid $process.Id |
      Where-Object { $_.Visible -and $_.Class -eq "#32770" -and $_.Text -like "*出力画面サイズ変更*" } |
      Select-Object -First 1
  }
  if (-not $dialog) {
    Write-Warning "MMD output size dialog not found; keeping PMM/current output size."
    return $false
  }

  $widthEdit = Get-ChildControlById $dialog.Handle 621
  $heightEdit = Get-ChildControlById $dialog.Handle 622
  if (-not $widthEdit -or -not $heightEdit) {
    throw "MMD output size edit controls were not found."
  }

  [void][MmdExportWin32]::SendMessage($widthEdit.Handle, 0x000C, [IntPtr]::Zero, ([string]$width))
  [void][MmdExportWin32]::SendMessage($heightEdit.Handle, 0x000C, [IntPtr]::Zero, ([string]$height))
  Start-Sleep -Milliseconds 100
  $okButton = Get-ChildControlById $dialog.Handle 1
  if ($okButton) {
    [void][MmdExportWin32]::SendMessage($okButton.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
  } else {
    [void][MmdExportWin32]::PostMessage($dialog.Handle, 0x0111, [IntPtr]1, [IntPtr]::Zero)
  }
  Start-Sleep -Milliseconds 300
  return $true
}

function Set-MmdCameraOperationMode($mainWindow) {
  $operationCombo = Get-ChildControlById $mainWindow.Handle 436
  if (-not $operationCombo) {
    return $false
  }

  $cameraOperationIndex = 0
  $comboBoxSetCurSel = 0x014E
  $comboBoxSelectionChanged = 1
  [void][MmdExportWin32]::SendMessage($operationCombo.Handle, $comboBoxSetCurSel, [IntPtr]$cameraOperationIndex, [IntPtr]::Zero)
  $command = ($comboBoxSelectionChanged -shl 16) -bor 436
  [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]$command, $operationCombo.Handle)
  Start-Sleep -Milliseconds 300
  return $true
}

function Set-MmdCurrentFrame($mainWindow, [int]$frame) {
  $frameEdit = Get-ChildControlById $mainWindow.Handle 417
  if (-not $frameEdit) {
    return $false
  }

  [void][MmdExportWin32]::ShowWindowAsync($mainWindow.Handle, 9)
  [void][MmdExportWin32]::ShowWindowAsync($mainWindow.Handle, 9)
  [void][MmdExportWin32]::SetForegroundWindow($mainWindow.Handle)
  Start-Sleep -Milliseconds 150
  [void][MmdExportWin32]::SetFocus($frameEdit.Handle)
  [void][MmdExportWin32]::SendMessage($frameEdit.Handle, 0x000C, [IntPtr]::Zero, ([string]$frame))
  Start-Sleep -Milliseconds 100
  Send-MmdKeyToHandle $frameEdit.Handle 0x0D
  Send-MmdKeyToHandle $mainWindow.Handle 0x0D
  Start-Sleep -Milliseconds 500
  return $true
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

function Export-MmdImage($mainWindow, [int]$frame, [string]$outputPath) {
  if ($frame -ge 0) {
    $frameWasSet = Set-MmdCurrentFrame $mainWindow $frame
    if (-not $frameWasSet -and $frame -gt 0) {
      Send-MmdVirtualKey $mainWindow.Handle 0x27 $frame
    }
    Start-Sleep -Milliseconds 1000
  }

  [void][MmdExportWin32]::PostMessage($mainWindow.Handle, 0x0111, [IntPtr]276, [IntPtr]::Zero)
  $dialog = Wait-Until 10000 {
    Get-TopWindowsByPid $process.Id |
      Where-Object { $_.Visible -and $_.Class -eq "#32770" -and $_.Text -like "*画像ファイル出力*" } |
      Select-Object -First 1
  }
  if (-not $dialog) {
    throw "MMD image export dialog not found."
  }

  $children = Get-ChildControls $dialog.Handle
  $fileNameEdit = $children |
    Where-Object { $_.Class -eq "Edit" -and $_.Width -gt 100 } |
    Sort-Object Top -Descending |
    Select-Object -First 1
  if (-not $fileNameEdit) {
    throw "MMD image export filename edit was not found."
  }

  [void][MmdExportWin32]::SendMessage($fileNameEdit.Handle, 0x000C, [IntPtr]::Zero, $outputPath)
  Start-Sleep -Milliseconds 200
  $saveButton = $children |
    Where-Object { $_.Class -eq "Button" -and $_.Text -like "*保存*" } |
    Select-Object -First 1
  if ($saveButton) {
    [void][MmdExportWin32]::SendMessage($saveButton.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)
  } else {
    [void][MmdExportWin32]::PostMessage($dialog.Handle, 0x0111, [IntPtr]1, [IntPtr]::Zero)
  }

  $created = Wait-Until 20000 {
    if (Test-Path -LiteralPath $outputPath) {
      Get-Item -LiteralPath $outputPath
    }
  }
  if (-not $created) {
    throw "MMD image export did not create output: $outputPath"
  }

  return [pscustomobject]@{
    ok = $true
    frame = $frame
    output = $created.FullName
    length = $created.Length
  }
}

if ($env:MMD_DUMPER_ALLOW_MMD_LAUNCH -ne "1") {
  throw "Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run."
}

$mmdExePath = (Resolve-Path -LiteralPath $MmdExe).Path
$projectPath = (Resolve-Path -LiteralPath $Project).Path
if ($BatchFile) {
  $batchPath = (Resolve-Path -LiteralPath $BatchFile).Path
  $batchItems = Get-Content -LiteralPath $batchPath -Raw | ConvertFrom-Json
} elseif ($Output) {
  $batchItems = @([pscustomobject]@{ frame = $Frame; output = $Output })
} else {
  throw "Either -Output or -BatchFile is required."
}

foreach ($item in $batchItems) {
  $item.output = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($item.output)
  $outputDir = Split-Path -Parent $item.output
  New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
  Remove-Item -LiteralPath $item.output -Force -ErrorAction SilentlyContinue
}

$process = Start-Process -FilePath $mmdExePath -ArgumentList @($projectPath) -WorkingDirectory (Split-Path -Parent $mmdExePath) -PassThru
try {
  $mainWindow = Wait-MmdMainWindow $process.Id $TimeoutMs
  if (-not $mainWindow) {
    throw "MMD main window not found."
  }

  [void][MmdExportWin32]::SetForegroundWindow($mainWindow.Handle)
  Start-Sleep -Milliseconds $LoadWaitMs
  Dismiss-MmdStartupDialogs $process.Id
  [void](Set-MmdOutputSize $mainWindow $OutputWidth $OutputHeight)
  Set-MmdRenderDisplay $mainWindow
  [void](Set-MmdCameraOperationMode $mainWindow)

  $results = New-Object System.Collections.Generic.List[object]
  foreach ($item in $batchItems) {
    $exported = Export-MmdImage $mainWindow ([int]$item.frame) $item.output
    $results.Add($exported) | Out-Null
    $exported | ConvertTo-Json -Compress
  }

  [pscustomobject]@{
    ok = $true
    count = $results.Count
    outputs = $results
  } | ConvertTo-Json -Compress
} finally {
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force
  }
}
