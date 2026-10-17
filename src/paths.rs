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

    /// CMake build folder of one dependency for this platform and build type.
    /// Every build type gets its own folder so single-config generators
    /// (Ninja, Makefiles) work as well as Visual Studio.
    pub fn build(&self, name: &str, tag: &str, build_type: &str) -> PathBuf {
        self.builds
            .join(name)
            .join(safe(tag))
            .join(safe(build_type))
    }

    /// Install prefix of one dependency for this platform.
    /// All build types of the platform are installed into this one prefix.
    pub fn export(&self, name: &str, tag: &str) -> PathBuf {
        self.exports.join(name).join(safe(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;

    fn test_profile(root: &str, compiler: &str) -> Profile {
        Profile {
            name: "Test".to_string(),
            os: "Linux".to_string(),
            compiler: compiler.to_string(),
            build_types: vec!["Debug".to_string(), "Release".to_string()],
            dependencies_dir: root.to_string(),
            generator: None,
            arch: Some("x64".to_string()),
        }
    }

    #[test]
    fn source_path_is_shared_across_profiles() {
        let gcc = Paths::new(&test_profile("deps", "GCC"));
        let clang = Paths::new(&test_profile("deps", "Clang"));

        assert_eq!(
            gcc.source("sdl", "release/1.2"),
            clang.source("sdl", "release/1.2")
        );
    }

    #[test]
    fn build_path_differs_between_profiles() {
        let gcc = Paths::new(&test_profile("deps", "GCC"));
        let clang = Paths::new(&test_profile("deps", "Clang"));

        assert_ne!(
            gcc.build("sdl", "v1.2", "Release"),
            clang.build("sdl", "v1.2", "Release")
        );
    }

    #[test]
    fn export_path_differs_between_profiles() {
        let gcc = Paths::new(&test_profile("deps", "GCC"));
        let clang = Paths::new(&test_profile("deps", "Clang"));

        assert_ne!(
            gcc.export("sdl", "v1.2"),
            clang.export("sdl", "v1.2")
        );
    }

    #[test]
    fn build_path_differs_between_build_types() {
        let paths = Paths::new(&test_profile("deps", "GCC"));

        assert_ne!(
            paths.build("sdl", "v1.2", "Debug"),
            paths.build("sdl", "v1.2", "Release")
        );

        assert_eq!(
            paths.build("sdl", "v1.2", "Debug"),
            PathBuf::from("deps/builds/linux-x64-gcc/sdl/v1.2/Debug")
        );
    }

    #[test]
    fn export_path_is_shared_by_all_build_types() {
        let paths = Paths::new(&test_profile("deps", "GCC"));

        assert_eq!(
            paths.export("sdl", "v1.2"),
            PathBuf::from("deps/exports/linux-x64-gcc/sdl/v1.2")
        );
    }

    #[test]
    fn tag_slashes_do_not_create_nested_directories() {
        let paths = Paths::new(&test_profile("deps", "GCC"));

        let source = paths.source("sdl", "release/1.2");

        assert_eq!(
            source,
            PathBuf::from("deps/sources/sdl/release_1.2")
        );
    }

    #[test]
    fn backslashes_are_replaced_in_tags() {
        let paths = Paths::new(&test_profile("deps", "GCC"));

        assert_eq!(
            paths.source("sdl", r"release\1.2"),
            PathBuf::from("deps/sources/sdl/release_1.2")
        );
    }
}