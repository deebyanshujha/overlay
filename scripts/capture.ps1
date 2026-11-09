# Captures the whole desktop at its true resolution.
#
# PowerShell is DPI-unaware by default, so `Screen.PrimaryScreen.Bounds`
# reports the *logical* size (1536x864 on a 1920x1080 panel at 125%) while
# `CopyFromScreen` reads the *physical* framebuffer — the result is a crop of
# the top-left corner, which silently hides the taskbar and makes correctly
# placed windows look like they run off the edge. Declaring per-monitor DPI
# awareness first makes both agree.
#
#   .\capture.ps1 desktop.png
#   .\capture.ps1 desktop.png -ShowDesktop   # minimise everything first

param(
    [Parameter(Mandatory = $true)][string]$Name,
    [switch]$ShowDesktop,
    [int]$SettleMs = 1200
)

Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Dpi {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
    public static readonly IntPtr PER_MONITOR_V2 = new IntPtr(-4);
}
'@

# Must happen before anything measures the screen.
[Dpi]::SetProcessDpiAwarenessContext([Dpi]::PER_MONITOR_V2) | Out-Null

Add-Type -AssemblyName System.Drawing

$shell = $null
if ($ShowDesktop) {
    $shell = New-Object -ComObject Shell.Application
    $shell.MinimizeAll()
    Start-Sleep -Milliseconds $SettleMs
}

# SM_CXVIRTUALSCREEN / SM_CYVIRTUALSCREEN / SM_XVIRTUALSCREEN / SM_YVIRTUALSCREEN
$w = [Dpi]::GetSystemMetrics(78)
$h = [Dpi]::GetSystemMetrics(79)
$x = [Dpi]::GetSystemMetrics(76)
$y = [Dpi]::GetSystemMetrics(77)

$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $h)))

$dir = "C:\Users\Lenovo\AppData\Local\Temp\claude\D--overlay\695f95fd-dea6-44b7-82ea-4c6d12b0b7d6\scratchpad"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$path = Join-Path $dir $Name
$bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()

if ($shell) {
    Start-Sleep -Milliseconds 300
    $shell.UndoMinimizeALL()
}

Write-Output "$path  (${w}x${h})"
