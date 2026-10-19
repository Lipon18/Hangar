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

/// Name of the file that marks a finished installation.
///
/// Header-only dependencies have one marker: `.hangar-<commit>`.
/// Compiled dependencies have one marker per build type:
/// `.hangar-<commit>-<build type>`.
fn marker_name(short_commit: &str, build_type: Option<&str>) -> String {
    match build_type {
        Some(build_type) => format!(".hangar-{short_commit}-{build_type}"),
        None => format!(".hangar-{short_commit}"),
    }
}

/// Delete whatever is in the prefix and start with an empty folder.
fn reset_prefix(prefix: &Path) -> Result<()> {
    if prefix.exists() {
        fs::remove_dir_all(prefix)
            .with_context(|| format!("cannot remove {}", prefix.display()))?;
    }

    fs::create_dir_all(prefix)
        .with_context(|| format!("cannot create {}", prefix.display()))?;

    Ok(())
}

/// True if the prefix holds a finished install of ANY build type of this
/// commit. It must not matter which profile installed it: Debug and Release
/// can come from two different profile files that share one prefix.
fn has_marker_for_commit(prefix: &Path, short_commit: &str) -> bool {
    let wanted = format!(".hangar-{short_commit}-");

    fs::read_dir(prefix)
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(&wanted))
        })
        .unwrap_or(false)
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

    // Header-only dependencies do not need CMake and do not depend on the
    // build type, so they have a single marker.
    if !dep.headers.is_empty() {
        let marker = prefix.join(marker_name(short_commit, None));

        if marker.exists() {
            return Ok(false);
        }

        // Remove leftovers from an interrupted installation.
        reset_prefix(&prefix)?;

        install_headers(name, dep, source_dir, &prefix)?;
        fs::write(&marker, "")?;

        return Ok(true);
    }

    // Compiled dependency: every build type is installed into the same
    // prefix and has its own marker, so a run can resume with only the
    // build types that are still missing.
    let marker_for = |build_type: &str| prefix.join(marker_name(short_commit, Some(build_type)));

    let missing: Vec<&str> = profile
        .build_types
        .iter()
        .map(String::as_str)
        .filter(|build_type| !marker_for(*build_type).exists())
        .collect();

    if missing.is_empty() {
        return Ok(false);
    }

    // An overlay replaces the repository's own CMakeLists.txt. Check it
    // before anything is deleted or built.
    let configure_source: &Path = match &dep.overlay {
        Some(overlay) => {
            if !overlay.join("CMakeLists.txt").is_file() {
                bail!(
                    "{name}: overlay '{}' has no CMakeLists.txt",
                    overlay.display()
                );
            }
            overlay.as_path()
        }
        None => source_dir,
    };

    // No finished build type for this commit: whatever is in the prefix is
    // leftover from an interrupted run (or from another commit), so clean
    // it. If at least one build type is finished, keep the prefix and only
    // add the missing ones next to it.
    // Look at the markers of every build type, not only the ones listed in
    // this profile. Otherwise a Release-only profile would wipe the Debug
    // install that a Debug-only profile made into the same prefix.
    let has_finished_build = has_marker_for_commit(&prefix, short_commit);

    if !has_finished_build {
        reset_prefix(&prefix)?;
    }

    for build_type in missing {
        let build_dir = paths.build(name, &dep.tag, build_type);

        fs::create_dir_all(&build_dir)
            .with_context(|| format!("cannot create {}", build_dir.display()))?;

        println!("[hangar] {name}: configuring ({build_type})");

        let mut cfg = cmake_command(cmake_program);

        cfg.arg("-S")
            .arg(configure_source)
            .arg("-B")
            .arg(&build_dir)
            .arg(format!("-DCMAKE_BUILD_TYPE={build_type}"))
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()));

        // An overlay builds the sources of the shared git clone, which it
        // finds through HANGAR_SOURCE_DIR. The clone itself is never changed.
        if dep.overlay.is_some() {
            cfg.arg(format!(
                "-DHANGAR_SOURCE_DIR={}",
                source_dir.to_string_lossy().replace('\\', "/")
            ));
        }

        // Debug and Release libraries share one prefix. Give Debug libraries
        // a "d" suffix so they don't overwrite the Release ones. Libraries
        // that set their own postfix keep it, and dependency options below
        // can still override this.
        if build_type == "Debug" {
            cfg.arg("-DCMAKE_DEBUG_POSTFIX=d");
        }

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

        run(&mut cfg, &format!("cmake configure for '{name}' ({build_type})"))?;

        println!("[hangar] {name}: building ({build_type})");

        run(
            &mut cmake_command(cmake_program)
                .arg("--build")
                .arg(&build_dir)
                .args(["--config", build_type, "--parallel"]),
            &format!("cmake build for '{name}' ({build_type})"),
        )?;

        println!("[hangar] {name}: installing ({build_type})");

        run(
            &mut cmake_command(cmake_program)
                .arg("--install")
                .arg(&build_dir)
                .args(["--config", build_type]),
            &format!("cmake install for '{name}' ({build_type})"),
        )?;

        // Only mark this build type installed after all its stages succeed.
        fs::write(marker_for(build_type), "")?;
    }

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

    fn test_profile_with(root: &Path, build_types: &[&str]) -> Profile {
        Profile {
            name: "Test".to_string(),
            os: "Windows".to_string(),
            compiler: "MSVC".to_string(),
            build_types: build_types.iter().map(|b| b.to_string()).collect(),
            dependencies_dir: root.to_string_lossy().into_owned(),
            generator: None,
            arch: Some("x64".to_string()),
        }
    }

    fn test_profile(root: &Path) -> Profile {
        test_profile_with(root, &["Release"])
    }

    /// Header-only dependency whose headers live in the `example` folder
    /// of the repository, like `glm` in the glm repository.
    fn header_dependency() -> Dependency {
        Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec![],
            headers: vec!["example".to_string()],
            overlay: None,
        }
    }

    fn compiled_dependency() -> Dependency {
        Dependency {
            git: "unused".to_string(),
            tag: "v1.0.0".to_string(),
            options: vec!["SDL_TESTS=OFF".to_string()],
            headers: vec![],
            overlay: None,
        }
    }

    // Only used by the Windows fake-CMake tests.
    #[cfg(windows)]
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
    fn header_only_dependency_ignores_build_types() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("source");

        fs::create_dir_all(source.join("example")).unwrap();

        fs::write(
            source.join("example/example.hpp"),
            "#pragma once\n",
        )
        .unwrap();

        let profile = test_profile_with(temp.path(), &["Debug", "Release"]);
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

        // One marker, no per-build-type markers.
        assert!(prefix.join(".hangar-0123456789").is_file());
        assert!(!prefix.join(".hangar-0123456789-Debug").exists());
        assert!(!prefix.join(".hangar-0123456789-Release").exists());
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
            overlay: None,
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
    // Compiled dependency: markers (runs everywhere, never calls CMake)
    // ---------------------------------------------------------

    #[test]
    fn skips_compiled_dependency_when_every_build_type_is_installed() {
        let temp = tempdir().unwrap();
        let profile = test_profile_with(temp.path(), &["Debug", "Release"]);
        let paths = Paths::new(&profile);
        let dep = compiled_dependency();

        let prefix = paths.export("compiled-example", &dep.tag);
        fs::create_dir_all(&prefix).unwrap();

        fs::write(prefix.join(".hangar-0123456789-Debug"), "").unwrap();
        fs::write(prefix.join(".hangar-0123456789-Release"), "").unwrap();

        // Returns before CMake would be started.
        let installed = build(
            "compiled-example",
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
    fn finds_markers_of_any_build_type_for_the_same_commit() {
        let temp = tempdir().unwrap();
        let prefix = temp.path().join("prefix");

        // No folder yet.
        assert!(!has_marker_for_commit(&prefix, "0123456789"));

        fs::create_dir_all(&prefix).unwrap();
        assert!(!has_marker_for_commit(&prefix, "0123456789"));

        // A header-only marker is not a build type marker.
        fs::write(prefix.join(".hangar-0123456789"), "").unwrap();
        assert!(!has_marker_for_commit(&prefix, "0123456789"));

        // A marker of another commit does not count.
        fs::write(prefix.join(".hangar-aaaaaaaaaa-Debug"), "").unwrap();
        assert!(!has_marker_for_commit(&prefix, "0123456789"));

        // Any build type of this commit counts.
        fs::write(prefix.join(".hangar-0123456789-Debug"), "").unwrap();
        assert!(has_marker_for_commit(&prefix, "0123456789"));
    }

    #[test]
    fn overlay_without_cmakelists_fails_before_touching_the_prefix() {
        let temp = tempdir().unwrap();
        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);

        let mut dep = compiled_dependency();
        dep.overlay = Some(temp.path().join("overlays/missing"));

        let result = build(
            "compiled-example",
            &dep,
            &profile,
            &paths,
            temp.path(),
            "0123456789abcdef",
            &[],
        );

        assert!(result.is_err());

        assert!(
            format!("{:#}", result.unwrap_err())
                .contains("has no CMakeLists.txt")
        );

        // Nothing was created or deleted.
        assert!(!paths.export("compiled-example", &dep.tag).exists());
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
        assert!(prefix.join(".hangar-0123456789-Release").is_file());

        let log = fs::read_to_string(
            temp.path().join("cmake-args.txt"),
        )
        .unwrap();

        assert!(log.contains("-S"));
        assert!(log.contains("--build"));
        assert!(log.contains("--install"));
        assert!(log.contains("-DCMAKE_BUILD_TYPE=Release"));
        assert!(log.contains("-DSDL_TESTS=OFF"));

        // The debug postfix is only for Debug builds.
        assert!(!log.contains("-DCMAKE_DEBUG_POSTFIX=d"));
    }

    #[cfg(windows)]
    #[test]
    fn compiled_dependency_builds_every_build_type_into_one_prefix() {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());
        let profile = test_profile_with(temp.path(), &["Debug", "Release"]);
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
        assert!(prefix.join(".hangar-0123456789-Debug").is_file());
        assert!(prefix.join(".hangar-0123456789-Release").is_file());

        // Each build type has its own build folder.
        assert!(paths.build("compiled-example", &dep.tag, "Debug").is_dir());
        assert!(paths.build("compiled-example", &dep.tag, "Release").is_dir());

        let log = fs::read_to_string(
            temp.path().join("cmake-args.txt"),
        )
        .unwrap();

        assert!(log.contains("-DCMAKE_BUILD_TYPE=Debug"));
        assert!(log.contains("-DCMAKE_BUILD_TYPE=Release"));
        assert!(log.contains("--config Debug"));
        assert!(log.contains("--config Release"));
        assert!(log.contains("-DCMAKE_DEBUG_POSTFIX=d"));
    }

    #[cfg(windows)]
    #[test]
    fn resumes_with_only_the_missing_build_types() {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());
        let profile = test_profile_with(temp.path(), &["Debug", "Release"]);
        let paths = Paths::new(&profile);
        let dep = compiled_dependency();
        let fake = fake_cmake(temp.path(), None);

        // Release is already finished from an earlier run.
        let prefix = paths.export("compiled-example", &dep.tag);
        fs::create_dir_all(&prefix).unwrap();
        fs::write(prefix.join(".hangar-0123456789-Release"), "").unwrap();
        fs::write(prefix.join("release-library.lib"), "keep me").unwrap();

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

        // The finished Release install was not wiped...
        assert!(prefix.join("release-library.lib").is_file());
        assert!(prefix.join(".hangar-0123456789-Release").is_file());

        // ...and Debug was added next to it.
        assert!(prefix.join(".hangar-0123456789-Debug").is_file());

        let log = fs::read_to_string(
            temp.path().join("cmake-args.txt"),
        )
        .unwrap();

        assert!(log.contains("-DCMAKE_BUILD_TYPE=Debug"));
        assert!(!log.contains("-DCMAKE_BUILD_TYPE=Release"));
    }

    #[cfg(windows)]
    #[test]
    fn a_second_profile_keeps_the_build_types_of_the_first() {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());

        // Two profile files that share one prefix: Debug-only, then Release-only.
        let debug_profile = test_profile_with(temp.path(), &["Debug"]);
        let release_profile = test_profile_with(temp.path(), &["Release"]);

        let paths = Paths::new(&debug_profile);
        let dep = compiled_dependency();
        let fake = fake_cmake(temp.path(), None);

        let prefix = paths.export("compiled-example", &dep.tag);

        build_with_cmake(
            "compiled-example",
            &dep,
            &debug_profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
            fake.as_os_str(),
        )
        .unwrap();

        // Stands in for the Debug library files.
        fs::write(prefix.join("debug-library.lib"), "keep me").unwrap();

        build_with_cmake(
            "compiled-example",
            &dep,
            &release_profile,
            &paths,
            &source,
            "0123456789abcdef",
            &[],
            fake.as_os_str(),
        )
        .unwrap();

        // The Release run must not have deleted the Debug install.
        assert!(prefix.join("debug-library.lib").is_file());
        assert!(prefix.join(".hangar-0123456789-Debug").is_file());
        assert!(prefix.join(".hangar-0123456789-Release").is_file());
    }

    #[cfg(windows)]
    #[test]
    fn overlay_is_configured_instead_of_the_repository() {
        let temp = tempdir().unwrap();
        let source = prepare_compiled_source(temp.path());

        let overlay = temp.path().join("overlay");
        fs::create_dir_all(&overlay).unwrap();
        fs::write(
            overlay.join("CMakeLists.txt"),
            "project(Overlay LANGUAGES NONE)\n",
        )
        .unwrap();

        let profile = test_profile(temp.path());
        let paths = Paths::new(&profile);

        let mut dep = compiled_dependency();
        dep.overlay = Some(overlay.clone());

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

        let log = fs::read_to_string(
            temp.path().join("cmake-args.txt"),
        )
        .unwrap();

        // CMake configures the overlay folder, not the repository...
        assert!(log.contains(&overlay.display().to_string()));
        assert!(!log.contains(&source.display().to_string()));

        // ...and is told where the repository clone is.
        let clone = source.to_string_lossy().replace('\\', "/");
        assert!(log.contains(&format!("-DHANGAR_SOURCE_DIR={clone}")));
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
        let profile = test_profile_with(temp.path(), &["Debug", "Release"]);
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

        for build_type in ["Debug", "Release"] {
            assert!(
                !prefix
                    .join(format!(".hangar-0123456789-{build_type}"))
                    .exists(),
                "Marker for {build_type} must not exist after {stage} failure"
            );
        }
    }
}