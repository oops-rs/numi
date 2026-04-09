pub mod cli;

use std::{
    borrow::Cow,
    fs,
    path::{Path, PathBuf},
};

use cli::{
    CheckArgs, Cli, Command, ConfigSubcommand, GenerateArgs, InitArgs, LocateArgs, PrintArgs,
    WorkspaceCheckArgs, WorkspaceGenerateArgs, WorkspaceSubcommand,
};
use numi_config::{CONFIG_FILE_NAME, LoadedWorkspace, WorkspaceMember};

const STARTER_CONFIG_FALLBACK: &str = include_str!("../../../docs/examples/starter-numi.toml");

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
        Command::Workspace(workspace) => match workspace.command {
            WorkspaceSubcommand::Generate(args) => run_workspace_generate(&args),
            WorkspaceSubcommand::Check(args) => run_workspace_check(&args),
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
    let report = numi_core::generate_with_options(
        &config_path,
        selected_jobs,
        numi_core::GenerateOptions {
            incremental: args.incremental_override.resolve(),
        },
    )
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

    let starter_config = load_starter_config()?;
    fs::write(&config_path, starter_config.as_ref()).map_err(|error| {
        CliError::new(format!(
            "failed to write starter config {}: {error}",
            config_path.display()
        ))
    })?;

    Ok(())
}

fn run_workspace_generate(args: &WorkspaceGenerateArgs) -> Result<(), CliError> {
    let loaded = load_workspace_manifest(args.workspace.as_deref(), "generate")?;
    let workspace_dir = workspace_dir(&loaded)?;

    for member in select_workspace_members(&loaded, &args.members)? {
        let config_path = workspace_dir.join(&member.config);
        let report = numi_core::generate_with_options(
            &config_path,
            workspace_member_jobs(member),
            numi_core::GenerateOptions {
                incremental: args.incremental_override.resolve(),
            },
        )
        .map_err(|error| CliError::new(error.to_string()))?;
        print_warnings(&report.warnings);
    }

    Ok(())
}

fn run_workspace_check(args: &WorkspaceCheckArgs) -> Result<(), CliError> {
    let loaded = load_workspace_manifest(args.workspace.as_deref(), "check")?;
    let workspace_dir = workspace_dir(&loaded)?;
    let mut stale_paths = Vec::new();

    for member in select_workspace_members(&loaded, &args.members)? {
        let config_path = workspace_dir.join(&member.config);
        let report = numi_core::check(&config_path, workspace_member_jobs(member))
            .map_err(|error| CliError::new(error.to_string()))?;
        print_warnings(&report.warnings);
        stale_paths.extend(
            report
                .stale_paths
                .iter()
                .map(|path| normalize_workspace_stale_path(path.as_std_path(), &workspace_dir)),
        );
    }

    if stale_paths.is_empty() {
        Ok(())
    } else {
        let lines = stale_paths
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

fn load_starter_config() -> Result<Cow<'static, str>, CliError> {
    let preferred_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/starter-numi.toml");

    match fs::read_to_string(&preferred_path) {
        Ok(contents) => Ok(Cow::Owned(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Cow::Borrowed(STARTER_CONFIG_FALLBACK))
        }
        Err(error) => Err(CliError::new(format!(
            "failed to read starter config {}: {error}",
            preferred_path.display()
        ))),
    }
}

fn load_workspace_manifest(
    explicit_path: Option<&Path>,
    command_name: &str,
) -> Result<LoadedWorkspace, CliError> {
    let cwd = current_dir()?;
    let manifest_path = numi_config::discover_workspace(&cwd, explicit_path)
        .map_err(|error| CliError::new(workspace_discovery_message(error, command_name)))?;
    numi_config::load_workspace_from_path(&manifest_path)
        .map_err(|error| CliError::new(error.to_string()))
}

fn workspace_discovery_message(
    error: numi_config::WorkspaceDiscoveryError,
    command_name: &str,
) -> String {
    let guidance = format!("numi workspace {command_name} --workspace <path>");
    error
        .to_string()
        .replace("numi workspace locate --workspace <path>", &guidance)
}

fn workspace_dir(loaded: &LoadedWorkspace) -> Result<&Path, CliError> {
    loaded
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            CliError::new(format!(
                "workspace manifest {} has no parent directory",
                loaded.path.display()
            ))
        })
}

fn discover_config_path(explicit_path: Option<&Path>) -> Result<PathBuf, CliError> {
    let cwd = current_dir()?;
    numi_config::discover_config(&cwd, explicit_path)
        .map_err(|error| CliError::new(error.to_string()))
}

fn selected_jobs(jobs: &[String]) -> Option<&[String]> {
    (!jobs.is_empty()).then_some(jobs)
}

fn workspace_member_jobs(member: &WorkspaceMember) -> Option<&[String]> {
    (!member.jobs.is_empty()).then_some(member.jobs.as_slice())
}

fn select_workspace_members<'a>(
    loaded: &'a LoadedWorkspace,
    selected_members: &[String],
) -> Result<Vec<&'a WorkspaceMember>, CliError> {
    if selected_members.is_empty() {
        return Ok(loaded.config.members.iter().collect());
    }

    let mut missing = selected_members
        .iter()
        .filter(|selected| {
            !loaded
                .config
                .members
                .iter()
                .any(|member| member.config == **selected)
        })
        .cloned()
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        let valid_members = loaded
            .config
            .members
            .iter()
            .map(|member| format!("  - {}", member.config))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(CliError::new(format!(
            "unknown workspace member selection(s): {}\nvalid workspace members:\n{}",
            missing.join(", "),
            valid_members
        )));
    }

    Ok(loaded
        .config
        .members
        .iter()
        .filter(|member| {
            selected_members
                .iter()
                .any(|selected| selected == &member.config)
        })
        .collect())
}

fn normalize_workspace_stale_path(path: &Path, workspace_dir: &Path) -> PathBuf {
    path.strip_prefix(workspace_dir)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn print_warnings<T: std::fmt::Display>(warnings: &[T]) {
    for warning in warnings {
        eprintln!("{warning}");
    }
}

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}
