use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{manifest::Dependency, paths::Paths, profile::Profile};

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let status = cmd
        .status()
        .context("failed to run cmake, is it installed and on PATH?")?;
    if !status.success() {
        bail!("{what} failed");
    }
    Ok(())
}

/// Install one dependency into its own prefix.
/// Returns true if it installed something, false if it was already installed.
pub fn build(
    name: &str,
    dep: &Dependency,
    profile: &Profile,
    paths: &Paths,
    source_dir: &Path,
    commit: &str,
    installed: &[PathBuf], // prefixes of dependencies installed before this one
) -> Result<bool> {
    let prefix = paths.export(name, &dep.tag);
    let marker = prefix.join(format!(".hangar-{}", &commit[..10]));
    if marker.exists() {
        return Ok(false);
    }

    // Remove leftovers of an interrupted install, then start clean
    if prefix.exists() {
        fs::remove_dir_all(&prefix)
            .with_context(|| format!("cannot remove {}", prefix.display()))?;
    }
    fs::create_dir_all(&prefix)
        .with_context(|| format!("cannot create {}", prefix.display()))?;

    // Header-only: just copy the headers, no CMake
    if !dep.headers.is_empty() {
        install_headers(name, dep, source_dir, &prefix)?;
        fs::write(&marker, "")?;
        return Ok(true);
    }

    // Compiled: configure, build, install
    let build_dir = paths.build(name, &dep.tag);
    fs::create_dir_all(&build_dir)
        .with_context(|| format!("cannot create {}", build_dir.display()))?;

    println!("[hangar] {name}: configuring");
    let mut cfg = Command::new("cmake");
    cfg.arg("-S").arg(source_dir)
        .arg("-B").arg(&build_dir)
        .arg(format!("-DCMAKE_BUILD_TYPE={}", profile.build_type))
        .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()));
    if !installed.is_empty() {
        let list = installed
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(";");
        cfg.arg(format!("-DCMAKE_PREFIX_PATH={list}"));
    }
    if let Some(g) = &profile.generator {
        cfg.args(["-G", g]);
    }
    if let Some(a) = &profile.arch {
        cfg.args(["-A", a]);
    }
    for opt in &dep.options {
        cfg.arg(format!("-D{opt}"));
    }
    run(&mut cfg, &format!("cmake configure for '{name}'"))?;

    println!("[hangar] {name}: building ({})", profile.build_type);
    run(
        Command::new("cmake")
            .arg("--build").arg(&build_dir)
            .args(["--config", &profile.build_type, "--parallel"]),
        &format!("cmake build for '{name}'"),
    )?;

    println!("[hangar] {name}: installing");
    run(
        Command::new("cmake")
            .arg("--install").arg(&build_dir)
            .args(["--config", &profile.build_type]),
        &format!("cmake install for '{name}'"),
    )?;

    // Only after everything succeeded
    fs::write(&marker, "")?;
    Ok(true)
}

fn install_headers(name: &str, dep: &Dependency, source_dir: &Path, prefix: &Path) -> Result<()> {
    let include = prefix.join("include");
    for h in &dep.headers {
        let from = source_dir.join(h);
        if !from.exists() {
            bail!("{name}: header path '{h}' not found in the repository");
        }
        let file_name = from.file_name().context("invalid header path")?;
        println!("[hangar] {name}: copying headers from {h}");
        copy_recursive(&from, &include.join(file_name))?;
    }
    Ok(())
}

fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
    }
    Ok(())
}