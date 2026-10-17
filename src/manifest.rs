use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

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
    /// Folder with a CMakeLists.txt that builds and installs this dependency
    /// instead of the repository's own build. It receives the location of
    /// the git clone as HANGAR_SOURCE_DIR. In the manifest the path is
    /// relative to the manifest file; `Manifest::load` resolves it.
    #[serde(default)]
    pub overlay: Option<PathBuf>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read manifest {}", path.display()))?;

        let mut manifest: Manifest = toml::from_str(&text)
            .with_context(|| format!("invalid manifest {}", path.display()))?;

        let base = path.parent().unwrap_or_else(|| Path::new(""));

        for (name, dep) in &mut manifest.dependencies {
            if let Some(overlay) = dep.overlay.take() {
                if !dep.headers.is_empty() {
                    bail!(
                        "dependency '{name}': 'overlay' cannot be combined with 'headers'"
                    );
                }

                dep.overlay = Some(if overlay.is_relative() {
                    base.join(overlay)
                } else {
                    overlay
                });
            }
        }

        Ok(manifest)
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
            format!("{:#}", result.unwrap_err())
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
        assert!(dep.overlay.is_none());
    }

    #[test]
    fn relative_overlay_is_resolved_against_the_manifest_folder() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(
            &path,
            r#"
                [package]
                name = "my-game"
                version = "1.0.0"

                [dependencies.lua]
                git = "https://example.com/lua.git"
                tag = "v5.3.5"
                overlay = "Overlays/lua"
            "#,
        )
        .unwrap();

        let manifest = Manifest::load(&path).unwrap();

        assert_eq!(
            manifest.dependencies["lua"].overlay,
            Some(dir.path().join("Overlays/lua"))
        );
    }

    #[test]
    fn absolute_overlay_is_kept_as_it_is() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");
        let absolute = dir.path().join("somewhere").join("lua");

        // A TOML literal string (single quotes) keeps Windows backslashes.
        fs::write(
            &path,
            format!(
                r#"
                    [package]
                    name = "my-game"
                    version = "1.0.0"

                    [dependencies.lua]
                    git = "https://example.com/lua.git"
                    tag = "v5.3.5"
                    overlay = '{}'
                "#,
                absolute.display()
            ),
        )
        .unwrap();

        let manifest = Manifest::load(&path).unwrap();

        assert_eq!(manifest.dependencies["lua"].overlay, Some(absolute));
    }

    #[test]
    fn overlay_cannot_be_combined_with_headers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hangar.toml");

        fs::write(
            &path,
            r#"
                [package]
                name = "my-game"
                version = "1.0.0"

                [dependencies.lua]
                git = "https://example.com/lua.git"
                tag = "v5.3.5"
                headers = ["include"]
                overlay = "Overlays/lua"
            "#,
        )
        .unwrap();

        let error = Manifest::load(&path).unwrap_err();

        assert!(
            format!("{error:#}")
                .contains("dependency 'lua': 'overlay' cannot be combined with 'headers'")
        );
    }
}