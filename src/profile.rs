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