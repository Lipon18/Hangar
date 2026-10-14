use anyhow::{bail, Context, Result};
use std::{fs, path::Path, process::Command};

use crate::manifest::Dependency;

/// Make sure the dependency is cloned into `source_dir`. Returns the commit hash.
pub fn fetch(name: &str, dep: &Dependency, source_dir: &Path) -> Result<String> {
    let done_marker = source_dir.join(".hangar-fetched");

    if !done_marker.exists() {
        // A folder without the marker is a half-finished clone: remove it.
        if source_dir.exists() {
            println!("[hangar] {name}: removing incomplete clone");
            fs::remove_dir_all(source_dir)
                .with_context(|| format!("cannot remove {}", source_dir.display()))?;
        }

        if let Some(parent) = source_dir.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }

        println!("[hangar] {name}: cloning {} @ {}", dep.git, dep.tag);
        let status = Command::new("git")
            .args([
                "-c", "advice.detachedHead=false",
                "clone",
                "--depth", "1",
                "--branch", &dep.tag,
                "--recurse-submodules",
                "--shallow-submodules",
                &dep.git,
            ])
            .arg(source_dir)
            .status()
            .context("failed to run git, is it installed and on PATH?")?;

        if !status.success() {
            let _ = fs::remove_dir_all(source_dir);
            bail!("git clone failed for '{name}' (check url and tag '{}')", dep.tag);
        }

        fs::write(&done_marker, "").context("cannot write fetch marker")?;
    } else {
        println!("[hangar] {name}: source already present");
    }

    git_commit(source_dir)
}

/// Read the full commit hash of HEAD.
fn git_commit(dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .context("failed to run git rev-parse")?;

    if !out.status.success() {
        bail!("git rev-parse failed in {}", dir.display());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}