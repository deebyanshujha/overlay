# OVERLAY

**Your desktop. Your rules.**

A Windows desktop customization studio. Widgets, taskbar appearance, a
floating dock, a system HUD, a control centre and a file shelf — each one
independent, each one switchable, each one reversible.

Everything is optional. Everything is configurable. Everything is reversible.

---

## What it does

| | |
| --- | --- |
| **Taskbar Studio** | Translucency, alignment, icon size, auto-hide |
| **Desktop Widgets** | 14 widgets: clock, calendar, weather, CPU, memory, GPU, battery, network, storage, uptime and more |
| **Floating Dock** | Pinned applications with hover magnification |
| **System HUD** | A corner strip of live metrics with sparklines |
| **Control Center** | Quick settings on a global shortcut |
| **Dropzone** | A shelf for files in transit |
| **Desktop Icons** | Hide and show, without touching a single file |
| **WallFlow / QuickDock** | First-class integration with both |

## The part that matters

OVERLAY changes Windows settings. Before it changes one, it reads and records
what was there — including settings that have *never been set*, which are
restored by being cleared again rather than by writing a default over them.

One button puts everything back:

> **Restore Original Desktop** — undoes every change OVERLAY made, and
> nothing else.

It never touches a setting OVERLAY did not change, never deletes a shortcut,
and never reconfigures your other applications. If you changed something
yourself after OVERLAY did, your value stands and OVERLAY tells you it left
it alone.

And it works without the interface:

```
overlay.exe --restore-original
```

That path needs only SQLite and Win32 — no window, no frontend, no WebView2.
A broken UI cannot trap you in a customised desktop.

---

## Install

Download the installer from `src-tauri/target/release/bundle/` after building,
or build it yourself:

```bash
npm install
npm run app:build
```

This produces:

- `…/bundle/nsis/OVERLAY_1.0.0_x64-setup.exe` — per-user installer
- `…/bundle/msi/OVERLAY_1.0.0_x64_en-US.msi` — MSI package

Both install without administrator rights. OVERLAY never requires elevation:
everything it changes lives in your own user profile.

### Requirements

| | |
| --- | --- |
| Windows | 10 1803 or later; Windows 11 for taskbar alignment and icon size |
| WebView2 | Pre-installed on Windows 11; the installer fetches it otherwise |
| Privileges | None. Runs as a normal user |

### Building from source

```bash
# Prerequisites: Rust 1.80+, Node 20+, the MSVC build tools
npm install
npm run app:dev      # development, with hot reload
npm run app:build    # production installers
```

---

## Command line

```
overlay.exe                        Start normally
overlay.exe --restore-original     Undo everything, print a report, exit
                                   (0 = verified, 2 = partial)
overlay.exe --status               List the settings OVERLAY owns. Changes nothing
overlay.exe --profile <name>       Apply a profile and stay in the tray
overlay.exe --minimised            Start in the tray without a window
overlay.exe --help
```

`--profile` accepts a profile's display name or its id, and takes unquoted
multi-word names: `overlay.exe --profile Original Windows`.

---

## Profiles

A profile is a saved set of customizations. Switching applies the difference
and puts back anything the new profile does not want — transactionally, so a
failed switch rolls back rather than leaving the desktop half-changed.

| Profile | What it is |
| --- | --- |
| **Original Windows** | Your desktop as OVERLAY first found it. Cannot be edited |
| Minimal | A clock, a clear taskbar, nothing else |
| Elegant | Glass widgets, soft taskbar tint, floating dock |
| Developer | System HUD, monospaced widgets, left taskbar, Dropzone |
| Gaming | Clean desktop, performance metrics, hidden icons |
| Custom | Empty, to build up however you like |

Profiles export to plain JSON and import with every value re-validated. A
profile is pure data — there is no field anywhere in the schema that names a
command, a script, or a path to execute.

---

## Where things live

```
%LOCALAPPDATA%\com.overlay.studio\
├── overlay.db               profiles, the change journal, original state
├── original-windows.json    plain-text mirror of every captured original
├── overlay.log              diagnostics
├── backups\                 taken before each restore and profile switch
└── exports\                 profiles you export
```

`original-windows.json` is redundant with the database on purpose: it is
readable, and it survives the database being lost.

Nothing leaves your machine. OVERLAY has no telemetry and makes no network
requests.

---

## Documentation

| | |
| --- | --- |
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | How it is put together and why |
| [MODULES.md](docs/MODULES.md) | Every module, its settings, and what it touches |
| [RESTORATION.md](docs/RESTORATION.md) | The change journal and the restore guarantees |
| [WINDOWS_INTEGRATION.md](docs/WINDOWS_INTEGRATION.md) | Which Win32 APIs, and what is deliberately not used |
| [TESTING.md](docs/TESTING.md) | How to run the suites, including the live Windows tests |
| [LIMITATIONS.md](docs/LIMITATIONS.md) | What does not work, and why |
| [CHANGELOG.md](CHANGELOG.md) | Release history |

---

## Development

```bash
npm run dev          # Vite dev server
npm run app:dev      # the full app against it
npm test             # frontend tests (Vitest)
npm run rust:test    # Rust tests
npm run typecheck    # TypeScript

cd src-tauri
cargo test                                          # unit + contract tests
cargo test --test windows_live -- --ignored         # live Windows tests
```

The live tests touch the real machine. They are `#[ignore]`d by default and
restore whatever they change, including on failure.

## Licence

MIT.
