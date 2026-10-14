use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
struct ProfileFile {
    profile: Profile,
}

#[derive(Debug, Deserialize)]
pub struct Profile {
    pub name: String,
    pub os: String,
    pub compiler: String,
    pub build_type: String,
    pub dependencies_dir: String,
    /// Optional CMake generator, e.g. "Visual Studio 17 2022"
    pub generator: Option<String>,
    /// Optional platform passed to -A, e.g. "x64"
    pub arch: Option<String>,
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read profile {}", path.display()))?;
        let file: ProfileFile = toml::from_str(&text)
            .with_context(|| format!("invalid profile {}", path.display()))?;
        file.profile.validate()?;
        Ok(file.profile)
    }

    /// Folder name for this platform, e.g. "windows-x64-msvc2022-release"
    pub fn key(&self) -> String {
        format!(
            "{}-{}-{}-{}",
            self.os,
            self.arch.as_deref().unwrap_or("default"),
            self.compiler,
            self.build_type
        )
        .to_lowercase()
    }

    fn validate(&self) -> Result<()> {
        const VALID: [&str; 4] = ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"];
        if !VALID.contains(&self.build_type.as_str()) {
            bail!(
                "build_type '{}' is not valid, use one of: {}",
                self.build_type,
                VALID.join(", ")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn valid_profile_toml(build_type: &str) -> String {
        format!(
              r#"
                [profile]
                name = "Windows Release"
                os = "Windows"
                compiler = "MSVC2022"
                build_type = "{build_type}"
                dependencies_dir = "deps"
                arch = "x64"
            "#
        )
    }

    #[test]
    fn loads_valid_profile() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.toml");

        fs::write(&path, valid_profile_toml("Release")).unwrap();

        let profile = Profile::load(&path).unwrap();

        assert_eq!(profile.name, "Windows Release");
        assert_eq!(profile.os, "Windows");
        assert_eq!(profile.compiler, "MSVC2022");
        assert_eq!(profile.build_type, "Release");
        assert_eq!(profile.dependencies_dir, "deps");
        assert_eq!(profile.arch.as_deref(), Some("x64"));
    }

    #[test]
    fn generates_lowercase_profile_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.toml");

        fs::write(&path, valid_profile_toml("Release")).unwrap();

        let profile = Profile::load(&path).unwrap();

        assert_eq!(
            profile.key(),
            "windows-x64-msvc2022-release"
        );
    }

    #[test]
    fn rejects_invalid_build_type() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.toml");

        fs::write(&path, valid_profile_toml("FastBuild")).unwrap();

        let error = Profile::load(&path).unwrap_err();

        assert!(
            format!("{error:#}").contains("build_type 'FastBuild' is not valid")
        );
    }

    #[test]
    fn accepts_all_supported_build_types() {
        for build_type in [
            "Debug",
            "Release",
            "RelWithDebInfo",
            "MinSizeRel",
        ] {
            let dir = tempdir().unwrap();
            let path = dir.path().join("profile.toml");

            fs::write(&path, valid_profile_toml(build_type)).unwrap();

            assert!(
                Profile::load(&path).is_ok(),
                "should accept {build_type}"
            );
        }
    }

    #[test]
    fn missing_arch_uses_default_in_key() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.toml");

        let toml = r#"
            [profile]
            name = "Linux Debug"
            os = "Linux"
            compiler = "GCC"
            build_type = "Debug"
            dependencies_dir = "deps"
        "#;

        fs::write(&path, toml).unwrap();

        let profile = Profile::load(&path).unwrap();

        assert_eq!(profile.key(), "linux-default-gcc-debug");
        assert_eq!(profile.arch, None);
    }
}