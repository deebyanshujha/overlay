# Lists OVERLAY's windows, front to back, with their stacking flags.
#
# Used to verify that desktop widgets sit behind ordinary windows and that
# nothing of OVERLAY's covers the studio.

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

[StructLayout(LayoutKind.Sequential)]
public struct WRect { public int Left, Top, Right, Bottom; }

public class Win {
    [DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out WRect r);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int index);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);

    public const uint NEXT = 2;
    public const int EXSTYLE = -20;
    public const int TOPMOST = 0x8;
    public const int TRANSPARENT = 0x20;
    public const int NOACTIVATE = 0x8000000;
}
'@

$proc = Get-Process overlay -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { Write-Output "OVERLAY is not running."; exit }

Write-Output "OVERLAY windows, front to back:"
$h = [Win]::GetTopWindow([IntPtr]::Zero)
$rank = 0
while ($h -ne [IntPtr]::Zero) {
    $owner = 0
    [Win]::GetWindowThreadProcessId($h, [ref]$owner) | Out-Null
    if ($owner -eq $proc.Id) {
        $cls = New-Object System.Text.StringBuilder 256
        [Win]::GetClassNameW($h, $cls, 256) | Out-Null
        if ($cls.ToString() -eq "Tauri Window") {
            $r = New-Object WRect
            [Win]::GetWindowRect($h, [ref]$r) | Out-Null
            $w = $r.Right - $r.Left
            $ht = $r.Bottom - $r.Top
            $ex = [Win]::GetWindowLong($h, [Win]::EXSTYLE)

            $flags = @()
            if ($ex -band [Win]::TOPMOST)     { $flags += "topmost" }
            if ($ex -band [Win]::TRANSPARENT) { $flags += "click-through" }
            if ($ex -band [Win]::NOACTIVATE)  { $flags += "no-activate" }
            if (-not [Win]::IsWindowVisible($h)) { $flags += "HIDDEN" }

            $kind = if ($w -gt 1000) { "studio " } else { "surface" }
            Write-Output ("  {0}. {1} {2,5}x{3,-5} at {4,5},{5,-5} {6}" -f `
                $rank, $kind, $w, $ht, $r.Left, $r.Top, ($flags -join " "))
            $rank++
        }
    }
    $h = [Win]::GetWindow($h, [Win]::NEXT)
}
