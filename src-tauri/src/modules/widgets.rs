//! Desktop Widget Studio.
//!
//! A framework plus a catalogue. Each widget is an independently
//! enableable surface with its own position, size, appearance and refresh
//! rate; the module owns the collection and the window lifecycle.
//!
//! Widgets draw OVERLAY's own windows and change nothing about Windows, so
//! this module has no journal entries and `touches_system` is false. Turning
//! it off closes windows — there is nothing to restore.

use super::{Availability, Category, CustomizationModule, ModuleCtx, ModuleDescriptor};
use crate::error::{OverlayError, Result};
use crate::windows::{clamp_to_visible, SurfaceManager, SurfaceSpec, ZMode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tauri::Manager;

/// Window-label prefix for every widget surface. Also the pattern allowed in
/// `capabilities/default.json`.
pub const LABEL_PREFIX: &str = "widget-";

/// One entry in the widget catalogue: what a widget *is*, independent of how
/// any particular user has configured it.
/// Static catalogue data, only ever serialised outward to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct WidgetKind {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub default_width: f64,
    pub default_height: f64,
    pub min_width: f64,
    pub min_height: f64,
    /// Default refresh in milliseconds. 0 means the widget is event-driven or
    /// static and does not poll at all.
    pub default_interval_ms: u64,
    /// True when the widget reads a hardware sensor that may not exist, in
    /// which case it must render "unavailable" rather than a plausible number.
    pub needs_sensor: bool,
}

/// Everything OVERLAY can put on the desktop.
pub const CATALOG: &[WidgetKind] = &[
    WidgetKind {
        id: "clock",
        name: "Clock",
        description: "Large time display.",
        default_width: 380.0,
        default_height: 150.0,
        min_width: 160.0,
        min_height: 70.0,
        default_interval_ms: 1_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "date",
        name: "Date & Day",
        description: "Today's date and weekday.",
        default_width: 300.0,
        default_height: 110.0,
        min_width: 140.0,
        min_height: 60.0,
        default_interval_ms: 60_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "calendar",
        name: "Calendar",
        description: "This month at a glance.",
        default_width: 320.0,
        default_height: 310.0,
        min_width: 260.0,
        min_height: 260.0,
        default_interval_ms: 300_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "weather",
        name: "Weather",
        description: "Current conditions from a provider you configure.",
        default_width: 300.0,
        default_height: 160.0,
        min_width: 220.0,
        min_height: 120.0,
        default_interval_ms: 900_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "cpu",
        name: "CPU Usage",
        description: "Processor load.",
        default_width: 260.0,
        default_height: 130.0,
        min_width: 160.0,
        min_height: 90.0,
        default_interval_ms: 2_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "ram",
        name: "Memory",
        description: "RAM in use.",
        default_width: 260.0,
        default_height: 130.0,
        min_width: 160.0,
        min_height: 90.0,
        default_interval_ms: 2_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "gpu",
        name: "GPU Usage",
        description: "Graphics load, where the driver reports it.",
        default_width: 260.0,
        default_height: 130.0,
        min_width: 160.0,
        min_height: 90.0,
        default_interval_ms: 2_000,
        needs_sensor: true,
    },
    WidgetKind {
        id: "gpu_temp",
        name: "GPU Temperature",
        description: "Graphics temperature, where the driver reports it.",
        default_width: 260.0,
        default_height: 130.0,
        min_width: 160.0,
        min_height: 90.0,
        default_interval_ms: 4_000,
        needs_sensor: true,
    },
    WidgetKind {
        id: "battery",
        name: "Battery",
        description: "Charge level and power state.",
        default_width: 260.0,
        default_height: 130.0,
        min_width: 160.0,
        min_height: 90.0,
        default_interval_ms: 15_000,
        needs_sensor: true,
    },
    WidgetKind {
        id: "network",
        name: "Network Throughput",
        description: "Live upload and download rates.",
        default_width: 300.0,
        default_height: 140.0,
        min_width: 200.0,
        min_height: 100.0,
        default_interval_ms: 2_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "network_status",
        name: "Network Status",
        description: "Connection name and state.",
        default_width: 280.0,
        default_height: 110.0,
        min_width: 180.0,
        min_height: 80.0,
        default_interval_ms: 10_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "uptime",
        name: "System Uptime",
        description: "How long Windows has been running.",
        default_width: 280.0,
        default_height: 110.0,
        min_width: 180.0,
        min_height: 80.0,
        default_interval_ms: 30_000,
        needs_sensor: false,
    },
    WidgetKind {
        id: "media",
        name: "Now Playing",
        description: "Whatever is currently playing.",
        default_width: 340.0,
        default_height: 130.0,
        min_width: 240.0,
        min_height: 100.0,
        default_interval_ms: 3_000,
        needs_sensor: true,
    },
    WidgetKind {
        id: "storage",
        name: "Storage",
        description: "Free space on your drives.",
        default_width: 300.0,
        default_height: 180.0,
        min_width: 220.0,
        min_height: 120.0,
        default_interval_ms: 60_000,
        needs_sensor: false,
    },
];

pub fn kind(id: &str) -> Option<&'static WidgetKind> {
    CATALOG.iter().find(|w| w.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WidgetTheme {
    #[default]
    Glass,
    Solid,
    Outline,
    /// No panel at all — just the content over the wallpaper.
    Bare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WidgetFont {
    #[default]
    Sans,
    Mono,
    Display,
}

/// One widget as the user has set it up.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WidgetInstance {
    pub id: String,
    pub enabled: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Device name of the monitor it belongs on; empty means "wherever the
    /// coordinates land".
    #[serde(default)]
    pub monitor: String,
    /// 0.1–1.0.
    pub opacity: f32,
    pub theme: WidgetTheme,
    pub font: WidgetFont,
    /// 0.6–2.5, multiplied into the widget's type scale.
    pub font_scale: f32,
    pub interval_ms: u64,
    /// Position is fixed; the widget cannot be dragged.
    pub locked: bool,
    /// Mouse events pass through to the desktop.
    pub click_through: bool,
    pub z: ZMode,
    /// Widget-specific extras, e.g. the weather location.
    #[serde(default)]
    pub settings: Value,
}

impl WidgetInstance {
    pub fn from_kind(k: &WidgetKind, index: usize) -> Self {
        // Stagger defaults so enabling several widgets does not stack them
        // all in one spot.
        let column = (index / 4) as f64;
        let row = (index % 4) as f64;
        Self {
            id: k.id.to_string(),
            enabled: true,
            x: 80.0 + column * (k.default_width + 24.0),
            y: 80.0 + row * (k.default_height + 24.0),
            width: k.default_width,
            height: k.default_height,
            monitor: String::new(),
            opacity: 1.0,
            theme: WidgetTheme::default(),
            font: WidgetFont::default(),
            font_scale: 1.0,
            interval_ms: k.default_interval_ms,
            locked: false,
            click_through: false,
            z: ZMode::Desktop,
            settings: Value::Object(Default::default()),
        }
    }

    pub fn label(&self) -> String {
        format!("{LABEL_PREFIX}{}", self.id)
    }

    /// Clamps every field into a range the renderer can actually handle.
    ///
    /// This is the gate an imported profile passes through, so it must cope
    /// with hostile values — a zero interval would spin a timer at full tilt,
    /// and a negative size would fail window creation.
    fn normalize(mut self) -> Result<Self> {
        let Some(k) = kind(&self.id) else {
            return Err(OverlayError::Config(format!("unknown widget `{}`", self.id)));
        };
        if !self.x.is_finite() || !self.y.is_finite() {
            return Err(OverlayError::Config(format!(
                "widget `{}` has a non-numeric position",
                self.id
            )));
        }
        if !self.width.is_finite() || !self.height.is_finite() {
            return Err(OverlayError::Config(format!(
                "widget `{}` has a non-numeric size",
                self.id
            )));
        }
        self.width = self.width.clamp(k.min_width, 2_000.0);
        self.height = self.height.clamp(k.min_height, 2_000.0);
        self.opacity = if self.opacity.is_finite() {
            self.opacity.clamp(0.1, 1.0)
        } else {
            1.0
        };
        self.font_scale = if self.font_scale.is_finite() {
            self.font_scale.clamp(0.6, 2.5)
        } else {
            1.0
        };
        // A floor of 250 ms: anything faster costs CPU without being
        // perceptible. 0 keeps its meaning of "never poll".
        if self.interval_ms != 0 {
            self.interval_ms = self.interval_ms.clamp(250, 3_600_000);
        }
        if !self.settings.is_object() {
            self.settings = Value::Object(Default::default());
        }
        Ok(self)
    }

    fn spec(&self) -> SurfaceSpec {
        SurfaceSpec::new(self.label(), "widget")
            .param("id", &self.id)
            .at(self.x, self.y)
            .sized(self.width, self.height)
            .z(self.z)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WidgetsConfig {
    pub widgets: Vec<WidgetInstance>,
    /// Master switch for dragging. Individual widgets can still be locked.
    pub allow_drag: bool,
}

impl Default for WidgetsConfig {
    fn default() -> Self {
        // A new user gets a clock, nothing more. Enabling the module must not
        // cover someone's desktop in panels they did not ask for.
        let clock = kind("clock").expect("clock is in the catalogue");
        Self {
            widgets: vec![WidgetInstance::from_kind(clock, 0)],
            allow_drag: true,
        }
    }
}

impl WidgetsConfig {
    fn normalize(self) -> Result<Self> {
        let mut seen = std::collections::HashSet::new();
        let mut widgets = Vec::with_capacity(self.widgets.len());
        for w in self.widgets {
            if !seen.insert(w.id.clone()) {
                return Err(OverlayError::Config(format!(
                    "widget `{}` is listed twice",
                    w.id
                )));
            }
            widgets.push(w.normalize()?);
        }
        Ok(Self {
            widgets,
            allow_drag: self.allow_drag,
        })
    }

    pub fn enabled(&self) -> impl Iterator<Item = &WidgetInstance> {
        self.widgets.iter().filter(|w| w.enabled)
    }
}

pub struct WidgetsModule;

impl WidgetsModule {
    fn surfaces(ctx: &ModuleCtx) -> Result<Arc<SurfaceManager>> {
        Ok(ctx.app()?.state::<Arc<SurfaceManager>>().inner().clone())
    }
}

impl CustomizationModule for WidgetsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "widgets",
            name: "Desktop Widgets",
            description:
                "Clocks, calendars, system monitors and more, placed anywhere on your desktop.",
            version: "1.0.0",
            category: Category::Widgets,
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
        serde_json::to_value(WidgetsConfig::default()).expect("serialisable")
    }

    fn validate_config(&self, config: &Value) -> Result<Value> {
        let cfg: WidgetsConfig = serde_json::from_value(config.clone())
            .map_err(|e| OverlayError::Config(format!("widget settings are invalid: {e}")))?;
        Ok(serde_json::to_value(cfg.normalize()?)?)
    }

    fn capture_original(&self, _ctx: &ModuleCtx) -> Result<()> {
        // Nothing to capture: widgets are OVERLAY's own windows and change no
        // Windows setting.
        Ok(())
    }

    fn enable(&self, ctx: &ModuleCtx, config: &Value) -> Result<()> {
        let cfg: WidgetsConfig = serde_json::from_value(self.validate_config(config)?)?;
        let app = ctx.app()?;
        let surfaces = Self::surfaces(ctx)?;

        // Close any widget that is open but no longer wanted, before opening
        // the rest — otherwise a profile switch briefly shows both sets.
        let wanted: std::collections::HashSet<String> =
            cfg.enabled().map(|w| w.label()).collect();
        for label in surfaces.open_labels() {
            if label.starts_with(LABEL_PREFIX) && !wanted.contains(&label) {
                let _ = surfaces.hide(app, &label);
            }
        }

        let mut failures = Vec::new();
        for w in cfg.enabled() {
            let (x, y) = clamp_to_visible(w.x, w.y, w.width, w.height, &ctx.env.monitors);
            let mut spec = w.spec();
            spec.x = x;
            spec.y = y;
            spec.click_through = w.click_through;
            // One widget failing to open must not stop the others.
            if let Err(e) = surfaces.show(app, spec) {
                failures.push(format!("{}: {e}", w.id));
            }
        }

        if !failures.is_empty() && failures.len() == cfg.enabled().count() {
            return Err(OverlayError::other(format!(
                "no widgets could be shown: {}",
                failures.join("; ")
            )));
        }
        Ok(())
    }

    fn disable(&self, ctx: &ModuleCtx) -> Result<()> {
        let app = ctx.app()?;
        Self::surfaces(ctx)?.hide_matching(app, LABEL_PREFIX);
        Ok(())
    }

    fn is_active(&self, ctx: &ModuleCtx) -> bool {
        let Ok(surfaces) = Self::surfaces(ctx) else {
            return false;
        };
        surfaces
            .open_labels()
            .iter()
            .any(|l| l.starts_with(LABEL_PREFIX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_has_unique_ids_and_usable_defaults() {
        let mut ids = std::collections::HashSet::new();
        for k in CATALOG {
            assert!(ids.insert(k.id), "duplicate widget id {}", k.id);
            assert!(k.default_width >= k.min_width, "{} default < min", k.id);
            assert!(k.default_height >= k.min_height, "{} default < min", k.id);
            assert!(!k.name.is_empty() && !k.description.is_empty());
        }
        assert!(CATALOG.len() >= 14, "every promised widget should be listed");
    }

    #[test]
    fn enabling_the_module_shows_one_widget_not_fourteen() {
        let cfg = WidgetsConfig::default();
        assert_eq!(cfg.enabled().count(), 1);
        assert_eq!(cfg.widgets[0].id, "clock");
    }

    #[test]
    fn an_unknown_widget_is_rejected() {
        let mut cfg = WidgetsConfig::default();
        cfg.widgets[0].id = "teapot".into();
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn a_duplicated_widget_is_rejected() {
        // Two windows with the same label would fight over one surface.
        let clock = kind("clock").unwrap();
        let cfg = WidgetsConfig {
            widgets: vec![
                WidgetInstance::from_kind(clock, 0),
                WidgetInstance::from_kind(clock, 1),
            ],
            allow_drag: true,
        };
        assert!(cfg.normalize().is_err());
    }

    #[test]
    fn sizes_are_clamped_to_what_the_widget_can_render() {
        let clock = kind("clock").unwrap();
        let mut w = WidgetInstance::from_kind(clock, 0);
        w.width = 1.0;
        w.height = 1.0;
        let w = w.normalize().unwrap();
        assert_eq!(w.width, clock.min_width);
        assert_eq!(w.height, clock.min_height);
    }

    #[test]
    fn a_hostile_refresh_interval_cannot_spin_the_cpu() {
        let clock = kind("clock").unwrap();
        let mut w = WidgetInstance::from_kind(clock, 0);
        w.interval_ms = 1;
        assert_eq!(w.normalize().unwrap().interval_ms, 250);
    }

    #[test]
    fn a_zero_interval_keeps_its_meaning_of_never_poll() {
        let clock = kind("clock").unwrap();
        let mut w = WidgetInstance::from_kind(clock, 0);
        w.interval_ms = 0;
        assert_eq!(w.normalize().unwrap().interval_ms, 0);
    }

    #[test]
    fn non_finite_geometry_is_rejected_rather_than_clamped() {
        let clock = kind("clock").unwrap();
        let mut w = WidgetInstance::from_kind(clock, 0);
        w.x = f64::NAN;
        assert!(w.clone().normalize().is_err());

        let mut w = WidgetInstance::from_kind(clock, 0);
        w.width = f64::INFINITY;
        assert!(w.normalize().is_err());
    }

    #[test]
    fn opacity_and_font_scale_are_clamped() {
        let clock = kind("clock").unwrap();
        let mut w = WidgetInstance::from_kind(clock, 0);
        w.opacity = 50.0;
        w.font_scale = -4.0;
        let w = w.normalize().unwrap();
        assert_eq!(w.opacity, 1.0);
        assert_eq!(w.font_scale, 0.6);
    }

    #[test]
    fn default_positions_do_not_all_stack_on_one_spot() {
        let a = WidgetInstance::from_kind(kind("clock").unwrap(), 0);
        let b = WidgetInstance::from_kind(kind("cpu").unwrap(), 1);
        assert_ne!((a.x, a.y), (b.x, b.y));
    }

    #[test]
    fn widget_labels_match_the_capability_pattern() {
        for k in CATALOG {
            let w = WidgetInstance::from_kind(k, 0);
            assert!(
                w.label().starts_with(LABEL_PREFIX),
                "{} would not be permitted by capabilities/default.json",
                w.label()
            );
        }
    }

    #[test]
    fn validation_round_trips_every_catalogue_widget() {
        let m = WidgetsModule;
        let widgets: Vec<_> = CATALOG
            .iter()
            .enumerate()
            .map(|(i, k)| WidgetInstance::from_kind(k, i))
            .collect();
        let cfg = serde_json::to_value(WidgetsConfig {
            widgets,
            allow_drag: true,
        })
        .unwrap();
        let once = m.validate_config(&cfg).unwrap();
        assert_eq!(m.validate_config(&once).unwrap(), once);
    }
}
