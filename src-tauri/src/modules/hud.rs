//! System HUD — a compact always-visible strip of live metrics.
//!
//! Depends on the widget module because it is a widget surface in all but
//! placement: it shares the metric pipeline, the theme engine and the window
//! machinery. Declaring the dependency means the engine enables them in the
//! right order instead of the HUD finding half a framework.
//!
//! Metrics the machine cannot report are shown as unavailable. There is no
//! code path that substitutes a plausible-looking number for a missing sensor.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::monitoring;
use crate::windows::{SurfaceManager, SurfaceSpec, ZMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

pub const LABEL: &str = "hud";

/// Metrics the HUD can display, in the order they appear.
pub const METRICS: &[(&str, &str)] = &[
    ("cpu", "CPU"),
    ("ram", "Memory"),
    ("gpu", "GPU"),
    ("gpu_temp", "GPU temp"),
    ("battery", "Battery"),
    ("network", "Network"),
    ("storage", "Storage"),
    ("uptime", "Uptime"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudLayout {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudAnchor {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HudConfig {
    /// Metric ids to show, in display order.
    pub metrics: Vec<String>,
    pub layout: HudLayout,
    pub anchor: HudAnchor,
    pub opacity: f32,
    pub interval_ms: u64,
    pub compact: bool,
    /// Draw sparklines behind each metric.
    pub show_graphs: bool,
    pub click_through: bool,
    pub monitor: String,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            metrics: vec!["cpu".into(), "ram".into(), "network".into()],
            layout: HudLayout::Horizontal,
            anchor: HudAnchor::TopRight,
            opacity: 0.9,
            interval_ms: 2_000,
            compact: false,
            show_graphs: true,
            click_through: true,
            monitor: String::new(),
        }
    }
}

impl HudConfig {
    fn normalize(mut self) -> Result<Self> {
        let known: std::collections::HashSet<&str> = METRICS.iter().map(|(id, _)| *id).collect();
        let mut seen = std::collections::HashSet::new();
        for m in &self.metrics {
            if !known.contains(m.as_str()) {
                return Err(OverlayError::Config(format!("unknown HUD metric `{m}`")));
            }
            if !seen.insert(m.clone()) {
                return Err(OverlayError::Config(format!("metric `{m}` is listed twice")));
            }
        }
        if self.metrics.is_empty() {
            return Err(OverlayError::Config(
                "the HUD needs at least one metric to show".into(),
            ));
        }
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.2, 1.0)
        } else {
            0.9
        };
        self.interval_ms = self.interval_ms.clamp(500, 60_000);
        Ok(self)
    }

    pub fn window_size(&self) -> (f64, f64) {
        let n = self.metrics.len() as f64;
        let cell_w = if self.compact { 104.0 } else { 136.0 };
        let cell_h = if self.compact { 52.0 } else { 76.0 };
        match self.layout {
            HudLayout::Horizontal => (n * cell_w + 24.0, cell_h + 20.0),
            HudLayout::Vertical => (cell_w + 48.0, n * cell_h + 20.0),
        }
    }

    /// Position in logical pixels — the units Tauri places windows in, which
    /// are not the physical pixels Win32 reports for a scaled display.
    pub fn position(&self, m: &crate::sys::shell::MonitorInfo) -> (f64, f64) {
        let (w, h) = self.window_size();
        let b = m.logical();
        let (wx, wy) = (b.work_x, b.work_y);
        let (ww, wh) = (b.work_width, b.work_height);
        const GAP: f64 = 20.0;
        match self.anchor {
            HudAnchor::TopLeft => (wx + GAP, wy + GAP),
            HudAnchor::TopRight => (wx + ww - w - GAP, wy + GAP),
            HudAnchor::BottomLeft => (wx + GAP, wy + wh - h - GAP),
            HudAnchor::BottomRight => (wx + ww - w - GAP, wy + wh - h - GAP),
        }
    }
}

pub struct HudModule;

impl HudModule {
    fn surfaces(ctx: &ModuleCtx) -> Result<Arc<SurfaceManager>> {
        Ok(ctx.app()?.state::<Arc<SurfaceManager>>().inner().clone())
    }
}

impl CustomizationModule for HudModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "hud",
            name: "System HUD",
            description: "A compact strip of live system metrics pinned to a corner.",
            version: "1.0.0",
            category: Category::Widgets,
            requires: &["widgets"],
            conflicts_with: &[],
            touches_system: false,
        }
    }

    fn availability(&self, ctx: &ModuleCtx) -> Availability {
        if ctx.env.monitors.is_empty() {
            return Availability::unavailable("No displays were detected.");
        }
        let caps = monitoring::capabilities();
        if !caps.gpu_usage || !caps.battery {
            let mut missing = Vec::new();
            if !caps.gpu_usage {
                missing.push("GPU load");
            }
            if !caps.gpu_temperature {
                missing.push("GPU temperature");
            }
            if !caps.battery {
                missing.push("battery");
            }
            if !missing.is_empty() {
                return Availability::partial(format!(
                    "This machine does not report {}. Those metrics are shown as unavailable \
                     rather than estimated.",
                    missing.join(", ")
                ));
            }
        }
        Availability::Available
    }

    fn default_config(&self) -> Value {
        serde_json::to_value(HudConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: HudConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("HUD settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg.normalize()?)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: HudConfig = serde_json::from_value(self.validate_config(config)?)?;
        let app = ctx.app()?;

        let monitor = ctx
            .env
            .monitors
            .iter()
            .find(|m| !cfg.monitor.is_empty() && m.device == cfg.monitor)
            .or_else(|| ctx.env.monitors.iter().find(|m| m.primary))
            .or_else(|| ctx.env.monitors.first())
            .ok_or_else(|| OverlayError::other("no display to place the HUD on"))?;

        let (w, h) = cfg.window_size();
        let (x, y) = cfg.position(monitor);

        let mut spec = SurfaceSpec::new(LABEL, "hud")
            .at(x, y)
            .sized(w, h)
            .z(ZMode::Top);
        spec.click_through = cfg.click_through;

        Self::surfaces(ctx)?.show(app, spec)
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide(app, LABEL)
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        Self::surfaces(ctx).map(|s| s.is_open(LABEL)).unwrap_or(false)
    }

    fn notes(&self, _ctx: &ModuleCtx) -> Vec<String> {
        let caps = monitoring::capabilities();
        let mut notes = Vec::new();
        if !caps.gpu_usage {
            notes.push(format!("GPU load is unavailable: {}", caps.gpu_detail));
        }
        if caps.gpu_usage && !caps.gpu_temperature {
            notes.push("This GPU reports load but not temperature.".into());
        }
        if !caps.battery {
            notes.push("No battery was detected; this looks like a desktop.".into());
        }
        notes
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

    #[test]
    fn an_unknown_metric_is_refused() {
        let cfg = HudConfig {
            metrics: vec!["cpu".into(), "flux_capacitor".into()],
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn a_hud_with_no_metrics_is_refused() {
        // An empty HUD is an invisible window, which reads as a bug.
        let cfg = HudConfig {
            metrics: vec![],
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn a_duplicated_metric_is_refused() {
        let cfg = HudConfig {
            metrics: vec!["cpu".into(), "cpu".into()],
            ..Default::default()
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn every_catalogued_metric_is_accepted() {
        let cfg = HudConfig {
            metrics: METRICS.iter().map(|(id, _)| id.to_string()).collect(),
            ..Default::default()
        };
        assert!(cfg.normalize().is_ok());
    }

    #[test]
    fn each_anchor_keeps_the_hud_inside_the_work_area() {
        let m = screen();
        for anchor in [
            HudAnchor::TopLeft,
            HudAnchor::TopRight,
            HudAnchor::BottomLeft,
            HudAnchor::BottomRight,
        ] {
            let cfg = HudConfig {
                anchor,
                ..Default::default()
            };
            let (w, h) = cfg.window_size();
            let (x, y) = cfg.position(&m);
            assert!(x >= 0.0, "{anchor:?} x {x}");
            assert!(y >= 0.0, "{anchor:?} y {y}");
            assert!(x + w <= m.work_width as f64 + 1.0, "{anchor:?} overflows right");
            assert!(
                y + h <= m.work_height as f64 + 1.0,
                "{anchor:?} overlaps the taskbar"
            );
        }
    }

    #[test]
    fn the_hud_stays_on_screen_on_a_scaled_display() {
        // Regression: a 1920x1080 panel at 125% is 1536x864 to Tauri. Using
        // the physical width put the HUD at x=1460 on a 1536-wide desktop,
        // with 364 of its 440 pixels off the edge.
        let m = MonitorInfo {
            device: "\\\\.\\DISPLAY1".into(),
            primary: true,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            work_x: 0,
            work_y: 0,
            work_width: 1920,
            work_height: 1020,
            scale: 1.25,
        };
        let cfg = HudConfig {
            metrics: vec!["cpu".into(), "ram".into(), "network".into(), "storage".into()],
            compact: true,
            anchor: HudAnchor::TopRight,
            ..Default::default()
        };
        let (w, _) = cfg.window_size();
        let (x, _) = cfg.position(&m);
        let logical_width = m.logical().work_width;
        assert!(
            x + w <= logical_width + 1.0,
            "HUD at x={x} width={w} runs past the {logical_width}px logical desktop"
        );
        assert_eq!(x, logical_width - w - 20.0);
    }

    #[test]
    fn the_refresh_interval_is_clamped_to_something_sane() {
        let fast = HudConfig {
            interval_ms: 1,
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(fast.interval_ms, 500);

        let slow = HudConfig {
            interval_ms: u64::MAX,
            ..Default::default()
        }
        .normalize()
        .unwrap();
        assert_eq!(slow.interval_ms, 60_000);
    }

    #[test]
    fn layout_changes_the_window_shape() {
        let h = HudConfig {
            layout: HudLayout::Horizontal,
            ..Default::default()
        }
        .window_size();
        let v = HudConfig {
            layout: HudLayout::Vertical,
            ..Default::default()
        }
        .window_size();
        assert!(h.0 > h.1, "horizontal should be wide");
        assert!(v.1 > v.0, "vertical should be tall");
    }

    #[test]
    fn the_hud_declares_its_dependency_on_the_widget_framework() {
        assert_eq!(HudModule.descriptor().requires, &["widgets"]);
    }
}
