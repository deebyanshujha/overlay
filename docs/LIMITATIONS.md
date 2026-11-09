# Known limitations

What OVERLAY does not do, and why. Everything here is surfaced in the
product itself — on the Integrations page, in module notes, or in the widget
that would otherwise show a number it cannot justify.

---

## Not implemented

### Now Playing widget

Reading the current media session needs WinRT's
`GlobalSystemMediaTransportControls`, which is not wired up in this build.
The widget renders *"Unavailable — media session reading is not implemented
in this build"* rather than *"Nothing playing"*, because the latter would be
indistinguishable from a wrong answer.

### Weather provider

The weather widget ships with no provider configured, and OVERLAY makes no
network requests until one is added. The widget says so.

### Volume and brightness control

Both are shortcuts to the matching Windows settings page, labelled *Opens
Windows* in the Control Center.

Driving the audio endpoint needs COM interop that is not implemented here,
and brightness is only settable on internal panels through WMI — it fails on
most desktops and many external monitors. A slider that works on some
machines and not others is worse than an honest link, so the type system
makes the distinction explicit (`ControlKind::Action` vs
`ControlKind::OpensWindows`) and the UI shows it.

---

## Limited by Windows

### Taskbar accent cannot be verified

Windows accepts `SetWindowCompositionAttribute` and visibly applies it, but
`GetWindowCompositionAttribute` reports the zeroed policy regardless of what
was written (measured on build 26100).

OVERLAY records *"Windows accepted the change; this setting cannot be read
back to confirm it"* — the strongest true statement available. It does not
claim verification it cannot perform, and it does not treat the unreadable
value as failure.

Nothing is lost for restore: the accent is window state, so it disappears
when OVERLAY exits or Explorer restarts.

### Taskbar accent is lost when Explorer restarts

Same reason. OVERLAY re-applies it while the module is enabled.

### GPU metrics on AMD and Intel

NVIDIA GPUs report load and temperature through NVML. AMD and Intel expose
neither to an unprivileged process on Windows. OVERLAY reports those metrics
as unavailable **with the reason**, in both the HUD and the widgets. It does
not estimate.

### Windows 11-only taskbar settings

Alignment, icon size and the widgets button do not exist before build 22000.
On Windows 10 those controls are hidden rather than shown doing nothing, and
the module reports `Partial` availability explaining why.

### Desktop widgets can be raised by Show Desktop

Widgets use `HWND_BOTTOM` rather than reparenting into Explorer's `WorkerW`.
Reparenting looks marginally more authentic but breaks input routing and
compositing for WebView2 surfaces, and varies by Windows build.

The trade-off: Win+D and some full-screen transitions can pull a widget
above other windows. `reassert_layering` puts it back.

### Secondary-monitor taskbars

The accent is applied to every taskbar window found, including
`Shell_SecondaryTrayWnd`. Registry settings like alignment are per-user and
apply to all taskbars — Windows offers no per-monitor control.

---

## Limited by the companion applications

### WallFlow: cannot request a wallpaper change

WallFlow exposes no command-line or IPC entry point for changing the
wallpaper. OVERLAY can detect it, report its state, read the current
wallpaper from its database, and open it — but the change itself has to be
made in WallFlow's own window.

The Integrations page states this as an unsupported capability rather than
offering a button that quietly does nothing.

### QuickDock: cannot be shown, hidden or quit

QuickDock is driven by its own global hotkey and has no interface for
toggling visibility. OVERLAY can start it (silently, with `--autostart`) but
cannot close it.

Quitting is *deliberately* not attempted even though terminating the process
would technically work: QuickDock persists its search index on exit, and
force-terminating it would throw that away.

### Neither application's settings are written

By design. Pinned items, recents, the search index, the wallpaper library
and its schedule belong to those applications. OVERLAY reads; it does not
write.

---

## Not verified on this hardware

Listed rather than implied to work.

| | Status |
| --- | --- |
| **Multi-monitor** placement, disconnection, mixed DPI | Logic unit-tested against synthetic multi-monitor geometry including negative origins; not exercised on real hardware — only one display was available |
| **GPU metrics** | No NVIDIA GPU available. The *unavailable* path is the one tested |
| **Windows 10** | Built and tested on Windows 11 build 26100. Win11-only settings are skipped by a tested code path, but the overall experience has not been run on 10 |
| **Battery** on a desktop | Tested on a laptop; the no-battery path is unit-tested only |

---

## Operational notes

### WebView2 startup can fail transiently

If a previous instance was force-killed, its WebView2 user-data folder can
stay locked briefly and the next start fails with
`0x800700AA — The requested resource is in use`. Starting again a few
seconds later works.

The recovery CLI is unaffected: `--restore-original` runs before Tauri is
initialised and needs no WebView2 at all.

### Built-in profiles are seeded once

`ensure_builtins` only inserts. If a later OVERLAY version ships better
defaults for Minimal or Developer, an existing install keeps the ones it has
— because the alternative is overwriting edits the user made. Delete a
built-in profile's row, or create a new profile, to pick up new defaults.

### The database is the restore record

`overlay.db` holds the original values. Deleting it while OVERLAY owns
settings strands those changes — OVERLAY would no longer know what to put
back.

`original-windows.json` is written alongside it as a plain-text mirror for
exactly this case, so the values are at least recoverable by hand. Back up
the whole `%LOCALAPPDATA%\com.overlay.studio` folder, not just parts of it.

### One shelf per session

The Dropzone shelf lives in memory and is cleared when OVERLAY exits. That
is deliberate: it is a holding area for files in transit, and persisting it
would accumulate references to files that have since moved or been deleted.
No file is ever deleted when the shelf is cleared.

### Folders cannot be copied or moved from the Dropzone

Files only. A folder on the shelf is shown and can be opened or revealed,
but transferring it reports *"folders are not supported yet — only files"*
rather than partially copying a tree.
