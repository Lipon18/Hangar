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