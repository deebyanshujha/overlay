//! Profiles: named sets of enabled modules and their configuration.
//!
//! `switch` holds the transactional engine that moves the desktop from one
//! profile to another; this module owns storage, the built-in profiles, and
//! import/export.

pub mod switch;

use crate::error::{OverlayError, Result};
use crate::modules::Registry;
use crate::storage::now;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// The profile that represents this machine as OVERLAY first found it.
/// Selecting it means "turn everything off and put it all back".
pub const ORIGINAL_WINDOWS: &str = "original_windows";

/// How one module is set up inside a profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleSetting {
    pub enabled: bool,
    pub config: Value,
}

impl Default for ModuleSetting {
    fn default() -> Self {
        Self {
            enabled: false,
            config: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Ships with OVERLAY. Cannot be deleted; `original_windows` additionally
    /// cannot be edited.
    pub builtin: bool,
    /// OVERLAY's own interface theme, independent of the desktop.
    pub theme: String,
    pub accent: String,
    pub modules: BTreeMap<String, ModuleSetting>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Profile {
    /// Module ids this profile wants switched on.
    pub fn enabled_ids(&self) -> HashSet<String> {
        self.modules
            .iter()
            .filter(|(_, s)| s.enabled)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn configs(&self) -> BTreeMap<String, Value> {
        self.modules
            .iter()
            .map(|(id, s)| (id.clone(), s.config.clone()))
            .collect()
    }

    /// True when this profile is the untouched desktop: nothing enabled.
    pub fn is_bare(&self) -> bool {
        self.enabled_ids().is_empty()
    }
}

/// A profile as it travels between machines. Carries a format version and
/// deliberately *no* executable content of any kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileExport {
    pub format: String,
    pub version: u32,
    pub exported_at: i64,
    pub name: String,
    pub description: String,
    pub theme: String,
    pub accent: String,
    pub modules: BTreeMap<String, ModuleSetting>,
}

pub const EXPORT_FORMAT: &str = "overlay.profile";
pub const EXPORT_VERSION: u32 = 1;

/// Profile storage.
#[derive(Clone)]
pub struct ProfileStore {
    conn: Arc<Mutex<Connection>>,
}

impl ProfileStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    // -----------------------------------------------------------------
    // Reading
    // -----------------------------------------------------------------

    pub fn list(&self) -> Result<Vec<Profile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, builtin, theme, accent, created_at, updated_at
             FROM profiles ORDER BY builtin DESC, name",
        )?;
        let rows: Vec<Profile> = stmt
            .query_map([], |r| {
                Ok(Profile {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    builtin: r.get::<_, i64>(3)? != 0,
                    theme: r.get(4)?,
                    accent: r.get(5)?,
                    modules: BTreeMap::new(),
                    created_at: r.get(6)?,
                    updated_at: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        let mut out = Vec::with_capacity(rows.len());
        for mut p in rows {
            p.modules = Self::load_modules(&conn, &p.id)?;
            out.push(p);
        }
        // Original Windows always first: it is the safety net, and it should
        // be the easiest thing on the page to find.
        out.sort_by_key(|p| (p.id != ORIGINAL_WINDOWS, !p.builtin, p.name.clone()));
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Profile> {
        let conn = self.conn.lock();
        let mut p: Profile = conn
            .query_row(
                "SELECT id, name, description, builtin, theme, accent, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                [id],
                |r| {
                    Ok(Profile {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        builtin: r.get::<_, i64>(3)? != 0,
                        theme: r.get(4)?,
                        accent: r.get(5)?,
                        modules: BTreeMap::new(),
                        created_at: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| OverlayError::UnknownProfile(id.to_string()))?;
        p.modules = Self::load_modules(&conn, id)?;
        Ok(p)
    }

    fn load_modules(conn: &Connection, profile_id: &str) -> Result<BTreeMap<String, ModuleSetting>> {
        let mut stmt = conn.prepare(
            "SELECT module_id, enabled, config FROM profile_modules WHERE profile_id = ?1",
        )?;
        let rows = stmt.query_map([profile_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? != 0,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (id, enabled, config) = row?;
            out.insert(
                id,
                ModuleSetting {
                    enabled,
                    // A config row that will not parse is treated as "no
                    // config", which falls back to the module's defaults,
                    // rather than making the whole profile unreadable.
                    config: serde_json::from_str(&config).unwrap_or_default(),
                },
            );
        }
        Ok(out)
    }

    pub fn active_id(&self) -> Result<Option<String>> {
        let conn = self.conn.lock();
        Ok(conn
            .query_row("SELECT profile_id FROM active_profile WHERE id = 1", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten())
    }

    pub fn set_active(&self, id: Option<&str>) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE active_profile SET profile_id = ?1 WHERE id = 1",
            params![id],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Writing
    // -----------------------------------------------------------------

    pub fn save(&self, profile: &Profile) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO profiles (id, name, description, builtin, theme, accent,
                                   created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                theme = excluded.theme,
                accent = excluded.accent,
                updated_at = excluded.updated_at",
            params![
                profile.id,
                profile.name,
                profile.description,
                profile.builtin as i64,
                profile.theme,
                profile.accent,
                profile.created_at,
                profile.updated_at,
            ],
        )?;
        conn.execute(
            "DELETE FROM profile_modules WHERE profile_id = ?1",
            [&profile.id],
        )?;
        for (module_id, setting) in &profile.modules {
            conn.execute(
                "INSERT INTO profile_modules (profile_id, module_id, enabled, config)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    profile.id,
                    module_id,
                    setting.enabled as i64,
                    serde_json::to_string(&setting.config)?,
                ],
            )?;
        }
        Ok(())
    }

    /// Creates a profile, generating a unique id from the name.
    pub fn create(&self, name: &str, description: &str, from: Option<&Profile>) -> Result<Profile> {
        let name = name.trim();
        if name.is_empty() {
            return Err(OverlayError::Config("a profile needs a name".into()));
        }
        if name.chars().count() > 60 {
            return Err(OverlayError::Config(
                "that profile name is too long (60 characters maximum)".into(),
            ));
        }
        let id = self.unique_id(&slug(name))?;
        let t = now();
        let profile = Profile {
            id,
            name: name.to_string(),
            description: description.trim().to_string(),
            builtin: false,
            theme: from.map(|p| p.theme.clone()).unwrap_or_else(|| "minimal".into()),
            accent: from.map(|p| p.accent.clone()).unwrap_or_else(|| "ice".into()),
            modules: from.map(|p| p.modules.clone()).unwrap_or_default(),
            created_at: t,
            updated_at: t,
        };
        self.save(&profile)?;
        Ok(profile)
    }

    pub fn rename(&self, id: &str, name: &str, description: &str) -> Result<Profile> {
        let mut p = self.get(id)?;
        if p.id == ORIGINAL_WINDOWS {
            return Err(OverlayError::ProfileImmutable(p.name));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(OverlayError::Config("a profile needs a name".into()));
        }
        p.name = name.to_string();
        p.description = description.trim().to_string();
        p.updated_at = now();
        self.save(&p)?;
        Ok(p)
    }

    pub fn duplicate(&self, id: &str, new_name: &str) -> Result<Profile> {
        let source = self.get(id)?;
        let name = if new_name.trim().is_empty() {
            format!("{} copy", source.name)
        } else {
            new_name.trim().to_string()
        };
        self.create(&name, &source.description, Some(&source))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let p = self.get(id)?;
        if p.builtin {
            return Err(OverlayError::ProfileImmutable(p.name));
        }
        if self.active_id()?.as_deref() == Some(id) {
            return Err(OverlayError::Config(
                "that profile is active. Switch to another one first.".into(),
            ));
        }
        let conn = self.conn.lock();
        conn.execute("DELETE FROM profiles WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Sets a module's enabled flag and configuration inside a profile,
    /// validating the configuration against the module that owns it.
    pub fn set_module(
        &self,
        profile_id: &str,
        module_id: &str,
        enabled: bool,
        config: Option<Value>,
        registry: &Registry,
    ) -> Result<Profile> {
        let mut p = self.get(profile_id)?;
        if p.id == ORIGINAL_WINDOWS {
            return Err(OverlayError::ProfileImmutable(p.name));
        }
        let module = registry.get(module_id)?;
        let config = match config {
            Some(c) => module.validate_config(&c)?,
            None => p
                .modules
                .get(module_id)
                .map(|s| s.config.clone())
                .filter(|c| !c.is_null() && c.as_object().map(|o| !o.is_empty()).unwrap_or(false))
                .map(|c| module.validate_config(&c))
                .transpose()?
                .unwrap_or_else(|| module.default_config()),
        };
        p.modules
            .insert(module_id.to_string(), ModuleSetting { enabled, config });
        p.updated_at = now();
        self.save(&p)?;
        Ok(p)
    }

    fn unique_id(&self, base: &str) -> Result<String> {
        let conn = self.conn.lock();
        let exists = |id: &str| -> Result<bool> {
            Ok(conn
                .query_row("SELECT 1 FROM profiles WHERE id = ?1", [id], |_| Ok(()))
                .optional()?
                .is_some())
        };
        if !exists(base)? {
            return Ok(base.to_string());
        }
        for n in 2..1_000 {
            let candidate = format!("{base}-{n}");
            if !exists(&candidate)? {
                return Ok(candidate);
            }
        }
        Err(OverlayError::Config(
            "too many profiles with similar names".into(),
        ))
    }

    // -----------------------------------------------------------------
    // Import / export
    // -----------------------------------------------------------------

    pub fn export(&self, id: &str) -> Result<ProfileExport> {
        let p = self.get(id)?;
        Ok(ProfileExport {
            format: EXPORT_FORMAT.into(),
            version: EXPORT_VERSION,
            exported_at: now(),
            name: p.name,
            description: p.description,
            theme: p.theme,
            accent: p.accent,
            modules: p.modules,
        })
    }

    /// Imports a profile from JSON.
    ///
    /// Everything is treated as hostile: the format tag and version are
    /// checked, unknown modules are dropped rather than stored, and every
    /// configuration is re-validated by the module that owns it before it is
    /// written. A profile is pure data — there is no field anywhere in the
    /// schema that names a command, a script or a path to execute.
    pub fn import(&self, json: &str, registry: &Registry) -> Result<(Profile, Vec<String>)> {
        let export: ProfileExport = serde_json::from_str(json)
            .map_err(|e| OverlayError::Config(format!("that is not an OVERLAY profile: {e}")))?;

        if export.format != EXPORT_FORMAT {
            return Err(OverlayError::Config(format!(
                "unexpected file format `{}`",
                export.format
            )));
        }
        if export.version > EXPORT_VERSION {
            return Err(OverlayError::Config(format!(
                "this profile was made by a newer OVERLAY (format {}, this build reads {})",
                export.version, EXPORT_VERSION
            )));
        }

        let mut warnings = Vec::new();
        let mut modules = BTreeMap::new();
        for (id, setting) in export.modules {
            let Ok(module) = registry.get(&id) else {
                warnings.push(format!("Skipped `{id}`: this build has no such module."));
                continue;
            };
            match module.validate_config(&setting.config) {
                Ok(config) => {
                    modules.insert(
                        id,
                        ModuleSetting {
                            enabled: setting.enabled,
                            config,
                        },
                    );
                }
                Err(e) => {
                    warnings.push(format!(
                        "`{}` was imported with default settings: {e}",
                        module.descriptor().name
                    ));
                    modules.insert(
                        id,
                        ModuleSetting {
                            enabled: setting.enabled,
                            config: module.default_config(),
                        },
                    );
                }
            }
        }

        let name = if export.name.trim().is_empty() {
            "Imported profile".to_string()
        } else {
            export.name.trim().chars().take(60).collect()
        };
        let mut profile = self.create(&name, &export.description, None)?;
        profile.theme = sanitize_theme(&export.theme);
        profile.accent = sanitize_accent(&export.accent);
        profile.modules = modules;
        self.save(&profile)?;
        Ok((profile, warnings))
    }

    // -----------------------------------------------------------------
    // Built-ins
    // -----------------------------------------------------------------

    /// Creates the shipped profiles if they are not already there.
    ///
    /// Only ever inserts: a user who has edited Minimal keeps their edits
    /// across upgrades.
    pub fn ensure_builtins(&self, registry: &Registry) -> Result<()> {
        for spec in builtin_specs() {
            if self.get(spec.id).is_ok() {
                continue;
            }
            let t = now();
            let mut modules = BTreeMap::new();
            for (module_id, enabled, overrides) in spec.modules {
                let Ok(module) = registry.get(module_id) else {
                    continue;
                };
                let mut config = module.default_config();
                if let (Some(obj), Some(extra)) = (config.as_object_mut(), overrides.as_object()) {
                    for (k, v) in extra {
                        obj.insert(k.clone(), v.clone());
                    }
                }
                let config = module.validate_config(&config).unwrap_or(config);
                modules.insert(module_id.to_string(), ModuleSetting { enabled, config });
            }
            self.save(&Profile {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                builtin: true,
                theme: spec.theme.to_string(),
                accent: spec.accent.to_string(),
                modules,
                created_at: t,
                updated_at: t,
            })?;
        }
        Ok(())
    }
}

struct BuiltinSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    theme: &'static str,
    accent: &'static str,
    /// (module id, enabled, config overrides merged over the default)
    modules: Vec<(&'static str, bool, Value)>,
}

fn builtin_specs() -> Vec<BuiltinSpec> {
    use serde_json::json;

    vec![
        BuiltinSpec {
            id: ORIGINAL_WINDOWS,
            name: "Original Windows",
            description:
                "Your desktop exactly as OVERLAY first found it. Nothing enabled, every tracked \
                 setting restored.",
            theme: "minimal",
            accent: "ice",
            // Deliberately empty: this profile is defined by what it does not do.
            modules: vec![],
        },
        BuiltinSpec {
            id: "minimal",
            name: "Minimal",
            description: "A clock, a clear taskbar and nothing else.",
            theme: "minimal",
            accent: "ice",
            modules: vec![
                (
                    "widgets",
                    true,
                    json!({ "widgets": [{
                        "id": "clock", "enabled": true,
                        "x": 90.0, "y": 90.0, "width": 420.0, "height": 170.0,
                        "monitor": "", "opacity": 1.0, "theme": "bare", "font": "display",
                        "font_scale": 1.3, "interval_ms": 1000, "locked": false,
                        "click_through": true, "z": "desktop", "settings": {}
                    }], "allow_drag": true }),
                ),
                (
                    "taskbar",
                    true,
                    json!({ "finish": "blurred", "opacity": 0.25, "tint": 0x0B0C10 }),
                ),
                ("control_center", true, json!({})),
            ],
        },
        BuiltinSpec {
            id: "elegant",
            name: "Elegant",
            description: "Glass widgets, a soft taskbar tint and a floating dock.",
            theme: "futuristic",
            accent: "violet",
            modules: vec![
                (
                    "widgets",
                    true,
                    json!({ "widgets": [
                        { "id": "clock", "enabled": true, "x": 90.0, "y": 90.0,
                          "width": 400.0, "height": 160.0, "monitor": "", "opacity": 0.95,
                          "theme": "glass", "font": "display", "font_scale": 1.15,
                          "interval_ms": 1000, "locked": false, "click_through": false,
                          "z": "desktop", "settings": {} },
                        { "id": "date", "enabled": true, "x": 90.0, "y": 270.0,
                          "width": 320.0, "height": 120.0, "monitor": "", "opacity": 0.95,
                          "theme": "glass", "font": "sans", "font_scale": 1.0,
                          "interval_ms": 60000, "locked": false, "click_through": false,
                          "z": "desktop", "settings": {} },
                        { "id": "calendar", "enabled": true, "x": 90.0, "y": 410.0,
                          "width": 330.0, "height": 320.0, "monitor": "", "opacity": 0.95,
                          "theme": "glass", "font": "sans", "font_scale": 1.0,
                          "interval_ms": 300000, "locked": false, "click_through": false,
                          "z": "desktop", "settings": {} }
                    ], "allow_drag": true }),
                ),
                (
                    "taskbar",
                    true,
                    json!({ "finish": "acrylic", "opacity": 0.4, "tint": 0x161220,
                            "alignment": "center" }),
                ),
                ("dock", true, json!({ "edge": "bottom", "magnification": 1.7 })),
                ("control_center", true, json!({})),
            ],
        },
        BuiltinSpec {
            id: "developer",
            name: "Developer",
            description:
                "System HUD, monospaced widgets, a left-aligned taskbar and the Dropzone.",
            theme: "minimal",
            accent: "emerald",
            modules: vec![
                (
                    "widgets",
                    true,
                    json!({ "widgets": [
                        { "id": "clock", "enabled": true, "x": 80.0, "y": 80.0,
                          "width": 320.0, "height": 130.0, "monitor": "", "opacity": 0.92,
                          "theme": "outline", "font": "mono", "font_scale": 1.0,
                          "interval_ms": 1000, "locked": false, "click_through": true,
                          "z": "desktop", "settings": {} },
                        { "id": "uptime", "enabled": true, "x": 80.0, "y": 230.0,
                          "width": 300.0, "height": 110.0, "monitor": "", "opacity": 0.92,
                          "theme": "outline", "font": "mono", "font_scale": 1.0,
                          "interval_ms": 30000, "locked": false, "click_through": true,
                          "z": "desktop", "settings": {} },
                        { "id": "storage", "enabled": true, "x": 80.0, "y": 360.0,
                          "width": 320.0, "height": 190.0, "monitor": "", "opacity": 0.92,
                          "theme": "outline", "font": "mono", "font_scale": 1.0,
                          "interval_ms": 60000, "locked": false, "click_through": true,
                          "z": "desktop", "settings": {} }
                    ], "allow_drag": true }),
                ),
                (
                    "hud",
                    true,
                    json!({ "metrics": ["cpu", "ram", "network", "storage"],
                            "anchor": "top_right", "compact": true }),
                ),
                (
                    "taskbar",
                    true,
                    json!({ "finish": "tinted", "opacity": 0.6, "tint": 0x0A0C0E,
                            "alignment": "left", "icon_size": "small" }),
                ),
                ("dropzone", true, json!({})),
                ("control_center", true, json!({})),
                ("quickdock", true, json!({})),
            ],
        },
        BuiltinSpec {
            id: "gaming",
            name: "Gaming",
            description:
                "A clean desktop with performance metrics and nothing in the way.",
            theme: "futuristic",
            accent: "amber",
            modules: vec![
                ("desktop_icons", true, json!({ "show_icons": false })),
                (
                    "widgets",
                    true,
                    json!({ "widgets": [
                        { "id": "clock", "enabled": true, "x": 90.0, "y": 80.0,
                          "width": 380.0, "height": 150.0, "monitor": "", "opacity": 0.9,
                          "theme": "bare", "font": "display", "font_scale": 1.2,
                          "interval_ms": 1000, "locked": true, "click_through": true,
                          "z": "desktop", "settings": {} }
                    ], "allow_drag": true }),
                ),
                (
                    "hud",
                    true,
                    json!({ "metrics": ["cpu", "gpu", "gpu_temp", "ram"],
                            "anchor": "top_left", "compact": true, "interval_ms": 1000 }),
                ),
                (
                    "taskbar",
                    true,
                    json!({ "finish": "clear", "opacity": 0.0, "auto_hide": true }),
                ),
                ("control_center", true, json!({})),
            ],
        },
        BuiltinSpec {
            id: "custom",
            name: "Custom",
            description: "An empty profile to build up however you like.",
            theme: "minimal",
            accent: "ice",
            modules: vec![],
        },
    ]
}

/// URL- and filename-safe id derived from a display name.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true; // stops a leading dash
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        format!("profile-{}", now())
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn sanitize_theme(raw: &str) -> String {
    match raw {
        "futuristic" => "futuristic".into(),
        _ => "minimal".into(),
    }
}

fn sanitize_accent(raw: &str) -> String {
    const ACCENTS: [&str; 5] = ["ice", "violet", "emerald", "amber", "rose"];
    if ACCENTS.contains(&raw) {
        raw.to_string()
    } else {
        "ice".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;

    fn store() -> (ProfileStore, Registry) {
        let conn = Arc::new(Mutex::new(storage::open_in_memory().unwrap()));
        let registry = Registry::new();
        let store = ProfileStore::new(conn);
        store.ensure_builtins(&registry).unwrap();
        (store, registry)
    }

    #[test]
    fn the_builtins_are_created_once_and_include_original_windows() {
        let (s, r) = store();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 6);
        assert_eq!(list[0].id, ORIGINAL_WINDOWS, "safety net must sort first");

        // Running it again must not duplicate or reset anything.
        s.ensure_builtins(&r).unwrap();
        assert_eq!(s.list().unwrap().len(), 6);
    }

    #[test]
    fn original_windows_enables_nothing() {
        // The definition of the profile, asserted rather than assumed.
        let (s, _) = store();
        let p = s.get(ORIGINAL_WINDOWS).unwrap();
        assert!(p.is_bare());
        assert!(p.enabled_ids().is_empty());
    }

    #[test]
    fn every_builtin_profile_passes_module_validation_and_planning() {
        // A shipped profile that cannot be applied would be a broken product.
        let (s, r) = store();
        for p in s.list().unwrap() {
            let plan = r.plan(&p.enabled_ids());
            assert!(plan.is_ok(), "profile `{}` cannot be planned: {plan:?}", p.id);
            for (id, setting) in &p.modules {
                let m = r.get(id).unwrap();
                assert!(
                    m.validate_config(&setting.config).is_ok(),
                    "profile `{}` has an invalid config for `{id}`",
                    p.id
                );
            }
        }
    }

    #[test]
    fn original_windows_cannot_be_edited_or_deleted() {
        let (s, r) = store();
        assert!(s.rename(ORIGINAL_WINDOWS, "Nope", "").is_err());
        assert!(s.delete(ORIGINAL_WINDOWS).is_err());
        assert!(s
            .set_module(ORIGINAL_WINDOWS, "taskbar", true, None, &r)
            .is_err());
    }

    #[test]
    fn builtins_cannot_be_deleted_but_can_be_edited() {
        let (s, r) = store();
        assert!(s.delete("minimal").is_err());
        let p = s.set_module("minimal", "dropzone", true, None, &r).unwrap();
        assert!(p.modules["dropzone"].enabled);
    }

    #[test]
    fn creating_and_duplicating_produces_distinct_ids() {
        let (s, _) = store();
        let a = s.create("My Setup", "", None).unwrap();
        let b = s.create("My Setup", "", None).unwrap();
        assert_eq!(a.id, "my-setup");
        assert_eq!(b.id, "my-setup-2");

        let c = s.duplicate("minimal", "").unwrap();
        assert_eq!(c.name, "Minimal copy");
        assert_eq!(c.modules, s.get("minimal").unwrap().modules);
        assert!(!c.builtin, "a copy of a builtin is editable");
    }

    #[test]
    fn a_nameless_profile_is_refused() {
        let (s, _) = store();
        assert!(s.create("   ", "", None).is_err());
    }

    #[test]
    fn slugs_are_safe_for_ids_and_filenames() {
        assert_eq!(slug("My Setup"), "my-setup");
        assert_eq!(slug("  Spaces  "), "spaces");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("C:\\Windows"), "c-windows");
        assert!(!slug("***").is_empty());
        assert!(slug(&"x".repeat(200)).len() <= 48);
    }

    #[test]
    fn the_active_profile_cannot_be_deleted() {
        let (s, _) = store();
        let p = s.create("Temp", "", None).unwrap();
        s.set_active(Some(&p.id)).unwrap();
        assert!(s.delete(&p.id).is_err());
        s.set_active(Some("minimal")).unwrap();
        assert!(s.delete(&p.id).is_ok());
    }

    #[test]
    fn export_then_import_preserves_the_configuration() {
        let (s, r) = store();
        let exported = s.export("developer").unwrap();
        let json = serde_json::to_string(&exported).unwrap();

        let (imported, warnings) = s.import(&json, &r).unwrap();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(imported.enabled_ids(), s.get("developer").unwrap().enabled_ids());
        assert!(!imported.builtin, "an import is always editable");
    }

    #[test]
    fn importing_a_foreign_file_is_refused() {
        let (s, r) = store();
        assert!(s.import("not json at all", &r).is_err());
        assert!(s
            .import(r#"{"format":"something.else","version":1}"#, &r)
            .is_err());
    }

    #[test]
    fn importing_a_newer_format_is_refused_rather_than_guessed_at() {
        let (s, r) = store();
        let json = format!(
            r#"{{"format":"{EXPORT_FORMAT}","version":{},"exported_at":0,"name":"X",
                 "description":"","theme":"minimal","accent":"ice","modules":{{}}}}"#,
            EXPORT_VERSION + 1
        );
        assert!(s.import(&json, &r).is_err());
    }

    #[test]
    fn an_unknown_module_in_an_import_is_dropped_with_a_warning() {
        let (s, r) = store();
        let json = format!(
            r#"{{"format":"{EXPORT_FORMAT}","version":1,"exported_at":0,"name":"X",
                 "description":"","theme":"minimal","accent":"ice",
                 "modules":{{"from_the_future":{{"enabled":true,"config":{{}}}}}}}}"#
        );
        let (p, warnings) = s.import(&json, &r).unwrap();
        assert!(p.modules.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("from_the_future"));
    }

    #[test]
    fn a_hostile_config_in_an_import_falls_back_to_defaults_with_a_warning() {
        // The key safety property of import: nothing reaches the Windows
        // layer without passing its module's validator.
        let (s, r) = store();
        let json = format!(
            r#"{{"format":"{EXPORT_FORMAT}","version":1,"exported_at":0,"name":"X",
                 "description":"","theme":"minimal","accent":"ice",
                 "modules":{{"widgets":{{"enabled":true,
                   "config":{{"widgets":[{{"id":"../../evil","enabled":true,
                     "x":0,"y":0,"width":0,"height":0,"opacity":1,"theme":"glass",
                     "font":"sans","font_scale":1,"interval_ms":1,"locked":false,
                     "click_through":false,"z":"desktop"}}],"allow_drag":true}}}}}}}}"#
        );
        let (p, warnings) = s.import(&json, &r).unwrap();
        assert_eq!(warnings.len(), 1);
        let cfg = &p.modules["widgets"].config;
        let ids: Vec<&str> = cfg["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["id"].as_str().unwrap())
            .collect();
        assert!(!ids.contains(&"../../evil"), "hostile id survived import");
    }

    #[test]
    fn an_unrecognised_theme_or_accent_falls_back_rather_than_being_stored() {
        let (s, r) = store();
        let json = format!(
            r##"{{"format":"{EXPORT_FORMAT}","version":1,"exported_at":0,"name":"X",
                 "description":"","theme":"<script>","accent":"#bada55","modules":{{}}}}"##
        );
        let (p, _) = s.import(&json, &r).unwrap();
        assert_eq!(p.theme, "minimal");
        assert_eq!(p.accent, "ice");
    }

    #[test]
    fn setting_a_module_validates_its_configuration() {
        let (s, r) = store();
        let bad = serde_json::json!({ "finish": "not-a-finish" });
        assert!(s.set_module("minimal", "taskbar", true, Some(bad), &r).is_err());
    }

    #[test]
    fn module_settings_survive_a_save_and_reload() {
        let (s, r) = store();
        let cfg = serde_json::json!({ "finish": "acrylic", "opacity": 0.5, "tint": 0x112233 });
        s.set_module("custom", "taskbar", true, Some(cfg), &r).unwrap();

        let reloaded = s.get("custom").unwrap();
        let stored = &reloaded.modules["taskbar"];
        assert!(stored.enabled);
        assert_eq!(stored.config["finish"], "acrylic");
        assert_eq!(stored.config["tint"], 0x112233);
    }
}
