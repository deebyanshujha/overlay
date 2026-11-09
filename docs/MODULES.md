# Modules

Every customization is a module. Each has an independent switch, its own
configuration, and its own undo.

`Changes Windows` marks the ones that modify system settings — those are the
only ones with anything to restore.

| Module | Category | Changes Windows | Requires | Conflicts |
| --- | --- | --- | --- | --- |
| `taskbar` | Appearance | yes | — | — |
| `desktop_icons` | Appearance | yes | — | — |
| `widgets` | Widgets | no | — | — |
| `hud` | Widgets | no | `widgets` | — |
| `dock` | Surfaces | no | — | — |
| `control_center` | Surfaces | no | — | — |
| `dropzone` | Surfaces | no | — | — |
| `wallflow` | Applications | no | — | — |
| `quickdock` | Applications | no | — | `dock` |

---

## The lifecycle

```rust
pub trait CustomizationModule: Send + Sync {
    fn descriptor(&self) -> ModuleDescriptor;
    fn availability(&self, ctx: &ModuleCtx) -> Availability;
    fn validate_config(&self, config: &Value) -> Result<Value>;
    fn default_config(&self) -> Value;
    fn capture_original(&self, ctx: &ModuleCtx) -> Result<()>;
    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()>;
    fn disable(&self, ctx: &ModuleCtx) -> Result<()>;
    fn restore(&self, ctx: &ModuleCtx) -> Result<Vec<RestoreOutcome>>;
    fn is_active(&self, ctx: &ModuleCtx) -> bool;
    fn notes(&self, ctx: &ModuleCtx) -> Vec<String>;
}
```

Three details carry weight:

* **`capture_original` runs before `enable`,** driven by the registry, not by
  the module remembering to do it.
* **`is_active` is derived from the world** — owned settings, live windows —
  never from a cached flag. A crash cannot leave the UI claiming a module is
  on when it is not.
* **`restore` defaults to `journal.restore_module(id)`,** which is the right
  answer for every module whose changes go through the journal. A module
  does not implement its own undo.

`Availability` is three-valued: `Available`, `Partial { note }` (usable but
degraded — say so), `Unavailable { reason }`. An unavailable module is shown
in the UI **with its reason**, not hidden.

Batch operations collect per-module results instead of short-circuiting, so
one failing module is reported alongside the successes rather than stopping
the rest.

---

## Taskbar Studio — `taskbar`

| Setting | Values | Touches |
| --- | --- | --- |
| `finish` | `system`, `tinted`, `blurred`, `acrylic`, `clear` | Window accent |
| `opacity` | 0.0–1.0 | Window accent |
| `tint` | `0xRRGGBB` | Window accent |
| `alignment` | `null`, `left`, `center` | `TaskbarAl` (Win11) |
| `icon_size` | `null`, `small`, `medium`, `large` | `TaskbarSi` (Win11) |
| `auto_hide` | `null`, `true`, `false` | `StuckRects3\Settings`, one bit |
| `system_transparency` | `null`, `true`, `false` | `EnableTransparency` |
| `show_widgets_button` | `null`, `true`, `false` | `TaskbarDa` (Win11) |
| `show_task_view` | `null`, `true`, `false` | `ShowTaskViewButton` |

**`null` means "leave it alone"** — and that is not the same as `false`. A
setting left on *Leave* is never captured, never owned and never restored,
because OVERLAY never touched it. The default config leaves every registry
setting alone; only the accent is applied.

Windows 11-only settings are skipped entirely on Windows 10 rather than
written as inert clutter.

## Desktop Icons — `desktop_icons`

One setting: `show_icons`. Visibility only — no file, shortcut or icon
position is touched, and nothing in this module can reach the filesystem.

Arrangement survives for free: Explorer keeps icon positions in its own view
state, and hiding the listview does not disturb them.

## Desktop Widgets — `widgets`

Fourteen widgets, each independently enableable:

| | | |
| --- | --- | --- |
| `clock` | `date` | `calendar` |
| `weather` | `cpu` | `ram` |
| `gpu` | `gpu_temp` | `battery` |
| `network` | `network_status` | `uptime` |
| `media` | `storage` | |

Per widget: position, size, monitor, opacity, panel style (`glass`, `solid`,
`outline`, `bare`), typeface (`sans`, `mono`, `display`), text scale, refresh
interval, lock, click-through, Z-mode.

Enabling the module adds **one** widget — a clock. Covering someone's
desktop in panels they did not ask for is the opposite of the point.

Values are clamped on the way in: sizes to the widget's own minimum,
intervals to 250 ms–1 hour (0 keeps its meaning of "never poll"), opacity to
0.1–1.0. Non-finite geometry is rejected outright rather than clamped.
A widget that needs a sensor this machine lacks renders *unavailable with
the reason*, never a zero.

## System HUD — `hud`

A corner strip of live metrics with sparklines. Metrics: `cpu`, `ram`, `gpu`,
`gpu_temp`, `battery`, `network`, `storage`, `uptime`. Four anchors, two
layouts, compact mode, configurable refresh, click-through by default.

Requires `widgets` — it shares that framework's metric pipeline and window
machinery, and the dependency makes the engine enable them in the right
order. At least one metric is required; an empty HUD is an invisible window,
which reads as a bug.

Polling stops entirely while the window is hidden, so a HUD behind a
full-screen game costs nothing.

## Floating Dock — `dock`

Pinned applications with Gaussian hover magnification, four edges,
configurable icon size and opacity, running indicators.

Launching goes through `launch_pinned`, which only accepts a path already
present in the validated profile config — the dock cannot be talked into
starting something else. Relative paths are refused outright.

**On QuickDock:** the brief anticipated reusing QuickDock for this. It is a
keyboard-first *search palette*, not an icon dock, so the two solve different
problems and both are offered. The UI says so rather than leaving the user to
wonder why there are two.

## Control Center — `control_center`

A quick-settings panel on a global shortcut (default `Alt+Shift+O`; taking
QuickDock's binding is refused).

Every control is one of exactly two things, and the type system says which:

```rust
enum ControlKind {
    Action,                          // OVERLAY performs it
    OpensWindows { target: String },  // a labelled shortcut to Windows
}
```

There is no third category. **Volume and brightness are shortcuts**, not
sliders: driving the audio endpoint needs COM interop that is not wired up
here, and brightness is only settable on internal panels. Shipping sliders
that work on some machines and not others would be worse than a labelled
link, so the UI marks them *Opens Windows*.

Controls are dispatched by **id**; the target URL comes from OVERLAY's own
catalogue, never from the frontend.

## Dropzone — `dropzone`

A floating shelf for files in transit.

**Dropping a file creates a reference.** Nothing is copied, moved or written.
Clearing the shelf deletes nothing. Copy and move are separate, explicit
actions, and move — the only destructive one — requires a `confirmed` flag
the caller must pass deliberately, and the UI asks first, naming the files.

Move is implemented as copy-then-verify-then-delete, not rename: rename fails
across volumes, and deleting only after a verified byte-count match means an
interrupted move loses nothing. The default collision policy is *rename*,
because nothing can be lost that way.

## WallFlow — `wallflow`

Detects WallFlow, reports its status, shows the current wallpaper, and starts
it when a profile asks for it.

OVERLAY never changes WallFlow's settings, library or schedule, and does not
duplicate its wallpaper engine. Its database is read **read-only**, from a
temporary snapshot of the WAL files, so a running WallFlow is never
disturbed.

Disabling does not stop WallFlow. Terminating an application the user may be
relying on is not OVERLAY's call — and the module says so rather than quietly
doing nothing.

**Not supported:** requesting a wallpaper change. The installed build exposes
no command-line or IPC entry point for it, and the Integrations page states
that plainly instead of offering a button that does nothing.

## QuickDock — `quickdock`

Detects QuickDock, reports its status and bound hotkey, and starts it —
silently with `--autostart`, the flag its own single-instance handler
honours.

Settings are read, never written: pinned items, recents and the search index
belong to QuickDock. Disabling does not quit it; it persists its index on
exit, and force-terminating it would throw that away.

---

## Adding a module

1. Implement `CustomizationModule` in `src-tauri/src/modules/`.
2. Register it in `all_modules()`.
3. If it changes Windows, add a `SettingTarget` variant — capture,
   verification, conflict detection, restoration and the recovery CLI all
   come for free.
4. Add a panel in `src/pages/ModulePanel.tsx`.

The registry's own tests then check it automatically: unique id, non-empty
descriptor, dependencies and conflicts naming real modules, no self-
reference, a default config that passes its own validator, and stable
normalisation.
