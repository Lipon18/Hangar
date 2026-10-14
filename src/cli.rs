use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "hanger", version, about = "Registry-free C/C++ package manager")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Fetch build and install all dependencies
    Install {
        /// Path to the project manifest
        #[arg(long, default_value = "hanger.toml")]
        manifest: PathBuf,

        /// Path to the platform profile
        #[arg(long)]
        profile: PathBuf,

        /// Write a CMake file that points find_package() at the prefix
        #[arg(long)]
        cmake_out: Option<PathBuf>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_install_command() {
        let cli = Cli::try_parse_from([
            "hangar",
            "install",
            "--profile",
            "profiles/linux.toml",
        ])
        .unwrap();

        match cli.cmd {
            Cmd::Install {
                manifest,
                profile,
                cmake_out,
            } => {
                assert_eq!(manifest, PathBuf::from("hanger.toml"));
                assert_eq!(profile, PathBuf::from("profiles/linux.toml"));
                assert_eq!(cmake_out, None);
            }
        }
    }

    #[test]
    fn parses_custom_manifest_and_cmake_output() {
        let cli = Cli::try_parse_from([
            "hangar",
            "install",
            "--manifest",
            "project.toml",
            "--profile",
            "linux.toml",
            "--cmake-out",
            "build/hangar-deps.cmake",
        ])
        .unwrap();

        match cli.cmd {
            Cmd::Install {
                manifest,
                profile,
                cmake_out,
            } => {
                assert_eq!(manifest, PathBuf::from("project.toml"));
                assert_eq!(profile, PathBuf::from("linux.toml"));
                assert_eq!(
                    cmake_out,
                    Some(PathBuf::from("build/hangar-deps.cmake"))
                );
            }
        }
    }

    #[test]
    fn profile_argument_is_required() {
        let result = Cli::try_parse_from([
            "hangar",
            "install",
        ]);

        assert!(result.is_err());
    }
}