# Architecture

## The shape of it

```
┌─────────────────────────────────────────────────────────────┐
│  React frontend — one bundle, many windows                  │
│  studio · widget-* · dock · hud · control-center · dropzone │
└───────────────────────────┬─────────────────────────────────┘
                            │ Tauri IPC — the only way across
┌───────────────────────────▼─────────────────────────────────┐
│  commands/      the command surface, validation at the door │
├─────────────────────────────────────────────────────────────┤
│  profiles/      profiles, transactional switching           │
│  restoration/   putting it back, with a headless path       │
├─────────────────────────────────────────────────────────────┤
│  modules/       one customization each, common lifecycle    │
│  integrations/  WallFlow and QuickDock adapters             │
│  windows/       native surfaces                             │
│  monitoring/    system metrics                              │
├─────────────────────────────────────────────────────────────┤
│  journal/       every system change: capture→apply→verify   │
├─────────────────────────────────────────────────────────────┤
│  storage/       SQLite, migrations                          │
│  sys/           Win32. Nothing above here calls Windows     │
└─────────────────────────────────────────────────────────────┘
```

Dependencies point downward only. Two consequences are load-bearing:

* **`sys` knows nothing about modules or profiles.** It is a thin, testable
  Win32 layer, which is why the live tests can exercise it directly.
* **`journal` knows nothing about which module made a change.** It stores
  targets and values, not intentions. That is what lets
  `overlay.exe --restore-original` undo everything in a process where no
  module was ever constructed.

---

## The three ideas worth knowing

### 1. Absence is a value

Most Windows personalisation settings do not exist until something writes
them. On the machine this was built on, `TaskbarAl`, `TaskbarSi` and
`EnableTransparency` were all absent.

A restore that writes `0` back over an absent value is not a restore — it
pins a default that was previously free to change with the OS. So:

```rust
pub enum RegValue { Absent, Dword(u32), Str(String), Binary(Vec<u8>) }
```

`Absent` round-trips through capture, storage, export and restore, and
restoring it *deletes* the value. `assert_ne!(RegValue::Absent, RegValue::Dword(0))`
is a test, because the distinction is the whole point.

### 2. One journal, every change

No module writes to Windows directly. Everything goes through
`Journal::apply`, which runs a fixed sequence — capture the original once and
forever, check nobody else has changed it, write a `pending` row, make the
change, read it back, settle the row. [RESTORATION.md](RESTORATION.md) has the
detail.

The payoff is that restoration is a property of the data model rather than of
discipline: the work list comes from `owned_settings`, so there is no code
path by which restore can touch a setting OVERLAY never changed.

### 3. Modules are uniform, surfaces are one bundle

Every customization implements `CustomizationModule` — descriptor,
availability, config validation, capture, enable, disable, restore, status.
The profile engine drives them through that trait and nothing else, so adding
a customization never means touching the profile, restoration or UI layers.

Every window OVERLAY draws loads the same frontend bundle with a `?surface=`
query. One build, one theme engine, one design system — which is why a widget
looks like it belongs to the same product as the studio.

---

## Transactional profile switching

```
validate  →  plan  →  mark in flight  →  disable  →  enable  →  commit
                            │
                            └── the crash marker: a process killed anywhere
                                after this leaves a row that recovery finds
```

1. **Validate** the destination: every module known, every config accepted by
   its own module, dependencies satisfied, no conflicts. Nothing has touched
   Windows yet, so a bad profile fails having changed nothing.
2. **Plan** the diff in dependency order.
3. **Mark in flight**, naming both profiles.
4. **Disable** what the destination does not want — this is where settings
   come back.
5. **Enable** what it does want, capturing originals first.
6. **Commit** and clear the marker.

A module that changes Windows and fails during step 5 rolls the whole switch
back. A module that only draws an OVERLAY window is allowed to fail with a
warning: a clock that would not open is worth reporting, not worth reverting
a taskbar for.

Structural validation runs before availability checks, because "this profile
needs the widget framework" is the same on every machine and more actionable
than "this machine has no displays".

---

## Data model

| Table | Holds |
| --- | --- |
| `profiles`, `profile_modules` | Profiles and their module settings |
| `active_profile` | Which is live, plus the in-flight switch marker |
| `original_state` | Write-once record of every setting's pre-OVERLAY value |
| `applied_changes` | The journal: one row per attempted change |
| `owned_settings` | What OVERLAY currently owns — the restore work list |
| `restoration_history` | Past restores and their reports |
| `integration_registry` | Where WallFlow and QuickDock were last seen |
| `widget_layout`, `app_settings` | Placement and preferences |

`original_state` is written with `INSERT OR IGNORE`. The first capture wins,
so enabling a module twice — or switching profiles ten times — cannot
overwrite the pre-OVERLAY truth with an OVERLAY-written value.

---

## Frontend

| | |
| --- | --- |
| `themes/theme.css` | The whole design system as CSS variables. Two themes, five accents, same token names — a component never branches on the theme |
| `services/api.ts` | Every backend call. Command names spelled once |
| `types/api.ts` | Hand-written mirrors of the Rust types, so drift is a type error |
| `stores/app.ts` | One store. Mutations go through the backend first and update from what it returns — never optimistically |
| `components/ui/` | The primitives everything is built from |
| `pages/`, `surfaces/`, `widgets/` | Studio pages, desktop surfaces, widget renderers |

State is never updated optimistically. A toggle that failed must not look
like it worked, because the user would then believe their desktop changed
when it did not.

---

## Security posture

* The frontend cannot reach Windows. Every command goes through the module
  registry, the journal or the restoration engine.
* No command takes a registry path, a command line or a window handle from
  the UI. Launching is dispatched by **id**, and the executable comes from
  OVERLAY's own detection; the dock can only launch a path already present in
  its validated config.
* `open_external` is allowlisted to `ms-settings:`, `http(s)://` and
  `file:///`.
* Imported profiles are re-validated field by field by the module that owns
  each config. Unknown modules are dropped with a warning.
* No elevation, no telemetry, no network requests.

---

## Decisions and their trade-offs

| Decision | Why | What it costs |
| --- | --- | --- |
| `HWND_BOTTOM` instead of reparenting widgets into `WorkerW` | Reparenting breaks input routing and composition for WebView2 and varies by build | Win+D can raise a widget; a watcher re-asserts |
| `SetWindowCompositionAttribute` for taskbar translucency | The only way past the one fixed acrylic Windows offers; out-of-process, no injection | Undocumented, and not readable back — see below |
| Taskbar accent is **unverifiable** | Windows accepts the write and visibly applies it, but never reports it back (measured, build 26100) | The journal records "Windows accepted it" rather than claiming verification |
| Idempotent desktop-icon setter | The shell offers only a toggle; blind toggling flips the wrong way on stale state | Needs a live read before each change |
| Icon state read from `SysListView32`, not the registry | `HideIcons` and reality genuinely disagree | An extra window lookup |
| OVERLAY ships its own dock | QuickDock is a search palette, not an icon dock | Two launchers, so the UI explains the difference |
| One frontend bundle for every surface | Shared theme, components, API layer | Every window pays for the whole bundle (~107 KB gzipped) |
| Hand-written TS types | A mismatch is a compile error at the call site | They must be kept in step by hand |

---

## DPI

Win32 reports **physical** pixels; Tauri positions windows in **logical**
ones. On the 1920×1080 panel at 125% this was built on, those differ by a
quarter of the screen — enough to put the HUD off the right edge, which is
exactly what happened before `MonitorInfo::logical()` existed.

Every placement calculation goes through `logical()`. Nothing positions a
window from the raw `MonitorInfo` fields.
