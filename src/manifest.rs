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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn loads_valid_manifest() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(
            &path,
            r#"
                [package]
                name = "my-game"
                version = "1.0.0"

                [dependencies.sdl]
                git = "https://example.com/sdl.git"
                tag = "release-1"
            "#,
        )
        .unwrap();

        let manifest = Manifest::load(&path).unwrap();

        assert_eq!(manifest.package.name, "my-game");
        assert_eq!(manifest.package.version, "1.0.0");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies["sdl"].tag, "release-1");
    }

    #[test]
    fn missing_dependencies_default_to_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(
            &path,
            r#"
                [package]
                name = "empty-project"
                version = "0.1.0"
            "#,
        )
        .unwrap();

        let manifest = Manifest::load(&path).unwrap();

        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn missing_manifest_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("does-not-exist.toml");

        let result = Manifest::load(&path);

        assert!(result.is_err());
        assert!(
            //format!("{:#}", result.unwrap_err())
            format!("{:#}", result.err().unwrap())
                .contains("cannot read manifest")
        );
    }

    #[test]
    fn invalid_toml_returns_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(&path, "this is not valid toml = = =").unwrap();

        let result = Manifest::load(&path);

        assert!(result.is_err());
        assert!(
            format!("{:#}", result.unwrap_err())
                .contains("invalid manifest")
        );
    }

    #[test]
    fn optional_dependency_fields_default_to_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(
            &path,
            r#"
                [package]
                name = "my-game"
                version = "1.0.0"

                [dependencies.headerlib]
                git = "https://example.com/headerlib.git"
                tag = "v1.0.0"
            "#,
        )
        .unwrap();

        let manifest = Manifest::load(&path).unwrap();
        let dep = &manifest.dependencies["headerlib"];

        assert!(dep.options.is_empty());
        assert!(dep.headers.is_empty());
    }
}