use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "numi",
    version,
    about = "CLI for numi",
    propagate_version = true,
    subcommand_required = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Generate(GenerateArgs),
    Check(CheckArgs),
    Init(InitArgs),
    Config(ConfigCommand),
    #[command(name = "dump-context")]
    DumpContext(DumpContextArgs),
}

#[derive(Debug, Args)]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    Locate(LocateArgs),
    Print(PrintArgs),
}

#[derive(Debug, Args)]
pub struct LocateArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct GenerateArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
    #[arg(long = "job")]
    pub jobs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
    #[arg(long = "job")]
    pub jobs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PrintArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DumpContextArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
    #[arg(long = "job")]
    pub job: String,
}
