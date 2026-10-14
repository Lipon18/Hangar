use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub package: Package,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub git: String,
    pub tag: String,
    /// Extra CMake options, e.g. "SDL_TESTS=OFF"
    #[serde(default)]
    pub options: Vec<String>,
    /// Header-only: paths (relative to the repo root) copied to <prefix>/include
    #[serde(default)]
    pub headers: Vec<String>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read manifest {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("invalid manifest {}", path.display()))
    }
}