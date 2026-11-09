//! Build script.
//!
//! Its one non-obvious job is making sure every binary this crate produces —
//! the application *and* each test harness — carries exactly one application
//! manifest, the one in `overlay.manifest`. See that file for why it is
//! needed; the short version is that without it, test binaries cannot resolve
//! `TaskDialogIndirect` and die before running a single test.
//!
//! "Exactly one" is the fiddly part, and the reason for the arrangement
//! below:
//!
//! * tauri-build normally embeds a manifest into a Windows *resource*, but
//!   only for the application binary — test harnesses get nothing, which is
//!   where the original failure came from.
//! * Adding a linker manifest on top of that resource is a hard error
//!   (`CVT1100: duplicate resource`), so the two cannot simply be combined.
//! * Cargo's `rustc-link-arg-tests` does not reach the lib's own unit-test
//!   harness, which is where most of the suite lives, so scoping the linker
//!   flag to test targets does not work either.
//!
//! So tauri-build is told to emit no manifest at all, and the linker attaches
//! ours to every target. One manifest, one code path, no special cases.

fn main() {
    #[cfg(target_os = "windows")]
    {
        let manifest_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("overlay.manifest");
        println!("cargo:rerun-if-changed=overlay.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest_path.display()
        );

        tauri_build::try_build(
            tauri_build::Attributes::new().windows_attributes(
                tauri_build::WindowsAttributes::new_without_app_manifest(),
            ),
        )
        .expect("failed to build OVERLAY");
        return;
    }

    #[cfg(not(target_os = "windows"))]
    tauri_build::build()
}
