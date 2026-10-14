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
    //let marker = prefix.join(format!(".hangar-{}", &commit[..10]));
    let short_commit = commit.get(..10).unwrap_or(commit);
    let marker = prefix.join(format!(".hangar-{short_commit}"));
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
    for header_path in &dep.headers {
        let from = source_dir.join(header_path);
        if !from.exists() {
            bail!("{name}: header path '{header_path}' not found in the repository");
        }

        if from.is_dir() {
            fs::create_dir_all(&include)?;
        
            for entry in fs::read_dir(&from)? {
                let entry = entry?;

                copy_recursive(&entry.path(), &include.join(entry.file_name()),)?;
            }
        } else {
            let file_name = from.file_name().context("invalid header path")?;

            copy_recursive(&from, &include.join(file_name))?;
        }
        println!("[hangar] {name}: copying headers from {header_path}");
        //let file_name = from.file_name().context("invalid header path")?;
        //println!("[hangar] {name}: copying headers from {h}");
        //if from.is_dir() {
            //copy_recursive(&from, &include.join(file_name))?;
        //} else {
            //copy_recursive(&from, &include.join(file_name))?;
        //}
        //copy_recursive(&from, &include.join(file_name))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_profile(root: &Path) -> Profile {
        Profile {
            name: "Test".to_string(),
            os: "Windows".to_string(),
            compiler: "MSVC".to_string(),
            build_type: "Release".to_string(),
            dependencies_dir: root.to_string_lossy().into_owned(),
            generator: None,
            arch: Some("x64".to_string()),
        }
    }

    fn header_dependency() -> Dependency {
        Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec![],
            headers: vec!["include".to_string()],
        }
    }

    // Diagnostic helper: print the entire installation directory tree.
    fn print_tree(path: &Path) {
        if !path.exists() {
            println!("MISSING: {}", path.display());
            return;
        }

        println!("{}", path.display());

        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                print_tree(&entry.path());
            }
        }
    }

    #[test]
    fn installs_header_only_dependency() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("include/nested")).unwrap();

        fs::write(
            source.join("include/example.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        fs::write(
            source.join("include/nested/detail.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = header_dependency();

        let installed = build(
            "example",
            &dep,
            &profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
        )
        .unwrap();

        assert!(installed);

        let prefix = paths.export("example", &dep.tag);

        // Print the tree to help diagnose any unexpected layout.
        print_tree(&prefix);

        assert!(prefix.join("include").is_dir());
        assert!(prefix.join("include/example.hpp").is_file());
        assert!(
            prefix
                .join("include/nested/detail.hpp")
                .is_file()
        );

        assert_eq!(
            fs::read_to_string(prefix.join("include/example.hpp")).unwrap(),
            "#pragma once\n"
        );

        assert_eq!(
            fs::read_to_string(
                prefix.join("include/nested/detail.hpp")
            )
            .unwrap(),
            "#pragma once\n"
        );

        assert!(prefix.join(".hangar-0123456789").is_file());
    }

    #[test]
    fn skips_already_installed_dependency() {
        let temp = tempdir().unwrap();
        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = header_dependency();

        let prefix = paths.export("example", &dep.tag);

        fs::create_dir_all(&prefix).unwrap();

        fs::write(
            prefix.join(".hangar-0123456789"),
            "",
        )
        .unwrap();

        let installed = build(
            "example",
            &dep,
            &profile,
            &paths,
            temp.path(),
            "0123456789abcdef",
            &[],
        )
        .unwrap();

        assert!(!installed);
    }

    #[test]
    fn missing_header_path_returns_error() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(&source).unwrap();

        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = header_dependency();

        let result = build(
            "example",
            &dep,
            &profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
        );

        assert!(result.is_err());

        assert!(
            format!("{:#}", result.unwrap_err())
                .contains("header path 'include' not found")
        );
    }

    #[test]
    fn short_commit_does_not_panic() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("include")).unwrap();

        fs::write(
            source.join("include/example.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = header_dependency();

        let installed = build(
            "example",
            &dep,
            &profile,
            &paths,
            &source,
            "abc",
            &[],
        )
        .unwrap();

        assert!(installed);

        assert!(
            paths
                .export("example", &dep.tag)
                .join(".hangar-abc")
                .is_file()
        );
    }
}