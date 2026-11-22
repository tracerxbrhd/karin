// SPDX-License-Identifier: GPL-3.0-or-later
//! Embed only public source identity, including source-only Flatpak snapshots.
use std::{env, fs, path::Path, process::Command};

fn main() {
    for path in [
        "BUILD-REVISION",
        "BUILD-SOURCE-STATUS",
        "src",
        "resources",
        "packaging",
        "Cargo.toml",
        "Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={path}");
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
        println!("cargo:rerun-if-changed={reference}");
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
}
