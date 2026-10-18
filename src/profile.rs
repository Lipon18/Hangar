use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Deserialize)]
struct ProfileFile {
    profile: Profile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub os: String,
    pub compiler: String,
    /// Build types to build and install into the same prefix,
    /// e.g. ["Debug", "Release"]
    #[serde(default)]
    pub build_types: Vec<String>,
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

    /// Folder name for this platform, e.g. "windows-x64-msvc2022".
    ///
    /// The build type is not part of the key: all build types of a
    /// platform share one install prefix.
    pub fn key(&self) -> String {
        format!(
            "{}-{}-{}",
            self.os,
            self.arch.as_deref().unwrap_or("default"),
            self.compiler,
        )
        .to_lowercase()
    }

    /// Profiles that are installed together (several `--profile` flags)
    /// must use the same install prefix, so that their build types end up
    /// next to each other and CMake sees them as one package.
    pub fn ensure_same_prefix(&self, other: &Profile) -> Result<()> {
        if self.key() != other.key() || self.dependencies_dir != other.dependencies_dir {
            bail!(
                "profiles '{}' and '{}' do not share an install prefix: \
                 os, arch, compiler and dependencies_dir must be identical",
                self.name,
                other.name
            );
        }

        Ok(())
    }

    fn validate(&self) -> Result<()> {
        const VALID: [&str; 4] = ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"];

        if self.build_types.is_empty() {
            bail!(
                "build_types must list at least one build type, \
                 e.g. build_types = [\"Debug\", \"Release\"]"
            );
        }

        for (i, build_type) in self.build_types.iter().enumerate() {
            if !VALID.contains(&build_type.as_str()) {
                bail!(
                    "build_type '{}' is not valid, use one of: {}",
                    build_type,
                    VALID.join(", ")
                );
            }

            if self.build_types[..i].contains(build_type) {
                bail!("build_types lists '{}' more than once", build_type);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn valid_profile_toml(build_types: &[&str]) -> String {
        let list = build_types
            .iter()
            .map(|b| format!("\"{b}\""))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r#"
                [profile]
                name = "Windows"
                os = "Windows"
                compiler = "MSVC2022"
                build_types = [{list}]
                dependencies_dir = "deps"
                arch = "x64"
            "#
        )
    }

    fn load(toml: &str) -> Result<Profile> {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profile.toml");

        fs::write(&path, toml).unwrap();

        Profile::load(&path)
    }

    fn profile(name: &str, compiler: &str, build_type: &str, dir: &str) -> Profile {
        Profile {
            name: name.to_string(),
            os: "Windows".to_string(),
            compiler: compiler.to_string(),
            build_types: vec![build_type.to_string()],
            dependencies_dir: dir.to_string(),
            generator: None,
            arch: Some("x64".to_string()),
        }
    }

    #[test]
    fn loads_valid_profile() {
        let profile = load(&valid_profile_toml(&["Debug", "Release"])).unwrap();

        assert_eq!(profile.name, "Windows");
        assert_eq!(profile.os, "Windows");
        assert_eq!(profile.compiler, "MSVC2022");
        assert_eq!(profile.build_types, vec!["Debug", "Release"]);
        assert_eq!(profile.dependencies_dir, "deps");
        assert_eq!(profile.arch.as_deref(), Some("x64"));
    }

    #[test]
    fn generates_lowercase_profile_key_without_build_type() {
        let profile = load(&valid_profile_toml(&["Debug", "Release"])).unwrap();

        assert_eq!(profile.key(), "windows-x64-msvc2022");
    }

    #[test]
    fn key_is_the_same_for_every_build_type_list() {
        let both = load(&valid_profile_toml(&["Debug", "Release"])).unwrap();
        let release = load(&valid_profile_toml(&["Release"])).unwrap();

        assert_eq!(both.key(), release.key());
    }

    #[test]
    fn rejects_invalid_build_type() {
        let error = load(&valid_profile_toml(&["Release", "FastBuild"])).unwrap_err();

        assert!(
            format!("{error:#}").contains("build_type 'FastBuild' is not valid")
        );
    }

    #[test]
    fn rejects_empty_build_types() {
        let error = load(&valid_profile_toml(&[])).unwrap_err();

        assert!(
            format!("{error:#}").contains("build_types must list at least one build type")
        );
    }

    #[test]
    fn rejects_missing_build_types() {
        let toml = r#"
            [profile]
            name = "Windows"
            os = "Windows"
            compiler = "MSVC2022"
            dependencies_dir = "deps"
        "#;

        let error = load(toml).unwrap_err();

        assert!(
            format!("{error:#}").contains("build_types must list at least one build type")
        );
    }

    #[test]
    fn rejects_duplicate_build_types() {
        let error = load(&valid_profile_toml(&["Release", "Debug", "Release"])).unwrap_err();

        assert!(
            format!("{error:#}").contains("build_types lists 'Release' more than once")
        );
    }

    #[test]
    fn accepts_all_supported_build_types() {
        let profile = load(&valid_profile_toml(&[
            "Debug",
            "Release",
            "RelWithDebInfo",
            "MinSizeRel",
        ]))
        .unwrap();

        assert_eq!(profile.build_types.len(), 4);
    }

    #[test]
    fn accepts_each_supported_build_type_alone() {
        for build_type in ["Debug", "Release", "RelWithDebInfo", "MinSizeRel"] {
            assert!(
                load(&valid_profile_toml(&[build_type])).is_ok(),
                "should accept {build_type}"
            );
        }
    }

    #[test]
    fn missing_arch_uses_default_in_key() {
        let toml = r#"
            [profile]
            name = "Linux Debug"
            os = "Linux"
            compiler = "GCC"
            build_types = ["Debug"]
            dependencies_dir = "deps"
        "#;

        let profile = load(toml).unwrap();

        assert_eq!(profile.key(), "linux-default-gcc");
        assert_eq!(profile.arch, None);
    }

    #[test]
    fn reject_unknown_profile_keys() {
        let toml = r#"
            [profile]
            name = "Windows"
            os = "Windows"
            compiler = "MSVC2022"
            build_type = "Release"
            build_types = ["Release"]
            dependencies_dir = "deps"
        "#;

        let error = load(toml).unwrap_err();

        assert!(format!("{error:#}").contains("unknown field `build_type`"));
    }

    // Several profiles installed together

    #[test]
    fn debug_and_release_profiles_share_a_prefix() {
        let debug = profile("windows-msvc-debug", "MSVC2022", "Debug", "C:/hangar");
        let release = profile("windows-msvc-release", "MSVC2022", "Release", "C:/hangar");

        // Different names and build types are fine.
        assert!(debug.ensure_same_prefix(&release).is_ok());
        assert!(release.ensure_same_prefix(&debug).is_ok());
    }

    #[test]
    fn profiles_with_different_compilers_do_not_share_a_prefix() {
        let msvc = profile("msvc", "MSVC2022", "Debug", "C:/hangar");
        let clang = profile("clang", "Clang", "Release", "C:/hangar");

        let error = msvc.ensure_same_prefix(&clang).unwrap_err();

        assert!(
            format!("{error:#}").contains("profiles 'msvc' and 'clang' do not share an install prefix")
        );
    }

    #[test]
    fn profiles_with_different_dependencies_dirs_do_not_share_a_prefix() {
        let first = profile("first", "MSVC2022", "Debug", "C:/hangar");
        let second = profile("second", "MSVC2022", "Release", "D:/other");

        assert!(first.ensure_same_prefix(&second).is_err());
    }
}