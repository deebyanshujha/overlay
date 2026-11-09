//! How long do the operations behind each command actually take?
//!
//! Tauri runs a synchronous command on the **main thread**, which is also
//! the window event loop. Anything slow there freezes the whole interface.
//! This measures the work each command does so the expensive ones can be
//! found rather than guessed at.
//!
//! `cargo test --test probe_latency -- --ignored --nocapture`

#![cfg(windows)]

use std::time::Instant;

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    let flag = if ms > 100.0 {
        "  <-- BLOCKS THE UI"
    } else if ms > 16.0 {
        "  <-- drops a frame"
    } else {
        ""
    };
    println!("{label:<46} {ms:>8.1} ms{flag}");
    out
}

#[test]
#[ignore = "measures the live machine"]
fn where_does_the_time_go() {
    use overlay_lib::integrations::{quickdock::QuickDock, wallflow::WallFlow};
    use overlay_lib::sys;

    println!("\n--- individual operations ---");

    time("sys::environment()", sys::environment);
    time("shell::monitors()", || sys::shell::monitors().ok());
    time("shell::taskbar_windows()", sys::shell::taskbar_windows);
    time("shell::desktop_icons_visible()", || {
        sys::shell::desktop_icons_visible().ok()
    });

    time("process::is_running(\"wallflow\")", || {
        sys::process::is_running("wallflow")
    });
    time("process::is_running(\"quickdock\")", || {
        sys::process::is_running("quickdock")
    });

    time("WallFlow::locate()", WallFlow::locate);
    time("QuickDock::locate()", QuickDock::locate);
    time("WallFlow::read_state()  [copies the WAL db]", || {
        WallFlow::read_state().ok()
    });
    time("QuickDock::read_settings()", || {
        QuickDock::read_settings().ok()
    });

    println!("\n--- what a command does ---");

    time("WallFlow::status()", WallFlow::status);
    time("QuickDock::status()", QuickDock::status);
    time("all_statuses()  [cold]", || {
        overlay_lib::commands::integrations::all_statuses()
    });
    time("all_statuses()  [cached]", || {
        overlay_lib::commands::integrations::all_statuses()
    });
    time("all_statuses()  [cached again]", || {
        overlay_lib::commands::integrations::all_statuses()
    });

    let mut sampler = overlay_lib::monitoring::Sampler::new();
    time("metrics sample (first)", || sampler.sample());
    time("metrics sample (steady state)", || sampler.sample());

    println!();
}

#[test]
#[ignore = "measures the live machine"]
fn module_status_cost() {
    use overlay_lib::journal::Journal;
    use overlay_lib::modules::{ModuleCtx, Registry};
    use overlay_lib::storage;
    use parking_lot::Mutex;
    use std::collections::{BTreeMap, HashSet};
    use std::sync::Arc;

    let conn = Arc::new(Mutex::new(storage::open_in_memory().unwrap()));
    let journal = Journal::new(conn);
    let env = Arc::new(overlay_lib::sys::environment());
    let ctx = ModuleCtx::headless(journal, env);
    let registry = Registry::new();

    println!("\n--- per-module status cost ---");
    for m in registry.iter() {
        let id = m.id();
        time(&format!("  {id}: availability()"), || m.availability(&ctx));
        time(&format!("  {id}: is_active()"), || m.is_active(&ctx));
        time(&format!("  {id}: notes()"), || m.notes(&ctx));
    }

    println!("\n--- the whole list, as list_modules() builds it ---");
    let configs = BTreeMap::new();
    let enabled = HashSet::new();
    time("registry.statuses()", || {
        registry.statuses(&ctx, &configs, &enabled)
    });
    time("registry.statuses() again", || {
        registry.statuses(&ctx, &configs, &enabled)
    });
    println!();
}
