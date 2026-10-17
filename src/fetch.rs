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
                "-c",
                "advice.detachedHead=false",
                "clone",
                "--depth",
                "1",
                "--branch",
                &dep.tag,
                "--recurse-submodules",
                "--shallow-submodules",
                &dep.git,
            ])
            .arg(source_dir)
            .status()
            .context("failed to run git, is it installed and on PATH?")?;

        if !status.success() {
            let _ = fs::remove_dir_all(source_dir);
            bail!(
                "git clone failed for '{name}' (check url and tag '{}')",
                dep.tag
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Output;
    use tempfile::tempdir;

    /// Run Git in a specified working directory and fail with useful output.
    fn run_git(dir: &Path, args: &[&str]) -> Output {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("Git must be installed and available on PATH");

        assert!(
            output.status.success(),
            "Git command failed: git {}\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        output
    }

    /// Create a tiny local repository with one commit and a version tag.
    fn create_local_repo(dir: &Path) -> String {
        fs::create_dir_all(dir).unwrap();

        run_git(dir, &["init"]);
        run_git(dir, &["config", "user.name", "Hangar Tests"]);
        run_git(dir, &["config", "user.email", "hangar-tests@example.com"]);

        fs::write(dir.join("hello.txt"), "hello from Hangar\n").unwrap();

        run_git(dir, &["add", "hello.txt"]);
        run_git(dir, &["commit", "-m", "Initial test commit"]);
        run_git(dir, &["tag", "v1.0.0"]);

        String::from_utf8(
            run_git(dir, &["rev-parse", "HEAD"]).stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    fn test_dependency(repo: &Path, tag: &str) -> Dependency {
        Dependency {
            git: repo.to_string_lossy().into_owned(),
            tag: tag.to_string(),
            options: vec![],
            headers: vec![],
            overlay: None,
        }
    }

    #[test]
    fn clones_local_repository_and_writes_marker() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let expected_commit = create_local_repo(&repo);

        let source = temp.path().join("cache").join("example");
        let dep = test_dependency(&repo, "v1.0.0");

        let commit = fetch("example", &dep, &source).unwrap();

        assert_eq!(commit, expected_commit);
        assert_eq!(commit.len(), 40);
        assert!(source.join(".hangar-fetched").is_file());

        assert_eq!(
            fs::read_to_string(source.join("hello.txt")).unwrap() .replace("\r\n", "\n"),
            "hello from Hangar\n"
        );
    }

    #[test]
    fn reuses_existing_clone_when_marker_exists() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        create_local_repo(&repo);

        let source = temp.path().join("cache").join("example");
        let dep = test_dependency(&repo, "v1.0.0");

        let first_commit = fetch("example", &dep, &source).unwrap();

        // This file is not in the Git repository. If fetch reclones the
        // source, this sentinel file will disappear.
        fs::write(source.join("sentinel.txt"), "keep me\n").unwrap();

        let second_commit = fetch("example", &dep, &source).unwrap();

        assert_eq!(second_commit, first_commit);
        assert_eq!(
            fs::read_to_string(source.join("sentinel.txt")).unwrap(),
            "keep me\n"
        );
    }

    #[test]
    fn removes_incomplete_clone_and_fetches_again() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        create_local_repo(&repo);

        let source = temp.path().join("cache").join("example");
        fs::create_dir_all(&source).unwrap();

        fs::write(source.join("partial.txt"), "incomplete clone").unwrap();

        assert!(!source.join(".hangar-fetched").exists());

        let dep = test_dependency(&repo, "v1.0.0");
        let commit = fetch("example", &dep, &source).unwrap();

        assert_eq!(commit.len(), 40);
        assert!(!source.join("partial.txt").exists());
        assert!(source.join("hello.txt").is_file());
        assert!(source.join(".hangar-fetched").is_file());
    }

    #[test]
    fn invalid_tag_returns_error_and_cleans_clone_destination() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        create_local_repo(&repo);

        let source = temp.path().join("cache").join("example");
        let dep = test_dependency(&repo, "tag-that-does-not-exist");

        let result = fetch("example", &dep, &source);

        assert!(result.is_err());
        assert!(!source.exists());
    }
}