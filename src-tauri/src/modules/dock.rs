//! Floating dock.
//!
//! # Why OVERLAY ships a dock at all
//!
//! The brief asked that QuickDock be reused if it already provided dock
//! functionality. It does not: QuickDock is a keyboard-first search palette
//! (see `integrations::quickdock` for the evidence), which solves a different
//! problem from a strip of pinned icons with running-app indicators. So both
//! exist, and the user picks either, both, or neither.
//!
//! The two are declared as conflicting only in the sense of *dock duty*: the
//! `quickdock` module represents "let QuickDock be my launcher", and enabling
//! OVERLAY's dock alongside it is still allowed — what is refused is enabling
//! two OVERLAY-managed docks, which cannot happen, so the conflict list stays
//! empty and the UI explains the relationship instead.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::windows::{SurfaceManager, SurfaceSpec, ZMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub const LABEL: &str = "dock";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DockEdge {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

/// A pinned application. `path` is resolved and validated when the dock is
/// configured, never taken straight from the frontend at launch time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedApp {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DockConfig {
    pub edge: DockEdge,
    /// Icon size in logical pixels.
    pub icon_size: f64,
    /// Peak scale under the cursor. 1.0 disables magnification.
    pub magnification: f64,
    pub auto_hide: bool,
    pub show_running_indicators: bool,
    pub show_recent: bool,
    pub opacity: f32,
    /// Device name of the monitor to place it on; empty means primary.
    pub monitor: String,
    pub pinned: Vec<PinnedApp>,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            edge: DockEdge::Bottom,
            icon_size: 48.0,
            magnification: 1.6,
            auto_hide: false,
            show_running_indicators: true,
            show_recent: false,
            opacity: 0.92,
            monitor: String::new(),
            pinned: Vec::new(),
        }
    }
}

impl DockConfig {
    fn normalize(mut self) -> Result<Self> {
        if !self.icon_size.is_finite() || !self.magnification.is_finite() {
            return Err(OverlayError::Config("dock sizes must be numbers".into()));
        }
        self.icon_size = self.icon_size.clamp(24.0, 128.0);
        self.magnification = self.magnification.clamp(1.0, 2.5);
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.2, 1.0)
        } else {
            0.92
        };

        let mut seen = std::collections::HashSet::new();
        for p in &self.pinned {
            if p.name.trim().is_empty() {
                return Err(OverlayError::Config("a pinned item has no name".into()));
            }
            if !seen.insert(p.id.clone()) {
                return Err(OverlayError::Config(format!(
                    "pinned item `{}` is listed twice",
                    p.id
                )));
            }
            // An imported profile could name anything; refuse a relative path
            // so a launch can never be resolved against the working directory.
            if !p.path.is_absolute() {
                return Err(OverlayError::Config(format!(
                    "pinned item `{}` must use a full path",
                    p.name
                )));
            }
        }
        Ok(self)
    }

    /// Size of the dock window for a given number of items.
    pub fn window_size(&self) -> (f64, f64) {
        let count = self.pinned.len().max(1) as f64;
        let cell = self.icon_size + 16.0;
        // Headroom for the magnified icon and the running indicator.
        let thickness = self.icon_size * self.magnification + 34.0;
        let length = count * cell + 28.0;
        match self.edge {
            DockEdge::Bottom | DockEdge::Top => (length, thickness),
            DockEdge::Left | DockEdge::Right => (thickness, length),
        }
    }

    /// Where the dock sits on a monitor's work area, in logical pixels —
    /// the units Tauri places windows in, which are not the physical pixels
    /// Win32 reports for a scaled display.
    pub fn position(&self, m: &crate::sys::shell::MonitorInfo) -> (f64, f64) {
        let (w, h) = self.window_size();
        let b = m.logical();
        let (wx, wy) = (b.work_x, b.work_y);
        let (ww, wh) = (b.work_width, b.work_height);
        const GAP: f64 = 12.0;
        match self.edge {
            DockEdge::Bottom => (wx + (ww - w) / 2.0, wy + wh - h - GAP),
            DockEdge::Top => (wx + (ww - w) / 2.0, wy + GAP),
            DockEdge::Left => (wx + GAP, wy + (wh - h) / 2.0),
            DockEdge::Right => (wx + ww - w - GAP, wy + (wh - h) / 2.0),
        }
    }
}

pub struct DockModule;

impl DockModule {
    fn surfaces(ctx: &ModuleCtx) -> Result<Arc<SurfaceManager>> {
        Ok(ctx.app()?.state::<Arc<SurfaceManager>>().inner().clone())
    }
}

impl CustomizationModule for DockModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "dock",
            name: "Floating Dock",
            description:
                "A dock of pinned applications with hover magnification and running indicators.",
            version: "1.0.0",
            category: Category::Surfaces,
            requires: &[],
            conflicts_with: &[],
            touches_system: false,
        }
    }

    fn availability(&self, ctx: &ModuleCtx) -> Availability {
        if ctx.env.monitors.is_empty() {
            return Availability::unavailable("No displays were detected.");
        }
        Availability::Available
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(DockConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: DockConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("dock settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg.normalize()?)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: DockConfig = serde_json::from_value(self.validate_config(config)?)?;
        let app = ctx.app()?;

        let monitor = ctx
            .env
            .monitors
            .iter()
            .find(|m| !cfg.monitor.is_empty() && m.device == cfg.monitor)
            .or_else(|| ctx.env.monitors.iter().find(|m| m.primary))
            .or_else(|| ctx.env.monitors.first())
            .ok_or_else(|| OverlayError::other("no display to place the dock on"))?;

        let (w, h) = cfg.window_size();
        let (x, y) = cfg.position(monitor);

        Self::surfaces(ctx)?.show(
            app,
            SurfaceSpec::new(LABEL, "dock")
                .at(x, y)
                .sized(w, h)
                // A dock the user clicks must be reachable above the desktop
                // but must not cover a maximised window, so Normal rather
                // than Top or Desktop.
                .z(ZMode::Normal),
        )
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide(app, LABEL)
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        Self::surfaces(ctx)
            .map(|s| s.is_open(LABEL))
            .unwrap_or(false)
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        vec![
            "QuickDock is a keyboard-first search palette rather than an icon dock, so the two \
             do different jobs and can be used together."
                .into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sys::shell::MonitorInfo;

    fn screen() -> MonitorInfo {
        MonitorInfo {
            device: "\\\\.\\DISPLAY1".into(),
            primary: true,
            x: 0,
            y: 0,
            width: 1536,
            height: 864,
            work_x: 0,
            work_y: 0,
            work_width: 1536,
            work_height: 816,
            scale: 1.0,
        }
    }

    fn with_pins(n: usize) -> DockConfig {
        DockConfig {
            pinned: (0..n)
                .map(|i| PinnedApp {
                    id: format!("a{i}"),
                    name: format!("App {i}"),
                    path: PathBuf::from(format!("C:\\Apps\\a{i}.exe")),
                    args: vec![],
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn the_dock_grows_with_the_number_of_pins() {
        let (w1, _) = with_pins(1).window_size();
        let (w5, _) = with_pins(5).window_size();
        assert!(w5 > w1);
    }

    #[test]
    fn an_empty_dock_still_has_a_usable_size() {
        let (w, h) = DockConfig::default().window_size();
        assert!(w > 0.0 && h > 0.0);
    }

    #[test]
    fn a_bottom_dock_sits_inside_the_work_area() {
        // Above the taskbar, not under it.
        let cfg = with_pins(4);
        let m = screen();
        let (x, y) = cfg.position(&m);
        let (w, h) = cfg.window_size();
        assert!(x >= 0.0, "x {x}");
        assert!(y + h <= m.work_height as f64, "dock overlaps the taskbar");
        assert!(x + w <= m.width as f64);
    }

    #[test]
    fn each_edge_puts_the_dock_somewhere_different() {
        let m = screen();
        let mut seen = std::collections::HashSet::new();
        for edge in [DockEdge::Bottom, DockEdge::Top, DockEdge::Left, DockEdge::Right] {
            let cfg = DockConfig {
                edge,
                ..with_pins(3)
            };
            let (x, y) = cfg.position(&m);
            assert!(seen.insert(format!("{x}:{y}")), "{edge:?} duplicated a position");
        }
    }

    #[test]
    fn a_relative_pinned_path_is_refused() {
        // Otherwise an imported profile could launch something resolved
        // against whatever directory OVERLAY happened to start in.
        let cfg = DockConfig {
            pinned: vec![PinnedApp {
                id: "x".into(),
                name: "X".into(),
                path: PathBuf::from("..\\..\\Windows\\System32\\cmd.exe"),
                args: vec![],
            }],
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn duplicate_and_nameless_pins_are_refused() {
        let mut cfg = with_pins(2);
        cfg.pinned[1].id = cfg.pinned[0].id.clone();
        assert!(cfg.clone().normalize().is_err());

        let mut cfg = with_pins(1);
        cfg.pinned[0].name = "   ".into();
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn icon_size_and_magnification_are_clamped() {
        let cfg = DockConfig {
            icon_size: 9000.0,
            magnification: 40.0,
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(cfg.icon_size, 128.0);
        assert_eq!(cfg.magnification, 2.5);
    }
}
