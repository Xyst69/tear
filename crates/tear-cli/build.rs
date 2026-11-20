//! Embeds the Windows application manifest (themed controls, DPI awareness)
//! into tear.exe and tearw.exe.
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=windows/tear.rc");
    println!("cargo:rerun-if-changed=windows/tear.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let resources =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("set by Cargo")).join("windows");
    match env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("gnu") => {
            let object =
                PathBuf::from(env::var("OUT_DIR").expect("set by Cargo")).join("tear-resources.o");
            let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".to_owned());
            let status = Command::new(&windres)
                .current_dir(&resources)
                .args(["-i", "tear.rc", "-O", "coff", "-o"])
                .arg(&object)
                .status()
                .unwrap_or_else(|e| {
                    panic!("could not run {windres} (use scripts\\cargo.ps1): {e}")
                });
            assert!(
                status.success(),
                "{windres} failed to compile windows/tear.rc"
            );
            println!("cargo:rustc-link-arg-bins={}", object.display());
        }
        Ok("msvc") => {
            println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
            println!(
                "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
                resources.join("tear.manifest").display()
            );
        }
        _ => {}
    }
}
