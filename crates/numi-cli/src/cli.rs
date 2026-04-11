use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "numi",
    version,
    about = "Generate Swift code from Apple project resources",
    long_about = "Generate Swift code from asset catalogs, localization files, and other project resources.",
    before_help = "Generate Swift code from Apple project resources",
    after_help = "Examples:\n  numi init\n  numi generate\n  numi check\n  numi generate --workspace\n  numi dump-context --job l10n",
    propagate_version = true,
    subcommand_required = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(
        about = "Generate outputs for one config or workspace",
        after_help = "Examples:\n  numi generate\n  numi generate --job assets --job l10n\n  numi generate --workspace"
    )]
    Generate(GenerateArgs),
    #[command(
        about = "Check whether generated outputs are up to date",
        after_help = "Examples:\n  numi check\n  numi check --job l10n\n  numi check --workspace"
    )]
    Check(CheckArgs),
    #[command(about = "Write a starter numi.toml in the current directory")]
    Init(InitArgs),
    #[command(about = "Inspect resolved config paths and values")]
    Config(ConfigCommand),
    #[command(
        name = "dump-context",
        about = "Print the template context for a single job",
        after_help = "Examples:\n  numi dump-context --job l10n\n  numi dump-context --config AppUI/numi.toml --job assets"
    )]
    DumpContext(DumpContextArgs),
}

#[derive(Debug, Args)]
#[command(about = "Inspect resolved config paths and values")]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub command: ConfigSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigSubcommand {
    #[command(about = "Print the resolved config path")]
    Locate(LocateArgs),
    #[command(about = "Print the resolved config with defaults applied")]
    Print(PrintArgs),
}

#[derive(Debug, Args)]
pub struct LocateArgs {
    #[arg(
        long = "config",
        help = "Use a specific numi.toml instead of auto-discovery"
    )]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct GenerateArgs {
    #[arg(
        long = "config",
        help = "Use a specific numi.toml instead of auto-discovery"
    )]
    pub config: Option<PathBuf>,
    #[arg(
        long = "workspace",
        action = ArgAction::SetTrue,
        help = "Use the ancestor workspace manifest instead of the nearest member manifest"
    )]
    pub workspace: bool,
    #[arg(long = "job", help = "Limit generation to the selected job name")]
    pub jobs: Vec<String>,
    #[command(flatten)]
    pub incremental_override: IncrementalOverrideArgs,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(
        long = "config",
        help = "Use a specific numi.toml instead of auto-discovery"
    )]
    pub config: Option<PathBuf>,
    #[arg(
        long = "workspace",
        action = ArgAction::SetTrue,
        help = "Use the ancestor workspace manifest instead of the nearest member manifest"
    )]
    pub workspace: bool,
    #[arg(long = "job", help = "Limit checking to the selected job name")]
    pub jobs: Vec<String>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long, help = "Overwrite an existing numi.toml in the current directory")]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct PrintArgs {
    #[arg(
        long = "config",
        help = "Use a specific numi.toml instead of auto-discovery"
    )]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DumpContextArgs {
    #[arg(
        long = "config",
        help = "Use a specific numi.toml instead of auto-discovery"
    )]
    pub config: Option<PathBuf>,
    #[arg(long = "job", help = "Job name to render as JSON context")]
    pub job: String,
}

#[derive(Debug, Args, Default, Clone, PartialEq, Eq)]
pub struct IncrementalOverrideArgs {
    #[arg(
        long = "incremental",
        action = ArgAction::SetTrue,
        help = "Force incremental parsing when supported",
        conflicts_with = "no_incremental"
    )]
    pub incremental: bool,
    #[arg(
        long = "no-incremental",
        action = ArgAction::SetTrue,
        help = "Disable incremental parsing even when the config enables it"
    )]
    pub no_incremental: bool,
}

impl IncrementalOverrideArgs {
    pub fn resolve(&self) -> Option<bool> {
        if self.incremental {
            Some(true)
        } else if self.no_incremental {
            Some(false)
        } else {
            None
        }
    }
}
