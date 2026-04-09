pub mod cli;

use std::{
    fs,
    path::{Path, PathBuf},
};

use cli::{
    CheckArgs, Cli, Command, ConfigSubcommand, GenerateArgs, InitArgs, LocateArgs, PrintArgs,
};
use numi_config::CONFIG_FILE_NAME;

const STARTER_CONFIG: &str = include_str!("../../../docs/examples/starter-swiftgen.toml");

#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    fn with_exit_code(message: impl Into<String>, exit_code: i32) -> Self {
        Self {
            message: message.into(),
            exit_code,
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

pub fn run(cli: Cli) -> Result<(), CliError> {
    let command = cli
        .command
        .ok_or_else(|| CliError::new("a subcommand is required"))?;

    match command {
        Command::Generate(args) => run_generate(&args),
        Command::Check(args) => run_check(&args),
        Command::Init(args) => run_init(&args),
        Command::Config(config) => match config.command {
            ConfigSubcommand::Locate(args) => run_config_locate(&args),
            ConfigSubcommand::Print(args) => run_config_print(&args),
        },
        Command::DumpContext(args) => {
            let config_path = discover_config_path(args.config.as_deref())?;
            let report = numi_core::dump_context(&config_path, &args.job)
                .map_err(|error| CliError::new(error.to_string()))?;
            print_warnings(&report.warnings);
            println!("{}", report.json);
            Ok(())
        }
    }
}

fn run_generate(args: &GenerateArgs) -> Result<(), CliError> {
    let config_path = discover_config_path(args.config.as_deref())?;
    let selected_jobs = selected_jobs(&args.jobs);
    let report = numi_core::generate(&config_path, selected_jobs)
        .map_err(|error| CliError::new(error.to_string()))?;
    print_warnings(&report.warnings);
    Ok(())
}

fn run_check(args: &CheckArgs) -> Result<(), CliError> {
    let config_path = discover_config_path(args.config.as_deref())?;
    let selected_jobs = selected_jobs(&args.jobs);

    let report = numi_core::check(&config_path, selected_jobs)
        .map_err(|error| CliError::new(error.to_string()))?;
    print_warnings(&report.warnings);

    if report.stale_paths.is_empty() {
        Ok(())
    } else {
        let lines = report
            .stale_paths
            .iter()
            .map(display_path)
            .collect::<Vec<_>>()
            .join("\n");
        Err(CliError::with_exit_code(
            format!("stale generated outputs:\n{lines}"),
            2,
        ))
    }
}

fn run_init(args: &InitArgs) -> Result<(), CliError> {
    let cwd = current_dir()?;
    let config_path = cwd.join(CONFIG_FILE_NAME);

    if config_path.exists() && !args.force {
        return Err(CliError::new(format!(
            "{CONFIG_FILE_NAME} already exists; pass --force to overwrite"
        )));
    }

    fs::write(&config_path, STARTER_CONFIG).map_err(|error| {
        CliError::new(format!(
            "failed to write starter config {}: {error}",
            config_path.display()
        ))
    })?;

    Ok(())
}

fn run_config_locate(args: &LocateArgs) -> Result<(), CliError> {
    let config_path = discover_config_path(args.config.as_deref())?;
    println!("{}", display_path(&config_path));
    Ok(())
}

fn run_config_print(args: &PrintArgs) -> Result<(), CliError> {
    let config_path = discover_config_path(args.config.as_deref())?;
    let loaded = numi_config::load_from_path(&config_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let resolved = numi_config::resolve_config(&loaded.config);
    let rendered = toml::to_string_pretty(&resolved)
        .map_err(|error| CliError::new(format!("failed to serialize config TOML: {error}")))?;
    print!("{rendered}");
    Ok(())
}

fn current_dir() -> Result<PathBuf, CliError> {
    std::env::current_dir().map_err(|error| CliError::new(format!("failed to read cwd: {error}")))
}

fn discover_config_path(explicit_path: Option<&Path>) -> Result<PathBuf, CliError> {
    let cwd = current_dir()?;
    numi_config::discover_config(&cwd, explicit_path)
        .map_err(|error| CliError::new(error.to_string()))
}

fn selected_jobs(jobs: &[String]) -> Option<&[String]> {
    (!jobs.is_empty()).then_some(jobs)
}

fn print_warnings<T: std::fmt::Display>(warnings: &[T]) {
    for warning in warnings {
        eprintln!("{warning}");
    }
}

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
