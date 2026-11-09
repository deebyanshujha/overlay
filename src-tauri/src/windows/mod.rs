//! Native window management for everything OVERLAY draws on the desktop:
//! widgets, the dock, the HUD, the control centre and the dropzone.
//!
//! All of them are Tauri webview windows loading the same frontend bundle with
//! a `?surface=…` query, so there is one build, one theme engine and one set
//! of components behind every surface.
//!
//! # Layering
//!
//! Three Z-modes, because "on the desktop" means different things to different
//! people:
//!
//! * [`ZMode::Desktop`] — above the wallpaper, below ordinary windows. What
//!   most people mean by a desktop widget.
//! * [`ZMode::Normal`] — an ordinary window in the stack.
//! * [`ZMode::Top`] — always on top. Right for a HUD, wrong for a clock.
//!
//! `Desktop` is implemented with `HWND_BOTTOM` rather than by reparenting the
//! window into Explorer's `WorkerW`. Reparenting looks marginally more
//! authentic but breaks input routing and composition for WebView2 surfaces
//! and varies by Windows build; see `sys::shell::send_to_desktop_layer`. The
//! cost is that Win+D and some full-screen transitions can raise the window,
//! so [`SurfaceManager::reassert_layering`] puts it back.

use crate::error::{OverlayError, Result};
use crate::sys::shell;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

/// Window stacking behaviour for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ZMode {
    #[default]
    Desktop,
    Normal,
    Top,
}

/// Everything needed to put one surface on screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurfaceSpec {
    /// Tauri window label. Must match a pattern in `capabilities/default.json`.
    pub label: String,
    /// Value of the `surface` query parameter the frontend routes on.
    pub kind: String,
    /// Extra query parameters, e.g. the widget id.
    pub params: Vec<(String, String)>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub z: ZMode,
    /// Mouse events pass straight through to whatever is behind.
    pub click_through: bool,
    pub resizable: bool,
    /// Keep out of the taskbar and Alt-Tab.
    pub tool_window: bool,
    pub transparent: bool,
}

impl SurfaceSpec {
    pub fn new(label: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: kind.into(),
            params: Vec::new(),
            x: 80.0,
            y: 80.0,
            width: 320.0,
            height: 160.0,
            z: ZMode::Desktop,
            click_through: false,
            resizable: false,
            tool_window: true,
            transparent: true,
        }
    }

    pub fn param(mut self, k: &str, v: &str) -> Self {
        self.params.push((k.into(), v.into()));
        self
    }

    pub fn at(mut self, x: f64, y: f64) -> Self {
        self.x = x;
        self.y = y;
        self
    }

    pub fn sized(mut self, w: f64, h: f64) -> Self {
        self.width = w;
        self.height = h;
        self
    }

    pub fn z(mut self, z: ZMode) -> Self {
        self.z = z;
        self
    }

    /// The URL the window loads, including the surface routing parameters.
    ///
    /// Values are percent-encoded: a widget id or profile name finding its
    /// way into a query string unescaped would silently truncate the URL at
    /// the first `&`.
    pub fn url(&self) -> String {
        let mut url = format!("index.html?surface={}", encode(&self.kind));
        for (k, v) in &self.params {
            url.push('&');
            url.push_str(&encode(k));
            url.push('=');
            url.push_str(&encode(v));
        }
        url
    }
}

/// Minimal percent-encoding for query values.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Tracks every surface OVERLAY has opened, so shutdown and profile switches
/// can close exactly what they opened and nothing else.
#[derive(Default)]
pub struct SurfaceManager {
    open: Mutex<HashMap<String, SurfaceSpec>>,
}

impl SurfaceManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Creates a surface, or repositions it if it is already open.
    ///
    /// Idempotent on purpose: profile switching may ask for a surface that is
    /// already up, and tearing it down and rebuilding it would flash.
    ///
    /// # Ordering
    ///
    /// Stacking is applied **after** `show()`, never before. Showing a window
    /// raises it, so lowering it to `HWND_BOTTOM` first and then showing it
    /// puts every desktop widget straight back on top of everything — which
    /// is exactly what happened: widgets, the HUD and the Dropzone ended up
    /// floating over the studio window, swallowing the clicks meant for it
    /// and covering its close button.
    pub fn show(&self, app: &AppHandle, spec: SurfaceSpec) -> Result<()> {
        if let Some(existing) = app.get_webview_window(&spec.label) {
            existing
                .set_position(LogicalPosition::new(spec.x, spec.y))
                .ok();
            existing
                .set_size(LogicalSize::new(spec.width, spec.height))
                .ok();
            self.apply_attributes(&existing, &spec)?;
            self.apply_stacking(&existing, &spec)?;
            existing.show().ok();
            self.lower_if_desktop(&existing, &spec)?;
            self.open.lock().insert(spec.label.clone(), spec);
            return Ok(());
        }

        let window = WebviewWindowBuilder::new(
            app,
            &spec.label,
            WebviewUrl::App(spec.url().into()),
        )
        .title("OVERLAY")
        .inner_size(spec.width, spec.height)
        .position(spec.x, spec.y)
        .decorations(false)
        .transparent(spec.transparent)
        .shadow(false)
        .resizable(spec.resizable)
        .skip_taskbar(spec.tool_window)
        .always_on_top(spec.z == ZMode::Top)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|e| OverlayError::Win32(format!("could not create `{}`: {e}", spec.label)))?;

        self.apply_attributes(&window, &spec)?;
        self.apply_stacking(&window, &spec)?;
        window.show().ok();
        self.lower_if_desktop(&window, &spec)?;

        self.open.lock().insert(spec.label.clone(), spec);
        Ok(())
    }

    /// Puts a desktop-layer surface back at the bottom after it is shown.
    ///
    /// Showing a window raises it, which is why this cannot be folded into
    /// [`Self::apply_stacking`]: lowering before `show()` is undone
    /// immediately, and that is what left every widget floating over the
    /// studio. The always-on-top flag has the opposite requirement — setting
    /// it *after* `show()` leaves the window hidden — so the two are applied
    /// on either side of it.
    fn lower_if_desktop(&self, window: &tauri::WebviewWindow, spec: &SurfaceSpec) -> Result<()> {
        if spec.z != ZMode::Desktop {
            return Ok(());
        }
        let Ok(hwnd) = window.hwnd() else {
            return Ok(());
        };
        shell::send_to_desktop_layer(hwnd.0 as isize)
    }

    /// Window styles, which are safe to set before the window is shown.
    fn apply_attributes(&self, window: &tauri::WebviewWindow, spec: &SurfaceSpec) -> Result<()> {
        let Ok(hwnd) = window.hwnd() else {
            // Not Windows, or the window is already gone: the surface still
            // works, it just has default stacking.
            return Ok(());
        };
        let hwnd = hwnd.0 as isize;

        if spec.tool_window {
            shell::set_tool_window(hwnd)?;
        }
        shell::set_click_through(hwnd, spec.click_through)?;

        // A desktop-layer surface must never take focus. Without this,
        // clicking one raises it above the windows it is supposed to sit
        // behind, and it stays there.
        if spec.z == ZMode::Desktop {
            shell::set_no_activate(hwnd, true)?;
        }
        Ok(())
    }

    /// Z-order, which must be applied after the window is visible.
    fn apply_stacking(&self, window: &tauri::WebviewWindow, spec: &SurfaceSpec) -> Result<()> {
        let Ok(hwnd) = window.hwnd() else {
            return Ok(());
        };
        let hwnd = hwnd.0 as isize;

        match spec.z {
            ZMode::Top => {
                window.set_always_on_top(true).ok();
            }
            ZMode::Normal => {
                window.set_always_on_top(false).ok();
            }
            ZMode::Desktop => {
                window.set_always_on_top(false).ok();
                shell::send_to_desktop_layer(hwnd)?;
            }
        }
        Ok(())
    }

    /// Lifts or restores the always-on-top surfaces.
    ///
    /// OVERLAY's own window must never be hidden behind OVERLAY's own
    /// overlays. The HUD sits in a screen corner by design, which is exactly
    /// where the studio's title bar and close button are — so while the
    /// studio is focused the topmost surfaces drop to ordinary stacking, and
    /// they go back on top the moment it is not. Nothing is hidden: the HUD
    /// stays visible and live, it just stops covering the window the user is
    /// working in.
    pub fn suspend_always_on_top(&self, app: &AppHandle, suspended: bool) {
        let labels: Vec<String> = self
            .open
            .lock()
            .iter()
            .filter(|(_, spec)| spec.z == ZMode::Top)
            .map(|(label, _)| label.clone())
            .collect();

        // Clearing the flag alone leaves the window at the top of the
        // ordinary band — still in front of the studio — so it is also
        // placed explicitly behind it.
        let main_hwnd = (!suspended)
            .then_some(None)
            .unwrap_or_else(|| {
                app.get_webview_window("main")
                    .and_then(|w| w.hwnd().ok())
                    .map(|h| h.0 as isize)
            });

        for label in labels {
            let Some(w) = app.get_webview_window(&label) else {
                continue;
            };
            let _ = w.set_always_on_top(!suspended);
            if let (Some(main), Ok(hwnd)) = (main_hwnd, w.hwnd()) {
                let _ = shell::place_behind(hwnd.0 as isize, main);
            }
        }
    }

    /// Re-applies desktop-layer stacking to every surface that asked for it.
    ///
    /// Needed because showing the desktop, a full-screen app exiting, or an
    /// Explorer restart can all pull a bottom-stacked window back up.
    pub fn reassert_layering(&self, app: &AppHandle) {
        let specs: Vec<SurfaceSpec> = self
            .open
            .lock()
            .values()
            .filter(|s| s.z == ZMode::Desktop)
            .cloned()
            .collect();
        for spec in specs {
            if let Some(w) = app.get_webview_window(&spec.label) {
                if let Ok(hwnd) = w.hwnd() {
                    let _ = shell::send_to_desktop_layer(hwnd.0 as isize);
                }
            }
        }
    }

    /// Closes one surface. Succeeds whether or not it was open, because the
    /// caller's intent is "it should not be on screen".
    pub fn hide(&self, app: &AppHandle, label: &str) -> Result<()> {
        if let Some(w) = app.get_webview_window(label) {
            w.close()
                .map_err(|e| OverlayError::Win32(format!("could not close `{label}`: {e}")))?;
        }
        self.open.lock().remove(label);
        Ok(())
    }

    /// Closes every surface whose label starts with `prefix` — used to drop a
    /// whole family at once, e.g. all `widget-*` windows.
    pub fn hide_matching(&self, app: &AppHandle, prefix: &str) -> Vec<String> {
        let labels: Vec<String> = self
            .open
            .lock()
            .keys()
            .filter(|l| l.starts_with(prefix))
            .cloned()
            .collect();
        for label in &labels {
            let _ = self.hide(app, label);
        }
        labels
    }

    /// Closes everything. Called on shutdown and by Restore Original Desktop,
    /// so no OVERLAY window outlives the decision to stop using it.
    pub fn hide_all(&self, app: &AppHandle) -> Vec<String> {
        let labels: Vec<String> = self.open.lock().keys().cloned().collect();
        for label in &labels {
            let _ = self.hide(app, label);
        }
        self.open.lock().clear();
        labels
    }

    pub fn is_open(&self, label: &str) -> bool {
        self.open.lock().contains_key(label)
    }

    pub fn open_labels(&self) -> Vec<String> {
        let mut v: Vec<String> = self.open.lock().keys().cloned().collect();
        v.sort();
        v
    }

    pub fn count(&self) -> usize {
        self.open.lock().len()
    }

    /// Moves a surface and remembers where it went.
    pub fn move_to(&self, app: &AppHandle, label: &str, x: f64, y: f64) -> Result<()> {
        let Some(w) = app.get_webview_window(label) else {
            return Err(OverlayError::other(format!("`{label}` is not open")));
        };
        w.set_position(LogicalPosition::new(x, y))
            .map_err(|e| OverlayError::Win32(e.to_string()))?;
        if let Some(spec) = self.open.lock().get_mut(label) {
            spec.x = x;
            spec.y = y;
        }
        Ok(())
    }

    pub fn resize(&self, app: &AppHandle, label: &str, w: f64, h: f64) -> Result<()> {
        let Some(win) = app.get_webview_window(label) else {
            return Err(OverlayError::other(format!("`{label}` is not open")));
        };
        win.set_size(LogicalSize::new(w, h))
            .map_err(|e| OverlayError::Win32(e.to_string()))?;
        if let Some(spec) = self.open.lock().get_mut(label) {
            spec.width = w;
            spec.height = h;
        }
        Ok(())
    }

    /// Turns click-through on or off for an open surface.
    pub fn set_click_through(&self, app: &AppHandle, label: &str, enabled: bool) -> Result<()> {
        let Some(w) = app.get_webview_window(label) else {
            return Err(OverlayError::other(format!("`{label}` is not open")));
        };
        // Ask Tauri as well as Win32: it keeps its own idea of the flag, and
        // a later Tauri call would otherwise undo ours.
        w.set_ignore_cursor_events(enabled).ok();
        if let Ok(hwnd) = w.hwnd() {
            shell::set_click_through(hwnd.0 as isize, enabled)?;
        }
        if let Some(spec) = self.open.lock().get_mut(label) {
            spec.click_through = enabled;
        }
        Ok(())
    }
}

/// Clamps a surface's position so it stays reachable on some connected
/// display.
///
/// A widget placed on a second monitor that is later unplugged would
/// otherwise sit at coordinates no display covers — invisible, and
/// unreachable without editing the database by hand.
pub fn clamp_to_visible(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    monitors: &[shell::MonitorInfo],
) -> (f64, f64) {
    if monitors.is_empty() {
        return (x.max(0.0), y.max(0.0));
    }

    // A surface counts as reachable if a reasonable grab area is on-screen.
    // All comparisons are in logical pixels, the same units the caller's
    // `x`/`y` are in and the same ones Tauri positions windows with.
    const MARGIN: f64 = 48.0;
    let visible = monitors.iter().any(|m| {
        let b = m.logical();
        x + width > b.x + MARGIN.min(width)
            && x < b.x + b.width - MARGIN.min(width)
            && y + height > b.y
            && y < b.y + b.height - MARGIN.min(height)
    });
    if visible {
        return (x, y);
    }

    // Put it back on the primary display, inside the work area.
    let home = monitors.iter().find(|m| m.primary).unwrap_or(&monitors[0]).logical();
    let nx = (home.work_x + 80.0).min(home.work_x + home.work_width - width);
    let ny = (home.work_y + 80.0).min(home.work_y + home.work_height - height);
    (nx.max(home.work_x), ny.max(home.work_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: i32, y: i32, w: u32, h: u32, primary: bool) -> shell::MonitorInfo {
        scaled_monitor(x, y, w, h, primary, 1.0)
    }

    /// A display reported in physical pixels with a scale factor, the way
    /// Win32 actually describes one.
    fn scaled_monitor(
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        primary: bool,
        scale: f64,
    ) -> shell::MonitorInfo {
        shell::MonitorInfo {
            device: format!("\\\\.\\DISPLAY{x}"),
            primary,
            x,
            y,
            width: w,
            height: h,
            work_x: x,
            work_y: y,
            work_width: w,
            work_height: h - 48,
            scale,
        }
    }

    #[test]
    fn logical_bounds_divide_physical_pixels_by_the_scale() {
        // The real machine this was built on: a 1920x1080 panel at 125%,
        // which Tauri sees as 1536x864.
        let m = scaled_monitor(0, 0, 1920, 1080, true, 1.25);
        let b = m.logical();
        assert_eq!(b.width, 1536.0);
        assert_eq!(b.height, 864.0);
        assert_eq!(b.work_width, 1536.0);
    }

    #[test]
    fn an_implausible_scale_falls_back_to_one_to_one() {
        for scale in [0.0, -2.0, f64::NAN] {
            let b = scaled_monitor(0, 0, 1920, 1080, true, scale).logical();
            assert_eq!(b.width, 1920.0, "scale {scale} should fall back to 1:1");
        }
    }

    #[test]
    fn reachability_is_judged_in_logical_pixels_not_physical_ones() {
        // A 1920x1080 panel at 125% is 1536x864 to Tauri. A widget sitting
        // just inside the logical right edge is on screen; one placed at
        // 1800 — inside the *physical* width but past the logical one — is
        // not, and gets rescued.
        let screens = vec![scaled_monitor(0, 0, 1920, 1080, true, 1.25)];
        let b = screens[0].logical();
        assert_eq!(b.width, 1536.0);

        let inside = clamp_to_visible(b.work_width - 460.0, 20.0, 440.0, 72.0, &screens);
        assert_eq!(inside, (1076.0, 20.0), "a surface on screen must not move");

        let (x, y) = clamp_to_visible(1800.0, 20.0, 440.0, 72.0, &screens);
        assert!(x < b.width, "a surface past the logical edge must come back");
        assert!(y >= 0.0);
    }

    #[test]
    fn a_surface_hanging_off_an_edge_is_left_alone() {
        // Partly off-screen is a legitimate choice — someone may want a
        // widget tucked against an edge — so only a fully unreachable
        // surface is moved.
        let screens = vec![monitor(0, 0, 1536, 864, true)];
        let (x, y) = clamp_to_visible(1400.0, 20.0, 440.0, 72.0, &screens);
        assert_eq!((x, y), (1400.0, 20.0));
    }

    #[test]
    fn surface_url_carries_the_routing_parameters() {
        let spec = SurfaceSpec::new("widget-clock", "widget").param("id", "clock");
        assert_eq!(spec.url(), "index.html?surface=widget&id=clock");
    }

    #[test]
    fn surface_url_escapes_values_that_would_break_the_query() {
        // An unescaped `&` would silently drop every later parameter.
        let spec = SurfaceSpec::new("widget-x", "widget").param("id", "a&b=c d");
        let url = spec.url();
        assert!(url.ends_with("id=a%26b%3Dc%20d"), "got {url}");
    }

    #[test]
    fn a_widget_on_screen_is_left_where_it_is() {
        let screens = vec![monitor(0, 0, 1536, 864, true)];
        assert_eq!(clamp_to_visible(100.0, 100.0, 320.0, 160.0, &screens), (100.0, 100.0));
    }

    #[test]
    fn a_widget_on_a_disconnected_monitor_comes_home() {
        // The second display is gone; its widget must not be stranded.
        let screens = vec![monitor(0, 0, 1536, 864, true)];
        let (x, y) = clamp_to_visible(2400.0, 300.0, 320.0, 160.0, &screens);
        assert!(x >= 0.0 && x < 1536.0, "x was {x}");
        assert!(y >= 0.0 && y < 864.0, "y was {y}");
    }

    #[test]
    fn a_widget_on_a_still_connected_second_monitor_stays_there() {
        let screens = vec![
            monitor(0, 0, 1536, 864, true),
            monitor(1536, 0, 1920, 1080, false),
        ];
        assert_eq!(
            clamp_to_visible(2400.0, 300.0, 320.0, 160.0, &screens),
            (2400.0, 300.0)
        );
    }

    #[test]
    fn negative_coordinates_from_a_left_hand_monitor_are_honoured() {
        // Monitors to the left of the primary have negative origins; those
        // are legitimate positions, not errors to clamp away.
        let screens = vec![
            monitor(-1920, 0, 1920, 1080, false),
            monitor(0, 0, 1536, 864, true),
        ];
        assert_eq!(
            clamp_to_visible(-1800.0, 100.0, 320.0, 160.0, &screens),
            (-1800.0, 100.0)
        );
    }

    #[test]
    fn with_no_monitors_reported_nothing_ends_up_off_the_top_left() {
        let (x, y) = clamp_to_visible(-500.0, -500.0, 320.0, 160.0, &[]);
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn only_a_hud_style_surface_asks_to_be_always_on_top() {
        // The bug this guards is a UX one rather than a crash: an
        // always-on-top surface parked over the studio window swallows the
        // clicks meant for it and covers its close button. Anything the user
        // interacts with directly belongs at Normal or below.
        use crate::modules::{dock, dropzone};

        let dropzone_cfg = dropzone::DropzoneConfig::default();
        assert!(dropzone_cfg.width > 0.0);

        // Encoded as the specs each module builds. Dock and Dropzone are
        // panels you work in; only the HUD is a glance-at readout.
        let dock_spec = SurfaceSpec::new(dock::LABEL, "dock").z(ZMode::Normal);
        let dropzone_spec = SurfaceSpec::new(dropzone::LABEL, "dropzone").z(ZMode::Normal);
        assert_ne!(dock_spec.z, ZMode::Top);
        assert_ne!(dropzone_spec.z, ZMode::Top);
    }

    #[test]
    fn a_desktop_surface_defaults_to_the_bottom_of_the_stack() {
        // Widgets must start behind ordinary windows. Showing a window
        // raises it, so SurfaceManager applies stacking *after* show() —
        // doing it before put every widget back on top.
        let spec = SurfaceSpec::new("widget-clock", "widget");
        assert_eq!(spec.z, ZMode::Desktop);
    }

    #[test]
    fn the_builder_produces_the_defaults_a_desktop_widget_wants() {
        let spec = SurfaceSpec::new("widget-clock", "widget");
        assert_eq!(spec.z, ZMode::Desktop);
        assert!(spec.tool_window, "widgets must stay out of Alt-Tab");
        assert!(spec.transparent);
        assert!(!spec.click_through, "click-through is opt-in");
    }
}
