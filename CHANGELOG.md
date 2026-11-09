# Changelog

All notable changes to OVERLAY.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.1] — 2026-11-09

Fixes for an unusable interface. Reported as "I click anything and it gets
stuck… it doesn't even close on clicking exit."

### Fixed

- **Surfaces floated on top of the studio window and swallowed its clicks.**
  `SurfaceManager` lowered each desktop widget to the bottom of the
  Z-order and *then* called `show()` — and showing a window raises it, so
  every widget, the HUD and the Dropzone ended up above the studio. The
  Dropzone landed over the middle of the page and the HUD over the title
  bar, which is why the close button appeared dead: the clicks were landing
  on surfaces. Stacking is now applied either side of `show()` — the
  always-on-top flag before it, the lowering after it — because each has
  the opposite requirement.
- **The Dropzone was always-on-top.** A 300×380 panel pinned above every
  other window, including OVERLAY's own. It is now an ordinary window, so
  clicking anything else raises that instead.
- **Clicking a desktop widget raised it permanently.** Desktop-layer
  surfaces now carry `WS_EX_NOACTIVATE`, so they never take focus.
- **Work that creates windows ran on the event loop and deadlocked the
  application.** This was the freeze. Creating a window blocks until the
  event loop services the request, so doing it *from* the event loop hangs
  everything: no clicks register, nothing repaints, and the close button
  stops responding — the only way out is Task Manager. Four entry points
  were affected, and all now hand the work to a worker thread:
  - every Tauri command (Tauri runs a synchronous command on the main
    thread);
  - the single-instance callback, so `--profile` froze the running copy
    after the first use;
  - the global Control Center shortcut;
  - the tray's Control Center item.

  Reproduced before the fix as `IsHungAppWindow == true` after a single
  `--profile` switch, and confirmed afterwards with five consecutive
  switches and no hang.
- **A deadlocked profile switch left the previous profile's widgets on
  screen.** They were never closed because the switch never finished. Fixed
  by the above; verified that switching to Original Windows now leaves no
  surface behind at all.
- **Listing integrations cost 86 ms and copied a 4 MB database every time.**
  Detection is cached for two seconds and invalidated when OVERLAY starts
  one of the applications. Module status went from 21 ms to 4 ms by caching
  the process lookup behind it.
- **Surfaces polled the backend on timers** — every widget window asking for
  its configuration every two seconds, the HUD and dock every four or five.
  They are now told when something changes, through one event.
- **The command line panicked when it had nowhere to print.** The release
  binary is a GUI-subsystem application with no console of its own, and
  `println!` panics rather than failing quietly when stdout cannot be
  written. `overlay.exe --version` crashed; `--restore-original` would have
  turned a successful restore into what looked like one. Output now ignores
  a closed pipe.
- **An empty warning banner could sit at the top of the studio forever.** A
  dismissed recovery notice was stored as an empty string rather than
  removed, and the empty string rendered as a banner with no text in it.
- **WallFlow's wallpaper thumbnail never loaded** on the Integrations page:
  its folder was outside the asset protocol's scope. Logged as
  `asset protocol not configured to allow the path`, visible as a silently
  missing image.

---

## [1.0.0] — 2026-11-09

First release.

### Added

**Customization engine**

- Modular architecture: nine modules behind one lifecycle contract, each
  independently enableable, configurable and reversible.
- Change journal — every system modification captured, applied, verified and
  recorded, with ownership tracking and conflict detection.
- Transactional profile switching with rollback on a failed system change.
- Crash recovery: interrupted changes are reconciled against the live
  machine at startup; an interrupted profile switch is completed or unwound.

**Modules**

- **Taskbar Studio** — translucency (tinted, blurred, acrylic, clear),
  alignment, icon size, auto-hide, widgets and task-view buttons, Windows
  transparency effects.
- **Desktop Widgets** — 14 widgets with per-widget position, size, monitor,
  opacity, panel style, typeface, text scale, refresh interval, lock,
  click-through and Z-mode.
- **System HUD** — corner strip with sparklines, eight selectable metrics,
  four anchors, two layouts.
- **Floating Dock** — pinned applications, Gaussian hover magnification,
  four edges, running indicators.
- **Control Center** — quick settings on a global shortcut.
- **Dropzone** — reference-only file shelf with explicit copy and confirmed
  move.
- **Desktop Icons** — visibility only; no file is ever touched.
- **WallFlow** and **QuickDock** integrations.

**Profiles**

- Six built-ins: Original Windows, Minimal, Elegant, Developer, Gaming,
  Custom.
- Create, rename, duplicate, delete, export and import. Imports are
  re-validated field by field; unknown modules are dropped with a warning.
- Optional startup profile.

**Restoration**

- *Restore Original Desktop* with a live preview of exactly what will change
  and explicit statements of what will not.
- `overlay.exe --restore-original` — works with no window, frontend or
  WebView2. Exit code 0 verified, 2 partial, 1 failed.
- `overlay.exe --status` — lists owned settings, changes nothing.
- `original-windows.json`, a plain-text mirror of every captured original,
  written atomically alongside the database.
- Automatic database backups before each restore and profile switch.

**Interface**

- Five pages: Home, Customize, Profiles, Integrations, Settings.
- Two themes (Minimal, Futuristic) × five accents, independent of the
  desktop.
- First-launch onboarding that enables nothing.
- Diagnostics with measured memory, surfaces and the full change journal.
- Tray with profile status, owned-setting count and restore.

**Command line**

- `--restore-original`, `--recover`, `--status`, `--profile <name>`,
  `--minimised`, `--version`, `--help`.
- `--profile` accepts unquoted multi-word names.

### Design decisions worth recording

- **Absence is a value.** `RegValue::Absent` round-trips through capture,
  storage, export and restore; restoring it deletes the value rather than
  writing a default over a setting the user never set.
- **No injection.** No DLL or thread is loaded into Explorer, no system
  binary is patched, nothing outside HKCU is written, and elevation is never
  required.
- **No decorative controls.** Volume and brightness are typed as shortcuts
  to Windows settings rather than sliders, because they cannot be driven
  reliably on every machine.
- **No invented readings.** A sensor this machine cannot report renders as
  unavailable *with the reason*.
- **Honest verification.** The taskbar accent cannot be read back on
  Windows; the journal records that Windows accepted the change rather than
  claiming a verification it cannot perform.
- **OVERLAY ships its own dock.** QuickDock turned out to be a keyboard
  search palette rather than an icon dock, so the two are complementary and
  the UI explains the relationship.

### Fixed during development

Recorded because each was found by a test or by running on real hardware,
and each would have been a user-visible defect:

- Binary registry values could not be serialised — internally-tagged serde
  enums cannot hold a `Vec<u8>` newtype, which is exactly the taskbar
  auto-hide blob.
- Crash recovery parsed display text as JSON, so every interrupted change
  would have been recorded as unrecoverable.
- Desktop icon visibility was read from `SHELLDLL_DefView`, which is always
  visible; the idempotent setter therefore behaved as a blind toggle. It now
  reads `SysListView32`.
- Taskbar accent verification failed changes that had in fact worked,
  rolling back every taskbar profile.
- Surfaces were placed using physical pixels while Tauri positions windows
  in logical ones, putting the HUD off-screen on any scaled display.
- `--profile Original Windows` matched nothing, because the multi-word name
  split across argv.
- Test binaries aborted at load with `STATUS_ENTRYPOINT_NOT_FOUND` for want
  of a Common Controls 6 manifest.
- The first-launch screen had no title bar, leaving the frameless window
  unmovable and unclosable.

### Known limitations

See [docs/LIMITATIONS.md](docs/LIMITATIONS.md). In brief: Now Playing and
weather are not implemented; volume and brightness open Windows settings;
GPU metrics need NVIDIA; the taskbar accent cannot be verified or survive an
Explorer restart; WallFlow cannot be asked to change the wallpaper and
QuickDock cannot be shown, hidden or quit.

[1.0.0]: https://example.invalid/overlay/releases/tag/v1.0.0
