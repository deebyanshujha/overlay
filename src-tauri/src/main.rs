// No console window in release; `--restore-original` still prints to an
// attached console when run from a terminal, which is where it is used from.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Writes a line to stdout, ignoring the failure if there is nowhere to
/// write it.
///
/// The release binary is a GUI subsystem application, so it has no console
/// of its own; `attach_console` borrows the parent's when there is one.
/// When there is not — launched from Explorer, or a caller that closed the
/// pipe — `println!` *panics*. That is tolerable for `--version` and
/// unacceptable for `--restore-original`, where it would turn a successful
/// restore into what looks like a crash.
macro_rules! out {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();
        let _ = writeln!(handle, $($arg)*);
        let _ = handle.flush();
    }};
}

/// The same, for diagnostics that belong on stderr.
macro_rules! err_out {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let stderr = std::io::stderr();
        let mut handle = stderr.lock();
        let _ = writeln!(handle, $($arg)*);
    }};
}

/// Entry point.
///
/// The recovery flags are handled *before* Tauri is initialised. That is the
/// point of them: if the UI cannot start — a corrupt frontend, a missing
/// WebView2, a panic in setup — the user must still be able to get their
/// desktop back. This path needs nothing but SQLite and Win32.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--help" || a == "-h" || a == "/?") {
        print_help();
        return;
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        out!("OVERLAY {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args
        .iter()
        .any(|a| a == "--restore-original" || a == "--recover")
    {
        std::process::exit(run_recovery());
    }

    if args.iter().any(|a| a == "--status") {
        std::process::exit(print_status());
    }

    overlay_lib::run()
}

/// Restores the original desktop without starting the GUI.
fn run_recovery() -> i32 {
    attach_console();
    match overlay_lib::restoration::restore_headless() {
        Ok(report) => {
            out!("{}", overlay_lib::restoration::format_report(&report));
            if report.verified {
                0
            } else {
                // Non-zero so a script can tell a partial restore from a
                // complete one.
                2
            }
        }
        Err(e) => {
            err_out!("OVERLAY could not restore your desktop: {e}");
            err_out!(
                "\nYour original settings are still recorded. Try again, or open OVERLAY and \
                 use Settings → Recovery."
            );
            1
        }
    }
}

/// Reports what OVERLAY currently owns, without changing anything.
fn print_status() -> i32 {
    attach_console();
    use overlay_lib::{journal::Journal, storage, storage::paths::Paths};
    use parking_lot::Mutex;
    use std::sync::Arc;

    let Ok(paths) = Paths::resolve() else {
        err_out!("Could not locate OVERLAY's data folder.");
        return 1;
    };
    if !paths.db.is_file() {
        out!("OVERLAY has never changed anything on this machine.");
        return 0;
    }
    let Ok(conn) = storage::open(&paths.db) else {
        err_out!("Could not open {}", paths.db.display());
        return 1;
    };
    let journal = Journal::new(Arc::new(Mutex::new(conn)));

    let owned = journal.owned().unwrap_or_default();
    let pending = journal.unsettled().unwrap_or_default();

    out!("OVERLAY {}", env!("CARGO_PKG_VERSION"));
    out!("Data folder: {}", paths.root.display());
    out!();
    if owned.is_empty() {
        out!("No Windows settings are currently changed by OVERLAY.");
    } else {
        out!("{} setting(s) currently changed by OVERLAY:", owned.len());
        for s in &owned {
            out!(
                "  {:<40} now {:<16} (module: {})",
                s.target.label(),
                s.applied.describe(),
                s.module_id
            );
        }
        out!("\nRun with --restore-original to put them all back.");
    }
    if !pending.is_empty() {
        out!(
            "\n{} change(s) were interrupted and will be reconciled on next start.",
            pending.len()
        );
    }
    0
}

/// Attaches to the parent console so output appears in the terminal the user
/// ran OVERLAY from. Without this the GUI subsystem swallows stdout entirely
/// and the recovery command would appear to do nothing.
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}

fn print_help() {
    attach_console();
    out!(
        "OVERLAY {} — Windows desktop customization studio

USAGE:
    overlay.exe [OPTIONS]

With no options, OVERLAY starts normally.

OPTIONS:
    --restore-original   Undo every change OVERLAY has made and put your
                         original desktop back. Works without the interface,
                         so it still works if OVERLAY will not open.
                         Exit code 0 when fully verified, 2 when partial.
    --recover            Same as --restore-original.
    --status             List the Windows settings OVERLAY currently owns.
                         Changes nothing.
    --profile <name>     Apply a profile by name or id and stay in the tray,
                         e.g. --profile Developer. Uses the same
                         transactional switch the interface does.
    --minimised          Start in the tray without showing the window.
                         Used by the start-with-Windows entry.
    --version            Print the version.
    --help               Print this help.

Your original desktop settings are recorded before OVERLAY changes anything,
and restoring never touches a setting OVERLAY did not change.",
        env!("CARGO_PKG_VERSION")
    );
}
