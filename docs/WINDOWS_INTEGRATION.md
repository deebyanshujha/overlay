# Windows integration

What OVERLAY calls, what it deliberately does not, and what was measured
rather than assumed.

Everything here was verified on **Windows 11 build 26100**, on a 1920×1080
display at 125% scaling.

---

## The rules

1. **No code runs inside Explorer.** No DLL injection, no thread injection,
   no hooks, no patched system binaries. Everything is an out-of-process
   call against a window handle, or a registry write under HKCU.
2. **No elevation.** Everything OVERLAY changes is per-user. It never asks
   for administrator rights, and the manifest requests `asInvoker`.
3. **Nothing outside HKCU.** No HKLM writes, no HKCR, no policy keys.
4. **No Windows security feature is disabled or bypassed.**

The cost is that a few effects other customisers achieve are not available
here. That is a deliberate trade: a shell that keeps working is worth more
than one extra blur mode.

---

## APIs used

### Registry — `advapi32`

`RegOpenKeyExW`, `RegCreateKeyExW`, `RegQueryValueExW`, `RegSetValueExW`,
`RegDeleteValueW`, `RegEnumKeyExW`.

Only under `HKEY_CURRENT_USER`, except reading the uninstall registry (also
`HKLM`, read-only) to detect WallFlow and QuickDock.

Keys touched:

| Key | Values |
| --- | --- |
| `…\Explorer\Advanced` | `TaskbarAl`, `TaskbarSi`, `TaskbarDa`, `ShowTaskViewButton` |
| `…\Explorer\StuckRects3` | `Settings` — one bit, see below |
| `…\Themes\Personalize` | `EnableTransparency` |

Only values the user actually asked OVERLAY to manage are read or written.
A setting left on "Leave" is never captured and never owned.

### Taskbar translucency — `SetWindowCompositionAttribute`

Undocumented, but shipping unchanged since Windows 10 1803 and what every
mainstream taskbar customiser uses. Resolved dynamically from `user32.dll`;
if the export is ever removed, the lookup fails and OVERLAY reports the
feature as unavailable rather than crashing.

Critically it is an **out-of-process call against a window handle**. No code
is loaded into Explorer. The accent is window state, not persisted state, so
it evaporates when OVERLAY exits or Explorer restarts — there is no path by
which it leaves a permanent mark.

**Measured limitation:** `GetWindowCompositionAttribute` does not report the
policy back. It returns the zeroed policy regardless of what was written. So
this target is marked unverifiable and the journal says so explicitly rather
than claiming a verification it cannot perform. See
[RESTORATION.md](RESTORATION.md) and `tests/probe_accent.rs`.

### Taskbar auto-hide — `StuckRects3\Settings`

Auto-hide is one bit inside a 40-byte binary blob, not its own value: byte 8,
bit 0. OVERLAY captures the whole blob and flips that single bit, so every
other taskbar preference packed into those bytes survives untouched. A blob
of an unexpected shape is refused rather than guessed at.

### Desktop icons — `WM_COMMAND 0x7402`

The exact message the desktop's own context menu sends, delivered to
`SHELLDLL_DefView` with `SendMessageTimeoutW` and `SMTO_ABORTIFHUNG`.

Two findings here, both from measurement:

* **Visibility lives on the `SysListView32`, not the `SHELLDLL_DefView`.**
  The container stays visible either way. Reading it reports "visible"
  forever, which turns the idempotent setter into a blind toggle.
  (`tests/probe_desktop2.rs` re-derives this.)
* **The registry and reality disagree.** This machine reported
  `HideIcons = 1` while icons were on screen. The window is what the user
  sees, so the window is the source of truth.

The shell offers only a *toggle*, so `set_desktop_icons_visible(true)` reads
the live state first and toggles only on a mismatch — which is what makes it
safe to call repeatedly during a profile switch. It then polls briefly for
the change to land, because Explorer applies it on its own message loop.

Only visibility changes. No file, shortcut or icon position is touched.

### Monitors — `EnumDisplayMonitors`, `GetMonitorInfoW`, `GetDpiForMonitor`

Reported in **physical** pixels. Tauri positions windows in **logical** ones.
On a 1920×1080 panel at 125% those differ by a quarter of the screen —
enough to put the HUD off the right edge, which is exactly what happened
before `MonitorInfo::logical()` existed. Every placement goes through it.

The manifest declares `PerMonitorV2`, so OVERLAY sees true pixels rather than
virtualised ones.

### Window layering — `SetWindowPos`, `SetWindowLongPtrW`

Three Z-modes:

| Mode | Implementation | Used by |
| --- | --- | --- |
| `Desktop` | `HWND_BOTTOM` + `WS_EX_NOACTIVATE` | Widgets |
| `Normal` | Default stacking | Dock |
| `Top` | Always-on-top | HUD, Control Center, Dropzone |

**Why not reparent into `WorkerW`?** True WorkerW parenting looks marginally
more "part of the desktop", but a reparented WebView2 surface stops
receiving input and composites unreliably across Windows builds.
`HWND_BOTTOM` achieves the same visual result through documented calls. The
cost is that Win+D and some full-screen transitions can raise the window, so
`SurfaceManager::reassert_layering` puts it back.

Click-through is `WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE`.
`WS_EX_LAYERED` is kept when click-through is turned off — dropping it on a
transparent WebView2 window makes rounded corners and shadows render as
black boxes. Widgets lift click-through while hovered, so a click-through
widget can still be grabbed and moved.

`WS_EX_TOOLWINDOW` keeps surfaces out of Alt-Tab and the taskbar.

### Metrics

| Metric | Source | Notes |
| --- | --- | --- |
| CPU, memory, network, disks, uptime | `sysinfo` | CPU and network are deltas; the first sample reports "waiting", never 0% |
| Battery | `GetSystemPowerStatus` | `BatteryFlag` 128/255 means no battery |
| GPU load and temperature | NVML (`nvml-wrapper`) | NVIDIA only — see below |
| Now Playing | — | Not implemented; reported as such |
| Process memory | `GetProcessMemoryInfo` | For the diagnostics page |

**GPU:** AMD and Intel do not expose load or temperature to an unprivileged
process on Windows. Rather than shipping a widget that shows a plausible
number, OVERLAY reports the metric as unavailable *with the reason*. A fake
sensor reading is worse than none, because the user cannot tell it is fake.

### Other

| | |
| --- | --- |
| Elevation check | `OpenProcessToken` + `TokenElevation` — reported in diagnostics; never required |
| Process detection | `CreateToolhelp32Snapshot` |
| Launching | `CreateProcess` via `std::process::Command`, detached, no window |
| Opening settings pages | `ShellExecuteW`, allowlisted to `ms-settings:`, `http(s)://`, `file:///` |
| OS version | Version resource of `ntoskrnl.exe` — `GetVersionExW` lies to unmanifested processes |
| Console output | `AttachConsole(ATTACH_PARENT_PROCESS)`, so `--restore-original` prints into the terminal it was run from |
| Autostart | `tauri-plugin-autostart`, with `--minimised` |
| Global shortcut | `tauri-plugin-global-shortcut`, one binding, never `Ctrl+Space` |

---

## Deliberately not used

| | Why not |
| --- | --- |
| DLL or thread injection into Explorer | Destabilises the shell; breaks on updates |
| Patching `explorer.exe` or system DLLs | Same, plus it trips integrity checks |
| `SetWindowsHookEx` | Loads our code into other processes |
| Writing to HKLM or policy keys | Needs elevation and affects other users |
| Replacing the shell | OVERLAY coexists with Explorer; it does not replace it |
| Reparenting widgets into `WorkerW` | Breaks WebView2 input and compositing |
| Terminating WallFlow or QuickDock | Not OVERLAY's call; QuickDock would lose its index |

---

## Coexisting with other applications

**QuickDock** owns `Ctrl+Space`. OVERLAY reads QuickDock's settings file to
find its *actual* binding, and refuses to take it — at config validation and
again at registration, because the setting could have been written before
QuickDock was installed. The default Control Center shortcut is
`Alt+Shift+O`.

**WallFlow** owns the wallpaper. OVERLAY does not set wallpapers. It reads
WallFlow's SQLite database read-only, from a temporary snapshot of the three
WAL files, so a running WallFlow is never disturbed.

Neither application's settings are ever written by OVERLAY.

---

## Known Windows-specific limitations

| | |
| --- | --- |
| Taskbar accent is not readable back | Windows does not report it. Recorded as "accepted, not verifiable" |
| Alignment, icon size, widgets button | Windows 11 only. Hidden on Windows 10 rather than shown doing nothing |
| Widgets can be raised by Win+D | Inherent to `HWND_BOTTOM`; re-asserted |
| Accent is lost on Explorer restart | It is window state. Re-applied while the module is on |
| GPU metrics on AMD/Intel | Not exposed to unprivileged processes |
| Now Playing | Needs WinRT `GlobalSystemMediaTransportControls`; not implemented |
| Volume and brightness | Open the Windows settings page instead of being driven directly — neither can be changed reliably on every machine |
