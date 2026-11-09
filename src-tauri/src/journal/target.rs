//! What a "system setting OVERLAY can change" is.
//!
//! The journal and the restoration engine are written entirely against these
//! two types and never against a specific module. That is what makes recovery
//! possible after a crash: the journal can put back any setting it has a row
//! for, in a process where the module that made the change was never even
//! constructed.
//!
//! Adding a new kind of system change means adding a variant here — and
//! getting capture, verification, conflict detection, restoration and the
//! recovery CLI for it at no extra cost.

use crate::error::{OverlayError, Result};
use crate::sys::accent::{self, AccentPolicy};
use crate::sys::registry::{self, RegTarget, RegValue};
use crate::sys::shell;
use serde::{Deserialize, Serialize};

/// One addressable piece of Windows state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "at", rename_all = "snake_case")]
pub enum SettingTarget {
    /// A single registry value.
    Registry(RegTarget),
    /// Whether desktop icons are drawn.
    DesktopIcons,
    /// The composition (translucency) policy on every taskbar window.
    TaskbarAccent,
}

impl SettingTarget {
    /// Stable primary key for this target in the database.
    pub fn id(&self) -> String {
        match self {
            SettingTarget::Registry(t) => format!("reg:{}", t.id()),
            SettingTarget::DesktopIcons => "shell:desktop-icons".into(),
            SettingTarget::TaskbarAccent => "shell:taskbar-accent".into(),
        }
    }

    /// Coarse type tag, stored alongside the row so a future schema can
    /// migrate one family of settings without parsing every payload.
    pub fn kind(&self) -> &'static str {
        match self {
            SettingTarget::Registry(_) => "registry",
            SettingTarget::DesktopIcons => "desktop_icons",
            SettingTarget::TaskbarAccent => "taskbar_accent",
        }
    }

    /// Whether a write to this target can be confirmed by reading it back.
    ///
    /// False for the taskbar accent, and this is a measured fact rather than
    /// a guess: Windows accepts `SetWindowCompositionAttribute` and visibly
    /// applies it, but `GetWindowCompositionAttribute` reports the zeroed
    /// policy regardless of what was written — see `tests/probe_accent.rs`,
    /// run against build 26100.
    ///
    /// For such a target the OS accepting the call is the strongest
    /// confirmation obtainable, so the journal records exactly that and does
    /// not claim a verification it cannot perform. The alternative — reading
    /// back, seeing "disabled" and declaring failure — fails a change that
    /// in fact worked, which is how an earlier version of this code rolled
    /// back every taskbar profile.
    ///
    /// Nothing is lost on the restore side: the accent lives only in the
    /// window, so it disappears when OVERLAY exits or Explorer restarts.
    pub fn verifiable(&self) -> bool {
        !matches!(self, SettingTarget::TaskbarAccent)
    }

    /// Human-readable name, used in the restoration report.
    pub fn label(&self) -> String {
        match self {
            SettingTarget::Registry(t) => format!("{} ({})", t.name, t.path),
            SettingTarget::DesktopIcons => "Desktop icons".into(),
            SettingTarget::TaskbarAccent => "Taskbar appearance".into(),
        }
    }

    /// Reads the live value.
    pub fn read(&self) -> Result<SettingValue> {
        match self {
            SettingTarget::Registry(t) => Ok(SettingValue::Reg(registry::read(t)?)),
            SettingTarget::DesktopIcons => Ok(SettingValue::Bool(shell::desktop_icons_visible()?)),
            SettingTarget::TaskbarAccent => Ok(SettingValue::Accent(
                accent::capture_all()
                    .into_iter()
                    .map(|(hwnd, policy)| AccentSnapshot { hwnd, policy })
                    .collect(),
            )),
        }
    }

    /// Writes a value, rejecting a value of the wrong shape for this target.
    pub fn write(&self, value: &SettingValue) -> Result<()> {
        match (self, value) {
            (SettingTarget::Registry(t), SettingValue::Reg(v)) => registry::write(t, v),
            (SettingTarget::DesktopIcons, SettingValue::Bool(v)) => {
                shell::set_desktop_icons_visible(*v)
            }
            (SettingTarget::TaskbarAccent, SettingValue::Accent(snaps)) => {
                // Taskbar handles do not survive an Explorer restart, so the
                // captured policy is re-applied by value to whatever taskbars
                // exist now rather than to the handles we recorded. Restoring
                // a stock desktop means the disabled policy, which is what an
                // empty capture also means.
                let policy = snaps
                    .first()
                    .map(|s| s.policy)
                    .unwrap_or_else(AccentPolicy::disabled);
                accent::apply_all(policy).map(|_| ())
            }
            _ => Err(OverlayError::Config(format!(
                "value {} cannot be written to {}",
                value.describe(),
                self.label()
            ))),
        }
    }

    /// Whether a readback counts as having arrived at `expected`.
    ///
    /// Not plain equality for every target: taskbar window handles change
    /// whenever Explorer restarts, so the policy is compared and the handles
    /// are ignored.
    pub fn matches(&self, expected: &SettingValue, found: &SettingValue) -> bool {
        match (expected, found) {
            (SettingValue::Accent(a), SettingValue::Accent(b)) => {
                let want = a.first().map(|s| s.policy).unwrap_or_default();
                // Every live taskbar must carry the policy we asked for.
                !b.is_empty() && b.iter().all(|s| s.policy == want)
            }
            _ => expected == found,
        }
    }
}

/// A captured accent policy and the window it came from. The handle is kept
/// for diagnostics only; restoration matches by value (see `matches`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccentSnapshot {
    pub hwnd: isize,
    pub policy: AccentPolicy,
}

/// The value of a [`SettingTarget`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SettingValue {
    Reg(RegValue),
    Bool(bool),
    Accent(Vec<AccentSnapshot>),
}

impl SettingValue {
    pub fn describe(&self) -> String {
        match self {
            SettingValue::Reg(v) => v.describe(),
            SettingValue::Bool(true) => "visible".into(),
            SettingValue::Bool(false) => "hidden".into(),
            SettingValue::Accent(s) => match s.first() {
                None => "no taskbar accent".into(),
                Some(f) if f.policy == AccentPolicy::disabled() => "system default".into(),
                Some(f) => format!(
                    "custom (state {}, {}% opaque)",
                    f.policy.state,
                    (f.policy.gradient_color >> 24) * 100 / 255
                ),
            },
        }
    }

    /// Convenience for the dword-valued registry settings, which are most of
    /// them.
    pub fn dword(v: u32) -> Self {
        SettingValue::Reg(RegValue::Dword(v))
    }

    pub fn absent() -> Self {
        SettingValue::Reg(RegValue::Absent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_ids_are_distinct_and_namespaced() {
        let a = SettingTarget::Registry(RegTarget::hkcu("P", "N")).id();
        let b = SettingTarget::DesktopIcons.id();
        let c = SettingTarget::TaskbarAccent.id();
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert!(a.starts_with("reg:"));
        assert!(b.starts_with("shell:"));
    }

    #[test]
    fn targets_and_values_round_trip_through_json() {
        // The journal stores both as JSON text, so this is the real format.
        let t = SettingTarget::Registry(RegTarget::hkcu("Soft\\X", "Y"));
        let s = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<SettingTarget>(&s).unwrap(), t);

        for v in [
            SettingValue::Reg(RegValue::Absent),
            SettingValue::Reg(RegValue::Dword(7)),
            SettingValue::Reg(RegValue::Binary(vec![1, 2, 3])),
            SettingValue::Bool(true),
            SettingValue::Accent(vec![AccentSnapshot {
                hwnd: 123,
                policy: AccentPolicy::new(crate::sys::accent::TaskbarFinish::Blurred, 0.5, 0x101010),
            }]),
        ] {
            let s = serde_json::to_string(&v).unwrap();
            assert_eq!(serde_json::from_str::<SettingValue>(&s).unwrap(), v);
        }
    }

    #[test]
    fn writing_a_mismatched_value_shape_is_refused() {
        let t = SettingTarget::DesktopIcons;
        let wrong = SettingValue::Reg(RegValue::Dword(1));
        assert!(t.write(&wrong).is_err());
    }

    #[test]
    fn accent_comparison_ignores_window_handles() {
        // Explorer restarts hand out new HWNDs; that must not read as drift.
        let policy = AccentPolicy::new(crate::sys::accent::TaskbarFinish::Acrylic, 0.8, 0x202020);
        let expected = SettingValue::Accent(vec![AccentSnapshot { hwnd: 1, policy }]);
        let found = SettingValue::Accent(vec![AccentSnapshot {
            hwnd: 999,
            policy,
        }]);
        assert!(SettingTarget::TaskbarAccent.matches(&expected, &found));
    }

    #[test]
    fn accent_comparison_still_catches_a_real_difference() {
        let a = AccentPolicy::new(crate::sys::accent::TaskbarFinish::Acrylic, 0.8, 0x202020);
        let b = AccentPolicy::new(crate::sys::accent::TaskbarFinish::Clear, 0.1, 0x202020);
        let expected = SettingValue::Accent(vec![AccentSnapshot { hwnd: 1, policy: a }]);
        let found = SettingValue::Accent(vec![AccentSnapshot { hwnd: 1, policy: b }]);
        assert!(!SettingTarget::TaskbarAccent.matches(&expected, &found));
    }

    #[test]
    fn an_empty_taskbar_readback_never_counts_as_success() {
        // No taskbar found means we could not confirm anything.
        let policy = AccentPolicy::disabled();
        let expected = SettingValue::Accent(vec![AccentSnapshot { hwnd: 1, policy }]);
        let found = SettingValue::Accent(vec![]);
        assert!(!SettingTarget::TaskbarAccent.matches(&expected, &found));
    }

    #[test]
    fn only_the_taskbar_accent_is_unverifiable() {
        // Everything else must be confirmed by reading it back; marking a
        // new target unverifiable should be a deliberate, visible decision.
        assert!(!SettingTarget::TaskbarAccent.verifiable());
        assert!(SettingTarget::DesktopIcons.verifiable());
        assert!(SettingTarget::Registry(RegTarget::hkcu("P", "N")).verifiable());
    }

    #[test]
    fn registry_comparison_is_exact() {
        let t = SettingTarget::Registry(RegTarget::hkcu("P", "N"));
        assert!(t.matches(&SettingValue::dword(1), &SettingValue::dword(1)));
        // The case that makes restoration honest.
        assert!(!t.matches(&SettingValue::absent(), &SettingValue::dword(0)));
    }
}
