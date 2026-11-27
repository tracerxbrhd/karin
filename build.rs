// SPDX-License-Identifier: GPL-3.0-or-later
//! Embed only public source identity, including source-only Flatpak snapshots.
use std::{env, fs, path::Path, process::Command};

fn main() {
    let resource =
        Path::new(&env::var_os("OUT_DIR").expect("Cargo supplies OUT_DIR")).join("karin.gresource");
    let status = Command::new("glib-compile-resources")
        .arg("resources/karin.gresource.xml")
        .arg("--sourcedir=resources")
        .arg("--sourcedir=packaging")
        .arg("--target")
        .arg(&resource)
        .status()
        .expect("glib-compile-resources is required by the native GTK build");
    assert!(status.success(), "Cannot compile Karin's bundled icons");
    for path in ["src", "resources", "packaging", "Cargo.toml", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={path}");
    }
    // Source-only packages provide these files before the first build. A Git
    // checkout does not. Watching a missing file makes Cargo rerun this script
    // and compile Karin again on every otherwise unchanged test invocation.
    for marker in ["BUILD-REVISION", "BUILD-SOURCE-STATUS"] {
        if Path::new(marker).is_file() {
            println!("cargo:rerun-if-changed={marker}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
    };
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        .and_then(|reference| git(&["rev-parse", "--git-path", &reference]))
    {
        let path = Path::new(&reference);
        if path.is_file() {
            println!("cargo:rerun-if-changed={reference}");
        } else if let Some(parent) = path.ancestors().skip(1).find(|path| path.is_dir()) {
            // A packed branch has no loose ref yet. Watch its existing parent
            // so creating a loose ref on the next commit is still detected.
            println!("cargo:rerun-if-changed={}", parent.display());
        }
    }
    if let Some(packed) =
        git(&["rev-parse", "--git-path", "packed-refs"]).filter(|path| Path::new(path).is_file())
    {
        println!("cargo:rerun-if-changed={packed}");
    }
    let revision = fs::read_to_string(Path::new("BUILD-REVISION"))
        .ok()
        .map(|s| s.trim().to_owned())
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown-source".into());
    let source_status = fs::read_to_string("BUILD-SOURCE-STATUS")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| matches!(s.as_str(), "clean" | "modified"))
        .or_else(|| {
            git(&["status", "--porcelain", "--untracked-files=normal"]).map(|status| {
                if status.is_empty() {
                    "clean".into()
                } else {
                    "modified".into()
                }
            })
        })
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=KARIN_SOURCE_STATUS={source_status}");
    println!("cargo:rustc-env=KARIN_SOURCE_REVISION={revision}");
    println!(
        "cargo:rustc-env=KARIN_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
    let compiler = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_owned())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=KARIN_BUILD_COMPILER={compiler}");
    println!(
        "cargo:rustc-env=KARIN_BUILD_TARGET={}",
        env::var("TARGET").unwrap()
    );
    let features = [
        ("auth-webkit", "CARGO_FEATURE_AUTH_WEBKIT"),
        ("browser-session-auth", "CARGO_FEATURE_BROWSER_SESSION_AUTH"),
        (
            "experimental-device-auth",
            "CARGO_FEATURE_EXPERIMENTAL_DEVICE_AUTH",
        ),
    ]
    .into_iter()
    .filter_map(|(name, variable)| env::var_os(variable).is_some().then_some(name))
    .collect::<Vec<_>>()
    .join(",");
    println!(
        "cargo:rustc-env=KARIN_BUILD_FEATURES={}",
        if features.is_empty() {
            "none"
        } else {
            &features
        }
    );
}
