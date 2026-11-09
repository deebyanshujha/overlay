# Restoration

The promise: **every change OVERLAY makes can be undone, and nothing else is
touched.**

This document is how that is kept.

---

## The contract

Every system modification passes through `Journal::apply`, in this order:

```
1. CAPTURE    the original value — once, ever
2. CHECK      that nobody else has changed it since we did
3. JOURNAL    a `pending` row, before touching Windows
4. APPLY      the change
5. VERIFY     by reading it back
6. SETTLE     the row to `applied` or `failed`, and record ownership
```

Step 3 is what makes a crash recoverable: a process killed between 3 and 6
leaves a `pending` row naming exactly one target, and the next start finds it.

Step 1 uses `INSERT OR IGNORE`. The first capture wins, so no amount of later
activity can overwrite the pre-OVERLAY truth with an OVERLAY-written value.

---

## Absence is a value

This is the single most important detail.

Most Windows personalisation settings do not exist until something writes
them. On the machine OVERLAY was built on, `TaskbarAl`, `TaskbarSi` and
`EnableTransparency` were all absent — Explorer was falling back to built-in
defaults.

If OVERLAY restored those by writing `0`, it would not be restoring. It would
be pinning a value the user never set, permanently, and silently changing how
the setting behaves across future Windows updates.

So absence is modelled explicitly:

```rust
pub enum RegValue { Absent, Dword(u32), Str(String), Binary(Vec<u8>) }
```

Restoring `Absent` **deletes** the value. The snapshot file records it as
`"(not set)"`. And `assert_ne!(RegValue::Absent, RegValue::Dword(0))` is a
test, because the day those compare equal is the day restore quietly stops
working.

---

## What restore will and will not do

**It will:**

- Put back every setting in `owned_settings`, verified by reading it back.
- Close every OVERLAY window.
- Report, per setting, exactly what happened.
- Keep going after an individual failure, so one stubborn setting cannot
  strand the rest.

**It will not:**

- Touch a setting OVERLAY did not change. The work list *is* `owned_settings`;
  there is no code path to anything else.
- Overwrite a setting someone changed after OVERLAY did. Those are reported
  as `ConflictSkipped` and left exactly as the user set them.
- Uninstall, close or reconfigure WallFlow or QuickDock.
- Delete a shortcut, move a file, or write to the Desktop folder. No code in
  OVERLAY does.
- Claim success it has not verified.

### Outcomes

| Status | Meaning |
| --- | --- |
| `Restored` | Put back and confirmed |
| `AlreadyOriginal` | Was already at its original value |
| `ConflictSkipped` | Changed outside OVERLAY since we set it — left alone, reported |
| `Failed` | Could not be put back; the original is still recorded, and it can be retried |

---

## Ownership and conflicts

A setting is *owned* from the moment OVERLAY writes it until it is restored.
Before each write, OVERLAY compares the live value against what it last
wrote:

- unchanged → proceed
- different → **`OwnershipConflict`**, and the write is refused

The same check runs during restore. If you changed your taskbar alignment by
hand after OVERLAY set it, OVERLAY will not quietly take it back. It reports
the conflict and leaves your value in place.

---

## What cannot be verified, and why we say so

One target is write-only: the **taskbar accent**.

Windows accepts `SetWindowCompositionAttribute` and visibly applies it, but
`GetWindowCompositionAttribute` reports the zeroed policy regardless of what
was written. Measured on build 26100; `tests/probe_accent.rs` re-derives it
on any machine.

An earlier version of this code verified by readback, saw "disabled", and
declared failure — rolling back every taskbar profile that had in fact
worked. The fix was not to pretend harder but to say less:

```rust
/// Whether a write to this target can be confirmed by reading it back.
pub fn verifiable(&self) -> bool {
    !matches!(self, SettingTarget::TaskbarAccent)
}
```

For an unverifiable target, the journal records *"Windows accepted the
change; this setting cannot be read back to confirm it"* — which is the
strongest true statement available. Nothing is lost on the restore side: the
accent lives only in the window, so it disappears when OVERLAY exits or
Explorer restarts.

---

## Crash recovery

At every start, OVERLAY:

1. Finds `pending` journal rows — changes begun but never settled.
2. Reads each target's live value and settles the row to what actually
   happened: `applied` if the write landed before the crash, `failed` if not.
   A change that landed becomes *owned*, so it is still restorable.
3. Checks for an in-flight profile switch. If one is found, it re-applies the
   destination (idempotent, and what the user asked for). If that fails, it
   falls back to Original Windows, which is always reachable.
4. Clears the marker *before* retrying, so a switch that crashes the same way
   twice does not loop.

The user is told what happened, in the banner at the top of the studio.

---

## Emergency recovery

```
overlay.exe --restore-original
```

Runs with no Tauri runtime, no window, and no frontend. It opens the database
directly, reconciles any interrupted change, reverts every owned setting and
prints a report:

```
OVERLAY — restore original desktop
==================================

1 setting(s) restored

Settings
--------
  [restored ] Taskbar appearance -> system default

Verified: every tracked setting is back at its original value.
```

Exit code `0` when fully verified, `2` when partial, `1` on failure — so a
script can tell the difference.

`overlay.exe --status` lists what is owned and changes nothing.

---

## If the database is lost

Alongside it, OVERLAY writes `original-windows.json` — a plain-text mirror of
every captured original, rewritten whenever a new one is recorded:

```json
{
  "note": "What this machine's settings were before OVERLAY changed them …",
  "originals": [
    {
      "description": "TaskbarAl (Software\\Microsoft\\…\\Advanced)",
      "original_value": "(not set)",
      "module": "taskbar",
      "captured_at": "2026-11-09T07:56:40+00:00",
      "raw": { "type": "reg", "value": { "type": "absent" } }
    }
  ]
}
```

It is redundant with the database on purpose. OVERLAY restores from the
database; this file exists so that a person can put things back by hand if
the database is ever lost, and so that "what did it change?" is answerable
without running anything. It is written atomically — temp file, then rename
— so a crash mid-write cannot replace a good snapshot with a truncated one.

A backup of the database is also taken before every restore and every profile
switch, into `backups\`.

---

## Testing this

`src-tauri/tests/journal_contract.rs` exercises the whole contract against the
real Windows registry, using a scratch key of its own
(`HKCU\Software\OVERLAY\__journaltest__`). Real Win32, no mocks — because no
mock reproduces the absent/zero distinction faithfully.

It covers: capture-once, absent-restore-deletes, exact value restore,
ownership release, refusing to touch unowned settings, conflict detection on
apply and on restore, per-module restore isolation, and both crash-recovery
branches.
