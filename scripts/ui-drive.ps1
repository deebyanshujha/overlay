# Drives the OVERLAY window for manual verification on the real machine:
# click at a point, type, and capture the screen.
#
# Uses SendInput rather than the older mouse_event, which WebView2's render
# widget does not reliably act on.
#
#   .\ui-drive.ps1 -Click 640,690
#   .\ui-drive.ps1 -Shot page.png
#   .\ui-drive.ps1 -Click 300,200 -Shot after.png -Wait 800

param(
    [string]$Click,
    [string]$Shot,
    [int]$Wait = 600,
    [string]$Type,
    [switch]$NoFocus,
    # Minimise the console this runs from before touching the app. Windows'
    # foreground lock will not let a background script raise another window
    # above the active terminal, so without this every click lands on the
    # terminal and every screenshot shows it.
    [switch]$Solo
)

Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Aware {
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
    public static readonly IntPtr PER_MONITOR_V2 = new IntPtr(-4);
}
'@
# Must run before anything measures the screen. Without it this script sees
# a virtualised 1536x864 desktop while capture.ps1 and the screenshots use
# the real 1920x1080, so every coordinate read off a screenshot is scaled
# wrong and clicks land about a quarter of the screen too far right.
[Aware]::SetProcessDpiAwarenessContext([Aware]::PER_MONITOR_V2) | Out-Null

Add-Type -AssemblyName System.Windows.Forms, System.Drawing

Add-Type @'
using System;
using System.Runtime.InteropServices;

[StructLayout(LayoutKind.Sequential)]
public struct MOUSEINPUT {
    public int dx, dy;
    public uint mouseData, dwFlags, time;
    public IntPtr dwExtraInfo;
}

// INPUT is 40 bytes on x64: a 4-byte type, 4 bytes of alignment padding,
// then the 32-byte union whose largest member is MOUSEINPUT. Adding explicit
// padding fields makes it 48 and SendInput rejects it with ERROR_INVALID_
// PARAMETER, returning 0 events sent — which looks exactly like input being
// blocked, so it is worth stating plainly.
[StructLayout(LayoutKind.Sequential)]
public struct INPUT {
    public uint type;
    public MOUSEINPUT mi;
}

public class Drive {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(
        IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);

    public const int MINIMIZE = 6, RESTORE = 9, SHOW = 5;

    const uint MOVE = 0x0001, LEFTDOWN = 0x0002, LEFTUP = 0x0004, ABSOLUTE = 0x8000;

    public static uint ClickAt(int x, int y) {
        int w = GetSystemMetrics(0), h = GetSystemMetrics(1);
        // SendInput's absolute coordinates are normalised to 0..65535.
        int nx = (int)((x * 65535.0) / (w - 1));
        int ny = (int)((y * 65535.0) / (h - 1));

        SetCursorPos(x, y);
        System.Threading.Thread.Sleep(80);

        INPUT[] seq = new INPUT[3];
        seq[0].type = 0; seq[0].mi.dx = nx; seq[0].mi.dy = ny;
        seq[0].mi.dwFlags = MOVE | ABSOLUTE;
        seq[1].type = 0; seq[1].mi.dwFlags = LEFTDOWN;
        seq[2].type = 0; seq[2].mi.dwFlags = LEFTUP;

        return SendInput(3, seq, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@

if ($Solo) {
    $host_hwnd = [Drive]::GetForegroundWindow()
    [Drive]::ShowWindow($host_hwnd, [Drive]::MINIMIZE) | Out-Null
    Start-Sleep -Milliseconds 500
}

if (-not $NoFocus) {
    $proc = Get-Process overlay -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($proc -and $proc.MainWindowHandle -ne 0) {
        [Drive]::ShowWindow($proc.MainWindowHandle, [Drive]::SHOW) | Out-Null
        [Drive]::BringWindowToTop($proc.MainWindowHandle) | Out-Null
        [Drive]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
        Start-Sleep -Milliseconds 350
    }
}

if ($Click) {
    $parts = $Click -split ','
    $sent = [Drive]::ClickAt([int]$parts[0], [int]$parts[1])
    if ($sent -ne 3) { Write-Warning "SendInput accepted only $sent of 3 events" }
    Start-Sleep -Milliseconds $Wait
}

if ($Type) {
    [System.Windows.Forms.SendKeys]::SendWait($Type)
    Start-Sleep -Milliseconds $Wait
}

if ($Shot) {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $dir = "C:\Users\Lenovo\AppData\Local\Temp\claude\D--overlay\695f95fd-dea6-44b7-82ea-4c6d12b0b7d6\scratchpad"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $path = Join-Path $dir $Shot
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output $path
}
