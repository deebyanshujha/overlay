# Testing

```bash
npm test                 # frontend — 38 tests
cd src-tauri
cargo test               # Rust unit + contract — 205 tests
cargo test --test windows_live -- --ignored --nocapture   # live Windows
```

All suites pass on Windows 11 build 26100.

---

## What is covered, and why it is covered that way

### Rust unit tests — `cargo test --lib`

In-process, no Windows state. They cover the logic where a mistake would be
silent: the absent-vs-zero distinction, taskbar blob bit manipulation,
config clamping, module registry consistency, placement maths, metric
formatting, profile planning and switch markers.

A few are worth calling out because they encode product rules rather than
implementation details:

| Test | Rule it keeps |
| --- | --- |
| `absent_is_not_zero` | Restoring an unset value must delete it, not write 0 |
| `untouched_options_produce_no_registry_writes` | A default taskbar config must own nothing |
| `autohide_leaves_every_other_byte_identical` | One bit changes; 39 bytes do not |
| `every_default_config_passes_its_own_validator` | A module must be enableable out of the box |
| `validation_is_idempotent` | Saving a profile twice must not drift it |
| `every_builtin_profile_passes_module_validation_and_planning` | A shipped profile that cannot be applied is a broken product |
| `only_the_taskbar_accent_is_unverifiable` | Marking a target unverifiable must be deliberate |
| `volume_and_brightness_are_shortcuts_not_sliders` | No decorative controls |
| `the_default_hotkey_does_not_collide_with_quickdock` | Do not break the user's launcher |
| `logical_bounds_divide_physical_pixels_by_the_scale` | DPI: Win32 and Tauri use different units |

### Journal contract — `cargo test --test journal_contract`

**Runs against the real Windows registry**, in a scratch key of its own
(`HKCU\Software\OVERLAY\__journaltest__`), cleaned up by a `Drop` guard so a
failure cannot leave anything behind.

Real Win32 rather than a mock, deliberately: no mock reproduces the
absent-versus-zero distinction faithfully, and that distinction is the whole
basis of restoration. A mocked journal would pass while the product was
broken.

Fourteen tests covering: capture-once-and-never-again, absent-restore-
deletes, exact value restore, ownership release, refusing to touch unowned
settings, conflict detection on both apply and restore, per-module restore
isolation, both crash-recovery branches, and the user-visible change history.

### Live Windows tests — `cargo test --test windows_live -- --ignored`

Touch the real machine, so they are `#[ignore]`d by default. Each restores
whatever it changes, on every exit path including a failing assertion.

```
reports_a_sane_environment
absent_values_read_as_absent_not_as_zero
reads_real_taskbar_settings
write_then_restore_absence_leaves_no_trace
string_and_binary_values_survive_the_registry
finds_the_taskbar_and_reads_its_accent
desktop_icons_round_trip        ← hides icons and puts them back
```

### Probes — `probe_desktop2.rs`, `probe_accent.rs`

Not assertions so much as instruments. They answer questions whose answers
are build-dependent, and they are kept so the answers can be re-derived on
another machine:

* **Which window's visibility tracks desktop icons?** The `SysListView32`,
  not the `SHELLDLL_DefView`. Reading the container reports "visible"
  forever and turns the idempotent setter into a blind toggle.
* **Can the taskbar accent be read back?** No. Windows accepts the write and
  applies it visibly, but reports the zeroed policy. This is why that target
  is marked unverifiable.

```bash
cargo test --test probe_desktop2 -- --ignored --nocapture
cargo test --test probe_accent -- --ignored --nocapture
```

### Frontend — `npm test`

Vitest and Testing Library, mocking `services/api` — the one seam we own.
Mocking there means a test fails when *our* contract changes, not when
Tauri's does.

| File | Covers |
| --- | --- |
| `customize.test.tsx` | Independent toggles, unavailable modules shown with reasons, dependency notes, warnings passed through, config panels on demand, and **never reporting success when the backend refused** |
| `restore.test.tsx` | The dialog lists real settings before acting, states what will *not* happen, flags externally-changed settings, requires confirmation, and does not claim verification it lacks |
| `widgets.test.tsx` | Every metric widget renders a real reading or an explicit unavailable-with-reason — never a zero standing in for a missing sensor |

---

## Verified by hand on Windows

Automated tests cannot prove a window is on the desktop. These were done on
the real machine and confirmed by screenshot:

| | Result |
| --- | --- |
| First-launch onboarding renders, nothing enabled | ✓ |
| `--profile Minimal` → clock appears on the desktop above the wallpaper | ✓ |
| `--profile Developer` → clock, uptime, storage widgets, HUD with four live metrics, Dropzone | ✓ |
| Taskbar left-aligned and translucent under Developer | ✓ |
| HUD placed correctly on a 125%-scaled display | ✓ after the DPI fix |
| `--profile "Original Windows"` → every surface closed, every setting restored | ✓ |
| `--restore-original` with the app **not running** → settings restored, verified, exit 0 | ✓ |
| `--status` reports owned settings accurately before and after | ✓ |
| WallFlow and QuickDock kept running and unmodified throughout | ✓ |
| `original-windows.json` written with `"(not set)"` preserved | ✓ |

### Not yet verified on hardware this machine lacks

- Multi-monitor placement and monitor disconnection (single display here).
  The logic is unit-tested against synthetic multi-monitor geometry,
  including negative origins for left-hand displays.
- GPU load and temperature (no NVIDIA GPU here). Reported as unavailable
  with the reason, which *is* the tested path.
- Windows 10 behaviour. Win11-only settings are skipped by a tested code
  path, but the overall experience has not been exercised on 10.

These are listed in [LIMITATIONS.md](LIMITATIONS.md) rather than implied to
work.

---

## Bugs these suites caught

Worth recording, because each was found by a test rather than by a user:

1. **Binary registry values could not be serialised.** Internally-tagged
   serde enums cannot hold a `Vec<u8>` newtype — which is exactly the
   taskbar auto-hide blob. Caught by a round-trip test before any code
   depended on it.
2. **Crash recovery parsed display text as JSON.** `unsettled()` formats
   values for the UI; reconciliation needed the raw stored JSON. Every
   interrupted change would have been recorded as unrecoverable.
3. **Desktop icon visibility read the wrong window.** Always reported
   "visible", so the idempotent setter behaved as a blind toggle.
4. **Taskbar accent verification failed a change that had worked.** Rolled
   back every taskbar profile. Fixed by modelling verifiability honestly.
5. **HUD placed off-screen on a scaled display.** Physical versus logical
   pixels.
6. **`--profile Original Windows` matched nothing.** A multi-word name split
   across argv.
