//! System metrics.
//!
//! # The rule
//!
//! A metric is either a real reading or an explicit absence. Nothing here
//! estimates, interpolates or substitutes a plausible figure for a sensor it
//! cannot reach — a fake GPU temperature is worse than no GPU temperature,
//! because the user cannot tell it is fake.
//!
//! That is why every value is `Option<T>` carried alongside a `detail` string
//! explaining an absence, and why [`capabilities`] is probed once at startup
//! so the UI can hide controls for sensors this machine does not have.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::Instant;
use sysinfo::{Disks, Networks, System};

/// What this machine can actually report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub cpu: bool,
    pub memory: bool,
    pub gpu_usage: bool,
    pub gpu_temperature: bool,
    pub battery: bool,
    pub network: bool,
    pub storage: bool,
    pub uptime: bool,
    pub media: bool,
    /// Why the GPU metrics are or are not available.
    pub gpu_detail: String,
    pub media_detail: String,
}

fn probe_capabilities() -> Capabilities {
    let (gpu_usage, gpu_temperature, gpu_detail) = gpu::probe();
    Capabilities {
        cpu: true,
        memory: true,
        gpu_usage,
        gpu_temperature,
        battery: battery::present(),
        network: true,
        storage: true,
        uptime: true,
        // Reading the Now Playing session needs WinRT's
        // GlobalSystemMediaTransportControls API, which is not wired up in
        // this build. Saying so beats shipping a widget that always reads
        // "Nothing playing" whether or not something is.
        media: false,
        gpu_detail,
        media_detail: "Media session reading is not implemented in this build.".into(),
    }
}

pub fn capabilities() -> &'static Capabilities {
    static CELL: OnceLock<Capabilities> = OnceLock::new();
    CELL.get_or_init(probe_capabilities)
}

/// One sample of everything the HUD and the monitor widgets display.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub cpu_percent: Option<f32>,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub gpu_percent: Option<f32>,
    pub gpu_temperature_c: Option<u32>,
    pub gpu_name: Option<String>,
    pub battery_percent: Option<u8>,
    pub battery_charging: Option<bool>,
    pub battery_minutes_left: Option<u32>,
    pub network_down_bps: Option<u64>,
    pub network_up_bps: Option<u64>,
    pub network_interface: Option<String>,
    pub disks: Vec<DiskUsage>,
    pub uptime_seconds: Option<u64>,
    /// Populated only where a reading genuinely could not be taken.
    pub unavailable: Vec<Unavailable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub mount: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unavailable {
    pub metric: String,
    pub reason: String,
}

/// Holds the `sysinfo` handles between samples.
///
/// Both CPU load and network throughput are *deltas*: they are meaningless
/// from a single reading and need the previous one to subtract from. Keeping
/// one long-lived sampler is therefore not an optimisation but a correctness
/// requirement — constructing a fresh `System` per tick reports 0% CPU
/// forever.
pub struct Sampler {
    system: System,
    networks: Networks,
    disks: Disks,
    last_sample: Option<Instant>,
    last_disk_refresh: Option<Instant>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        system.refresh_memory();
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            last_sample: None,
            last_disk_refresh: None,
        }
    }

    pub fn sample(&mut self) -> Metrics {
        let mut m = Metrics::default();
        let now = Instant::now();
        let elapsed = self
            .last_sample
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);

        // --- CPU ---------------------------------------------------------
        self.system.refresh_cpu_usage();
        let cpus = self.system.cpus();
        m.cpu_cores = cpus.len();
        if let Some(first) = cpus.first() {
            m.cpu_name = first.brand().trim().to_string();
        }
        // The first sample after construction has no baseline to diff
        // against, so it is reported as unavailable rather than as 0%.
        if self.last_sample.is_some() && !cpus.is_empty() {
            let total: f32 = cpus.iter().map(|c| c.cpu_usage()).sum();
            m.cpu_percent = Some((total / cpus.len() as f32).clamp(0.0, 100.0));
        } else {
            m.unavailable.push(Unavailable {
                metric: "cpu".into(),
                reason: "waiting for a second reading".into(),
            });
        }

        // --- Memory ------------------------------------------------------
        self.system.refresh_memory();
        m.memory_total_bytes = Some(self.system.total_memory());
        m.memory_used_bytes = Some(self.system.used_memory());

        // --- Network -----------------------------------------------------
        self.networks.refresh();
        if elapsed > 0.05 {
            let (mut down, mut up) = (0u64, 0u64);
            let mut busiest: Option<(String, u64)> = None;
            for (name, data) in self.networks.iter() {
                let rx = data.received();
                let tx = data.transmitted();
                down += rx;
                up += tx;
                let traffic = rx + tx;
                if busiest.as_ref().map(|(_, t)| traffic > *t).unwrap_or(true) && traffic > 0 {
                    busiest = Some((name.clone(), traffic));
                }
            }
            m.network_down_bps = Some((down as f64 / elapsed) as u64);
            m.network_up_bps = Some((up as f64 / elapsed) as u64);
            m.network_interface = busiest.map(|(n, _)| n);
        } else {
            m.unavailable.push(Unavailable {
                metric: "network".into(),
                reason: "waiting for a second reading".into(),
            });
        }

        // --- Storage -----------------------------------------------------
        // Disks change rarely and enumerating them is comparatively costly,
        // so this is refreshed at most every 30 seconds regardless of how
        // fast the caller polls.
        let stale = self
            .last_disk_refresh
            .map(|t| now.duration_since(t).as_secs() >= 30)
            .unwrap_or(true);
        if stale {
            self.disks.refresh();
            self.last_disk_refresh = Some(now);
        }
        m.disks = self
            .disks
            .iter()
            .map(|d| DiskUsage {
                mount: d.mount_point().to_string_lossy().to_string(),
                total_bytes: d.total_space(),
                free_bytes: d.available_space(),
            })
            .collect();

        // --- Uptime ------------------------------------------------------
        m.uptime_seconds = Some(System::uptime());

        // --- GPU ---------------------------------------------------------
        match gpu::sample() {
            Some(g) => {
                m.gpu_percent = g.utilization;
                m.gpu_temperature_c = g.temperature_c;
                m.gpu_name = Some(g.name);
                if g.utilization.is_none() {
                    m.unavailable.push(Unavailable {
                        metric: "gpu".into(),
                        reason: "this driver does not report load".into(),
                    });
                }
                if g.temperature_c.is_none() {
                    m.unavailable.push(Unavailable {
                        metric: "gpu_temp".into(),
                        reason: "this driver does not report temperature".into(),
                    });
                }
            }
            None => {
                let reason = capabilities().gpu_detail.clone();
                m.unavailable.push(Unavailable {
                    metric: "gpu".into(),
                    reason: reason.clone(),
                });
                m.unavailable.push(Unavailable {
                    metric: "gpu_temp".into(),
                    reason,
                });
            }
        }

        // --- Battery -----------------------------------------------------
        match battery::sample() {
            Some(b) => {
                m.battery_percent = b.percent;
                m.battery_charging = Some(b.charging);
                m.battery_minutes_left = b.minutes_left;
            }
            None => m.unavailable.push(Unavailable {
                metric: "battery".into(),
                reason: "no battery is present".into(),
            }),
        }

        if !capabilities().media {
            m.unavailable.push(Unavailable {
                metric: "media".into(),
                reason: capabilities().media_detail.clone(),
            });
        }

        self.last_sample = Some(now);
        m
    }
}

/// Process-wide sampler, so every widget and the HUD share one set of
/// readings instead of each running its own `sysinfo` refresh.
pub fn shared() -> &'static Mutex<Sampler> {
    static CELL: OnceLock<Mutex<Sampler>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Sampler::new()))
}

// ---------------------------------------------------------------------------
// GPU
// ---------------------------------------------------------------------------

mod gpu {
    pub struct GpuSample {
        pub name: String,
        pub utilization: Option<f32>,
        pub temperature_c: Option<u32>,
    }

    /// Returns (usage, temperature, explanation).
    pub fn probe() -> (bool, bool, String) {
        #[cfg(all(windows, feature = "nvidia"))]
        {
            match nvml() {
                Ok(_) => {
                    return (
                        true,
                        true,
                        "Readings come from NVML, the NVIDIA management library.".into(),
                    )
                }
                Err(e) => {
                    return (
                        false,
                        false,
                        format!(
                            "No NVIDIA GPU reachable through NVML ({e}). AMD and Intel GPUs do \
                             not expose load or temperature to an unprivileged process on \
                             Windows, so these metrics are shown as unavailable."
                        ),
                    )
                }
            }
        }
        #[allow(unreachable_code)]
        (
            false,
            false,
            "GPU monitoring was not compiled into this build.".into(),
        )
    }

    #[cfg(all(windows, feature = "nvidia"))]
    fn nvml() -> Result<&'static nvml_wrapper::Nvml, String> {
        use std::sync::OnceLock;
        static CELL: OnceLock<Result<nvml_wrapper::Nvml, String>> = OnceLock::new();
        CELL.get_or_init(|| nvml_wrapper::Nvml::init().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| e.clone())
    }

    #[cfg(all(windows, feature = "nvidia"))]
    pub fn sample() -> Option<GpuSample> {
        let nvml = nvml().ok()?;
        let device = nvml.device_by_index(0).ok()?;
        Some(GpuSample {
            name: device.name().unwrap_or_else(|_| "GPU".into()),
            utilization: device.utilization_rates().ok().map(|u| u.gpu as f32),
            temperature_c: device
                .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                .ok(),
        })
    }

    #[cfg(not(all(windows, feature = "nvidia")))]
    pub fn sample() -> Option<GpuSample> {
        None
    }
}

// ---------------------------------------------------------------------------
// Battery
// ---------------------------------------------------------------------------

mod battery {
    pub struct BatterySample {
        pub percent: Option<u8>,
        pub charging: bool,
        pub minutes_left: Option<u32>,
    }

    #[cfg(windows)]
    pub fn sample() -> Option<BatterySample> {
        use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

        let mut status = SYSTEM_POWER_STATUS::default();
        unsafe {
            GetSystemPowerStatus(&mut status).ok()?;
        }
        // 255 is the documented "unknown" marker for both fields.
        if status.BatteryFlag == 128 || status.BatteryFlag == 255 {
            return None; // no system battery
        }
        let percent = (status.BatteryLifePercent != 255).then_some(status.BatteryLifePercent);
        let minutes_left = (status.BatteryLifeTime != u32::MAX)
            .then(|| status.BatteryLifeTime / 60);
        Some(BatterySample {
            percent,
            charging: status.ACLineStatus == 1,
            minutes_left,
        })
    }

    #[cfg(not(windows))]
    pub fn sample() -> Option<BatterySample> {
        None
    }

    pub fn present() -> bool {
        sample().is_some()
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers shared with the frontend via commands.
// ---------------------------------------------------------------------------

/// Human-readable byte rate, e.g. "1.4 MB/s".
pub fn format_rate(bytes_per_second: u64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes_per_second as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value.round() as u64, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_format_at_a_readable_scale() {
        assert_eq!(format_rate(0), "0 B/s");
        assert_eq!(format_rate(512), "512 B/s");
        assert_eq!(format_rate(1024), "1.0 KB/s");
        assert_eq!(format_rate(1_572_864), "1.5 MB/s");
    }

    #[test]
    fn byte_sizes_format_at_a_readable_scale() {
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
    }

    #[test]
    fn uptime_reads_naturally_at_every_scale() {
        assert_eq!(format_uptime(90), "1m");
        assert_eq!(format_uptime(3_700), "1h 1m");
        assert_eq!(format_uptime(200_000), "2d 7h");
    }

    #[test]
    fn the_first_sample_reports_cpu_as_pending_rather_than_zero() {
        // A fresh sampler has no baseline; claiming 0% would be a lie.
        let mut s = Sampler::new();
        let m = s.sample();
        assert!(m.cpu_percent.is_none());
        assert!(m.unavailable.iter().any(|u| u.metric == "cpu"));
    }

    #[test]
    fn a_second_sample_produces_a_real_cpu_reading() {
        let mut s = Sampler::new();
        let _ = s.sample();
        std::thread::sleep(std::time::Duration::from_millis(220));
        let m = s.sample();
        let cpu = m.cpu_percent.expect("second sample should have CPU load");
        assert!((0.0..=100.0).contains(&cpu), "implausible CPU load {cpu}");
    }

    #[test]
    fn memory_readings_are_internally_consistent() {
        let mut s = Sampler::new();
        let m = s.sample();
        let (used, total) = (
            m.memory_used_bytes.unwrap(),
            m.memory_total_bytes.unwrap(),
        );
        assert!(total > 0);
        assert!(used <= total, "used {used} exceeds total {total}");
    }

    #[test]
    fn storage_readings_are_internally_consistent() {
        let mut s = Sampler::new();
        let m = s.sample();
        for d in &m.disks {
            assert!(
                d.free_bytes <= d.total_bytes,
                "{} reports more free than total",
                d.mount
            );
        }
    }

    #[test]
    fn an_unavailable_metric_always_carries_a_reason() {
        // The honesty rule: never a blank absence.
        let mut s = Sampler::new();
        let m = s.sample();
        for u in &m.unavailable {
            assert!(!u.reason.trim().is_empty(), "{} has no reason", u.metric);
        }
    }

    #[test]
    fn capabilities_explain_the_gpu_situation_either_way() {
        let c = capabilities();
        assert!(!c.gpu_detail.trim().is_empty());
        if !c.gpu_usage {
            assert!(
                c.gpu_detail.len() > 20,
                "an unavailable GPU needs a real explanation"
            );
        }
    }

    #[test]
    fn media_is_declared_unimplemented_rather_than_silently_empty() {
        let c = capabilities();
        assert!(!c.media);
        assert!(!c.media_detail.is_empty());
    }
}
