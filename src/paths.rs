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