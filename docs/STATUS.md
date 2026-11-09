# OVERLAY — build status

Working notes kept during the build: what was found on the real machine,
what is done and proven, and what remains.

---

## Environment findings (verified, 2026-11-08/09)

| Fact | Value |
| --- | --- |
| Windows | 11, build 26100 |
| Display | 1920×1080 physical at **125%** = 1536×864 logical |
| Rust / Node | 1.98.0 / 22.20.0 |
| `windows` crate | 0.62 (0.58's signatures did not match) |

### Companion applications

| | WallFlow | QuickDock |
| --- | --- | --- |
| Source | `D:\wallflow` | `D:\QuickDock` |
| Installed | `%LOCALAPPDATA%\WallFlow\wallflow.exe` | `%LOCALAPPDATA%\QuickDock\quickdock.exe` |
| Data | `%LOCALAPPDATA%\com.wallflow.app\` (SQLite, WAL) | `%APPDATA%\com.quickdock.app\` (JSON) |
| Stack | Tauri 2 + React 18 | Tauri 2 + React 19 + Tailwind 4 |
| Integration surface | single-instance launch; read-only DB snapshot | single-instance launch, `--autostart`; read-only JSON |

**QuickDock is not an icon dock.** It is a keyboard-first search palette
bound to `Ctrl+Space`. The brief anticipated reusing it for the dock module;
that premise does not hold, so OVERLAY ships its own dock and integrates
QuickDock as a launcher. Both detected installed and running throughout.

Neither application was modified. Both were running at the end.

### Live discoveries that changed the design

1. **`TaskbarAl`, `TaskbarSi`, `EnableTransparency` do not exist** in this
   user's registry → restoration must distinguish absent from zero and
   *delete* on restore. Drove `RegValue::Absent`.
2. **`HideIcons = 1` while icons were visible** → registry and reality
   disagree; the window is the source of truth.
3. **Icon visibility lives on `SysListView32`, not `SHELLDLL_DefView`** →
   reading the container reports "visible" forever.
4. **`GetWindowCompositionAttribute` never reports the accent back** → that
   target is modelled as unverifiable rather than failing changes that
   worked.
5. **Win32 reports physical pixels; Tauri positions in logical ones** →
   every placement goes through `MonitorInfo::logical()`.
6. **Test binaries need a Common Controls 6 manifest** or they abort at load.

---

## Delivered

| Layer | Files |
| --- | --- |
| Win32 | `sys/{registry,shell,accent,process}.rs` |
| Storage | `storage/`, 10-table schema with migrations |
| **Change journal** | `journal/`, plus the JSON original-state mirror |
| Modules | 9 modules behind one lifecycle trait |
| Profiles | `profiles/` + transactional `switch.rs` |
| Restoration | `restoration/` + headless CLI path |
| Windows | `windows/` surface manager, 3 Z-modes |
| Monitoring | `monitoring/` with honest capability reporting |
| Commands | 60 Tauri commands across 7 modules |
| Lifecycle | tray, shortcuts, autostart, single instance, logging |
| Frontend | design system, 5 pages, 5 surfaces, 14 widget renderers |
| Packaging | NSIS + MSI, per-user, no elevation |
| Docs | README, ARCHITECTURE, MODULES, RESTORATION, WINDOWS_INTEGRATION, TESTING, LIMITATIONS, CHANGELOG |

## Tests — all green

| Suite | Count |
| --- | --- |
| Rust unit | 187 |
| Acceptance (real registry) | 10 |
| Journal contract (real registry) | 14 |
| Live Windows (`--ignored`) | 7 |
| Frontend (Vitest) | 38 |
| **Total** | **256** |

## Verified on the real machine

- First launch renders; nothing enabled; detection shows real values
  including both companion apps running and an honest GPU explanation.
- `--profile Minimal` → clock on the desktop above the wallpaper.
- `--profile Developer` → 3 widgets + HUD with 4 live metrics + Dropzone,
  taskbar left-aligned and translucent.
- `--profile "Original Windows"` → everything closed and restored.
- `--restore-original` with the app **not running** → restored, verified,
  exit 0.
- Installed NSIS package runs with the dev server stopped.
- `original-windows.json` written with `"(not set)"` preserved.

## Not verified (no hardware here)

Multi-monitor, NVIDIA GPU metrics, Windows 10. All unit-tested against
synthetic inputs and listed in [LIMITATIONS.md](LIMITATIONS.md).

Synthetic mouse input into WebView2 initially appeared broken; it was my
test harness being DPI-unaware while screenshots were DPI-aware. Fixed in
`scripts/ui-drive.ps1`; the studio pages were then driven and captured.
