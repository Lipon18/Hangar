use anyhow::{bail, Context, Result};
use std::{
    ffi::OsStr,
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

/// Create a CMake command.
///
/// Production uses the normal CMake executable.
/// Windows tests can supply a .cmd script instead.
fn cmake_command(program: &OsStr) -> Command {
    let path = Path::new(program);

    #[cfg(windows)]
    {
        if path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
        {
            let mut cmd = Command::new("cmd.exe");
            cmd.arg("/D").arg("/C").arg(path);
            return cmd;
        }
    }

    Command::new(path)
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
    installed: &[PathBuf],
) -> Result<bool> {
    build_with_cmake(
        name,
        dep,
        profile,
        paths,
        source_dir,
        commit,
        installed,
        OsStr::new("cmake"),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_with_cmake(
    name: &str,
    dep: &Dependency,
    profile: &Profile,
    paths: &Paths,
    source_dir: &Path,
    commit: &str,
    installed: &[PathBuf],
    cmake_program: &OsStr,
) -> Result<bool> {
    let prefix = paths.export(name, &dep.tag);

    // Avoid panicking if the supplied commit is shorter than 10 bytes.
    let short_commit = commit.get(..10).unwrap_or(commit);
    let marker = prefix.join(format!(".hangar-{short_commit}"));

    if marker.exists() {
        return Ok(false);
    }

    // Remove leftovers from an interrupted installation.
    if prefix.exists() {
        fs::remove_dir_all(&prefix)
            .with_context(|| format!("cannot remove {}", prefix.display()))?;
    }

    fs::create_dir_all(&prefix)
        .with_context(|| format!("cannot create {}", prefix.display()))?;

    // Header-only dependencies do not need CMake.
    if !dep.headers.is_empty() {
        install_headers(name, dep, source_dir, &prefix)?;
        fs::write(&marker, "")?;
        return Ok(true);
    }

    // Compiled dependency: configure, build, and install.
    let build_dir = paths.build(name, &dep.tag);

    fs::create_dir_all(&build_dir)
        .with_context(|| format!("cannot create {}", build_dir.display()))?;

    println!("[hangar] {name}: configuring");

    let mut cfg = cmake_command(cmake_program);

    cfg.arg("-S")
        .arg(source_dir)
        .arg("-B")
        .arg(&build_dir)
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

    if let Some(generator) = &profile.generator {
        cfg.args(["-G", generator]);
    }

    if let Some(arch) = &profile.arch {
        cfg.args(["-A", arch]);
    }

    for option in &dep.options {
        cfg.arg(format!("-D{option}"));
    }

    run(&mut cfg, &format!("cmake configure for '{name}'"))?;

    println!("[hangar] {name}: building ({})", profile.build_type);

    run(
        &mut cmake_command(cmake_program)
            .arg("--build")
            .arg(&build_dir)
            .args(["--config", &profile.build_type, "--parallel"]),
        &format!("cmake build for '{name}'"),
    )?;

    println!("[hangar] {name}: installing");

    run(
        &mut cmake_command(cmake_program)
            .arg("--install")
            .arg(&build_dir)
            .args(["--config", &profile.build_type]),
        &format!("cmake install for '{name}'"),
    )?;

    // Only mark the dependency installed after all stages succeed.
    fs::write(&marker, "")?;

    Ok(true)
}

/// Copy configured header paths into the installation's include directory.
///
/// Both directories and files are copied into include/ under their own
/// name. A directory keeps its name and nested layout, so `glm` becomes
/// `include/glm/...` and `<glm/glm.hpp>` resolves. A single file is copied
/// directly into include/.
fn install_headers(
    name: &str,
    dep: &Dependency,
    source_dir: &Path,
    prefix: &Path,
) -> Result<()> {
    let include = prefix.join("include");

    for header_path in &dep.headers {
        let from = source_dir.join(header_path);

        if !from.exists() {
            bail!(
                "{name}: header path '{header_path}' not found in the repository"
            );
        }

        // Works for directories and files alike: keep the last path
        // component as the name inside include/.
        let entry_name = from
            .file_name()
            .with_context(|| format!("{name}: invalid header path '{header_path}'"))?;

        copy_recursive(&from, &include.join(entry_name))?;

        println!("[hangar] {name}: copying headers from {header_path}");
    }

    Ok(())
}

fn copy_recursive(from: &Path, to: &Path) -> Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;

        for entry in fs::read_dir(from)? {
            let entry = entry?;

            copy_recursive(
                &entry.path(),
                &to.join(entry.file_name()),
            )?;
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

    /// Header-only dependency whose headers live in the `example` folder
    /// of the repository, like `glm` in the glm repository.
    fn header_dependency() -> Dependency {
        Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec![],
            headers: vec!["example".to_string()],
        }
    }

    fn compiled_dependency() -> Dependency {
        Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec!["SDL_TESTS=OFF".to_string()],
            headers: vec![],
        }
    }

    fn prepare_compiled_source(root: &Path) -> PathBuf {
        let source = root.join("compiled-source");
        fs::create_dir_all(&source).unwrap();

        fs::write(
            source.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.20)\n\
             project(HangarTest LANGUAGES NONE)\n",
        )
        .unwrap();

        source
    }

    // Windows-only fake CMake script. It logs every invocation and can
    // simulate failure at configure, build, or install.
    #[cfg(windows)]
    fn fake_cmake(root: &Path, fail_stage: Option<&str>) -> PathBuf {
        let script = root.join("fake-cmake.cmd");
        let log = root.join("cmake-args.txt");
        let fail = fail_stage.unwrap_or("none");

        let contents = format!(
            "@echo off\r\n\
             >>\"{}\" echo %*\r\n\
             if \"{}\"==\"configure\" if \"%1\"==\"-S\" exit /b 1\r\n\
             if \"{}\"==\"build\" if \"%1\"==\"--build\" exit /b 1\r\n\
             if \"{}\"==\"install\" if \"%1\"==\"--install\" exit /b 1\r\n\
             exit /b 0\r\n",
            log.display(),
            fail,
            fail,
            fail,
        );

        fs::write(&script, contents).unwrap();
        script
    }

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

    // ---------------------------------------------------------
    // Header-only tests
    // ---------------------------------------------------------

    #[test]
    fn installs_header_only_dependency() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("example/nested")).unwrap();

        fs::write(
            source.join("example/example.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        fs::write(
            source.join("example/nested/detail.hpp"),
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
        print_tree(&prefix);

        // The folder name is preserved: include/example/..., so that
        // `#include <example/example.hpp>` works.
        assert!(prefix.join("include").is_dir());
        assert!(prefix.join("include/example").is_dir());
        assert!(prefix.join("include/example/example.hpp").is_file());
        assert!(prefix.join("include/example/nested/detail.hpp").is_file());

        // The folder's contents must NOT be flattened into include/.
        assert!(!prefix.join("include/example.hpp").exists());
        assert!(!prefix.join("include/nested").exists());

        assert_eq!(
            fs::read_to_string(prefix.join("include/example/example.hpp")).unwrap(),
            "#pragma once\n"
        );

        assert_eq!(
            fs::read_to_string(
                prefix.join("include/example/nested/detail.hpp")
            )
            .unwrap(),
            "#pragma once\n"
        );

        assert!(prefix.join(".hangar-0123456789").is_file());
    }

    #[test]
    fn installs_single_header_file_directly_into_include() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("single")).unwrap();

        fs::write(
            source.join("single/lib.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);

        let dep = Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec![],
            headers: vec!["single/lib.hpp".to_string()],
        };

        let installed = build(
            "single",
            &dep,
            &profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
        )
        .unwrap();

        assert!(installed);

        let prefix = paths.export("single", &dep.tag);

        assert!(prefix.join("include/lib.hpp").is_file());
        assert!(!prefix.join("include/single").exists());
    }

    #[test]
    fn skips_already_installed_dependency() {
        let temp = tempdir().unwrap();
        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = header_dependency();

        let prefix = paths.export("example", &dep.tag);
        fs::create_dir_all(&prefix).unwrap();

        fs::write(prefix.join(".hangar-0123456789"), "").unwrap();

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
                .contains("header path 'example' not found")
        );
    }

    #[test]
    fn short_commit_does_not_panic() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("example")).unwrap();

        fs::write(
            source.join("example/example.hpp"),
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

    // ---------------------------------------------------------
    // CMake tests (Windows)
    // ---------------------------------------------------------

    #[cfg(windows)]
    #[test]
    fn compiled_dependency_runs_all_cmake_stages() {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());
        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = compiled_dependency();
        let fake = fake_cmake(temp.path(), None);

        let installed = build_with_cmake(
            "compiled-example",
            &dep,
            &profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
            fake.as_os_str(),
        )
        .unwrap();

        assert!(installed);

        let prefix = paths.export("compiled-example", &dep.tag);
        assert!(prefix.join(".hangar-0123456789").is_file());

        let log = fs::read_to_string(
            temp.path().join("cmake-args.txt"),
        )
        .unwrap();

        assert!(log.contains("-S"));
        assert!(log.contains("--build"));
        assert!(log.contains("--install"));
        assert!(log.contains("-DCMAKE_BUILD_TYPE=Release"));
        assert!(log.contains("-DSDL_TESTS=OFF"));
    }

    #[cfg(windows)]
    #[test]
    fn cmake_configure_failure_does_not_write_marker() {
        assert_cmake_failure_does_not_write_marker("configure");
    }

    #[cfg(windows)]
    #[test]
    fn cmake_build_failure_does_not_write_marker() {
        assert_cmake_failure_does_not_write_marker("build");
    }

    #[cfg(windows)]
    #[test]
    fn cmake_install_failure_does_not_write_marker() {
        assert_cmake_failure_does_not_write_marker("install");
    }

    #[cfg(windows)]
    fn assert_cmake_failure_does_not_write_marker(stage: &str) {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());
        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);
        let dep = compiled_dependency();
        let fake = fake_cmake(temp.path(), Some(stage));

        let result = build_with_cmake(
            "compiled-example",
            &dep,
            &profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
            fake.as_os_str(),
        );

        assert!(result.is_err(), "Expected {stage} failure");

        let prefix = paths.export("compiled-example", &dep.tag);
        assert!(
            !prefix.join(".hangar-0123456789").exists(),
            "Marker must not exist after {stage} failure"
        );
    }
}