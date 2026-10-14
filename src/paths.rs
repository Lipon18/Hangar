use crate::profile::Profile;
use std::path::PathBuf;

pub struct Paths {
    pub root: PathBuf,
    sources: PathBuf,
    builds: PathBuf,
    exports: PathBuf,
}

/// Tags like "release/1.2" must not create nested folders by accident.
fn safe(s: &str) -> String {
    s.replace(['/', '\\'], "_")
}

impl Paths {
    pub fn new(p: &Profile) -> Self {
        let root = PathBuf::from(&p.dependencies_dir);
        let key = p.key();
        Self {
            sources: root.join("sources"),
            builds: root.join("builds").join(&key),
            exports: root.join("exports").join(&key),
            root,
        }
    }

    /// Git clone of one dependency (shared by all profiles)
    pub fn source(&self, name: &str, tag: &str) -> PathBuf {
        self.sources.join(name).join(safe(tag))
    }

    /// CMake build folder of one dependency for this profile
    pub fn build(&self, name: &str, tag: &str) -> PathBuf {
        self.builds.join(name).join(safe(tag))
    }

    /// Install prefix of one dependency for this profile
    pub fn export(&self, name: &str, tag: &str) -> PathBuf {
        self.exports.join(name).join(safe(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;

    fn test_profile(root: &str, build_type: &str) -> Profile {
        Profile {
            name: "Test".to_string(),
            os: "Linux".to_string(),
            compiler: "GCC".to_string(),
            build_type: build_type.to_string(),
            dependencies_dir: root.to_string(),
            generator: None,
            arch: Some("x64".to_string()),
        }
    }

    #[test]
    fn source_path_is_shared_across_profiles() {
        let debug = Paths::new(&test_profile("deps", "Debug"));
        let release = Paths::new(&test_profile("deps", "Release"));

        assert_eq!(
            debug.source("sdl", "release/1.2"),
            release.source("sdl", "release/1.2")
        );
    }

    #[test]
    fn build_path_differs_between_profiles() {
        let debug = Paths::new(&test_profile("deps", "Debug"));
        let release = Paths::new(&test_profile("deps", "Release"));

        assert_ne!(
            debug.build("sdl", "v1.2"),
            release.build("sdl", "v1.2")
        );
    }

    #[test]
    fn export_path_differs_between_profiles() {
        let debug = Paths::new(&test_profile("deps", "Debug"));
        let release = Paths::new(&test_profile("deps", "Release"));

        assert_ne!(
            debug.export("sdl", "v1.2"),
            release.export("sdl", "v1.2")
        );
    }

    #[test]
    fn tag_slashes_do_not_create_nested_directories() {
        let paths = Paths::new(&test_profile("deps", "Release"));

        let source = paths.source("sdl", "release/1.2");

        assert_eq!(
            source,
            PathBuf::from("deps/sources/sdl/release_1.2")
        );
    }

    #[test]
    fn backslashes_are_replaced_in_tags() {
        let paths = Paths::new(&test_profile("deps", "Release"));

        assert_eq!(
            paths.source("sdl", r"release\1.2"),
            PathBuf::from("deps/sources/sdl/release_1.2")
        );
    }
}