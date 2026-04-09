pub mod cli;

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use cli::{
    CheckArgs, Cli, Command, ConfigSubcommand, DumpContextArgs, GenerateArgs, InitArgs, LocateArgs,
    PrintArgs,
};
use numi_config::{
    CONFIG_FILE_NAME, Config, LoadedManifest, Manifest, ManifestKindSniff, WorkspaceConfig,
    WorkspaceMember, resolve_workspace_member_config, workspace_member_config_path,
};

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
        Command::DumpContext(args) => run_dump_context(&args),
    }
}

fn run_generate(args: &GenerateArgs) -> Result<(), CliError> {
    let loaded = load_cli_manifest(args.config.as_deref(), args.workspace)?;
    match &loaded.manifest {
        Manifest::Config(config) => run_generate_config(&loaded.path, config, args),
        Manifest::Workspace(workspace) => run_generate_workspace(&loaded.path, workspace, args),
    }
}

fn run_generate_config(
    config_path: &Path,
    _config: &Config,
    args: &GenerateArgs,
) -> Result<(), CliError> {
    let selected_jobs = selected_jobs(&args.jobs);
    let report = numi_core::generate_with_options(
        config_path,
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
    let loaded = load_cli_manifest(args.config.as_deref(), args.workspace)?;
    match &loaded.manifest {
        Manifest::Config(config) => run_check_config(&loaded.path, config, args),
        Manifest::Workspace(workspace) => run_check_workspace(&loaded.path, workspace, args),
    }
}

fn run_check_config(
    config_path: &Path,
    _config: &Config,
    args: &CheckArgs,
) -> Result<(), CliError> {
    let selected_jobs = selected_jobs(&args.jobs);

    let report = numi_core::check(config_path, selected_jobs)
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

fn run_dump_context(args: &DumpContextArgs) -> Result<(), CliError> {
    let loaded = load_cli_manifest(args.config.as_deref(), false)?;
    match &loaded.manifest {
        Manifest::Config(_) => {
            let report = numi_core::dump_context(&loaded.path, &args.job)
                .map_err(|error| CliError::new(error.to_string()))?;
            print_warnings(&report.warnings);
            println!("{}", report.json);
            Ok(())
        }
        Manifest::Workspace(_) => Err(CliError::new(
            "`dump-context` only supports single-config manifests; run it from a member directory or pass `--config <member>/numi.toml`",
        )),
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

fn run_generate_workspace(
    manifest_path: &Path,
    workspace: &WorkspaceConfig,
    args: &GenerateArgs,
) -> Result<(), CliError> {
    let workspace_dir = manifest_dir(manifest_path)?;

    for member in workspace.members() {
        let member_root = workspace_member_root(&member);
        let config_path = workspace_member_config_path(workspace_dir, &member_root);
        let loaded_member = numi_config::load_unvalidated_from_path(&config_path)
            .map_err(|error| CliError::new(error.to_string()))?;
        let merged_config =
            resolve_workspace_member_config(&workspace, &member_root, &loaded_member.config)
                .map_err(render_config_diagnostics)?;
        let selected_jobs = workspace_jobs(args, &member);
        let report = numi_core::generate_loaded_config(
            &config_path,
            &merged_config,
            selected_jobs.as_deref(),
            numi_core::GenerateOptions {
                incremental: args.incremental_override.resolve(),
            },
        )
        .map_err(|error| CliError::new(error.to_string()))?;
        print_warnings(&report.warnings);
    }

    Ok(())
}

fn run_check_workspace(
    manifest_path: &Path,
    workspace: &WorkspaceConfig,
    args: &CheckArgs,
) -> Result<(), CliError> {
    let workspace_dir = manifest_dir(manifest_path)?;
    let mut stale_paths = Vec::new();

    for member in workspace.members() {
        let member_root = workspace_member_root(&member);
        let config_path = workspace_member_config_path(workspace_dir, &member_root);
        let loaded_member = numi_config::load_unvalidated_from_path(&config_path)
            .map_err(|error| CliError::new(error.to_string()))?;
        let merged_config =
            resolve_workspace_member_config(&workspace, &member_root, &loaded_member.config)
                .map_err(render_config_diagnostics)?;
        let selected_jobs = workspace_jobs(args, &member);
        let report =
            numi_core::check_loaded_config(&config_path, &merged_config, selected_jobs.as_deref())
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
    let loaded = load_cli_manifest(args.config.as_deref(), false)?;
    match &loaded.manifest {
        Manifest::Config(config) => {
            let resolved = numi_config::resolve_config(config);
            let rendered = toml::to_string_pretty(&resolved).map_err(|error| {
                CliError::new(format!("failed to serialize config TOML: {error}"))
            })?;
            print!("{rendered}");
            Ok(())
        }
        Manifest::Workspace(_) => Err(CliError::new(
            "`config print` only supports single-config manifests; run it from a member directory or pass `--config <member>/numi.toml`",
        )),
    }
}

fn load_cli_manifest(
    explicit_path: Option<&Path>,
    workspace: bool,
) -> Result<LoadedManifest, CliError> {
    if workspace {
        return load_workspace_cli_manifest(explicit_path);
    }

    let cwd = current_dir()?;
    let manifest_path = numi_config::discover_config(&cwd, explicit_path)
        .map_err(|error| CliError::new(error.to_string()))?;

    numi_config::load_manifest_from_path(&manifest_path)
        .map_err(|error| CliError::new(error.to_string()))
}

fn load_workspace_cli_manifest(explicit_path: Option<&Path>) -> Result<LoadedManifest, CliError> {
    let cwd = current_dir()?;

    if let Some(explicit_path) = explicit_path {
        let manifest_path = numi_config::discover_workspace_ancestor(&cwd, Some(explicit_path))
            .map_err(workspace_manifest_discovery_error)?;
        return load_workspace_manifest_candidate(&manifest_path);
    }

    let canonical_cwd = cwd
        .canonicalize()
        .map_err(|error| CliError::new(format!("failed to read cwd: {error}")))?;

    for directory in canonical_cwd.ancestors() {
        let candidate = directory.join(CONFIG_FILE_NAME);
        if !candidate.is_file() {
            continue;
        }

        match numi_config::sniff_manifest_kind_from_path(&candidate).map_err(|error| {
            CliError::new(format!(
                "failed to read manifest {}: {error}",
                candidate.display()
            ))
        })? {
            ManifestKindSniff::WorkspaceLike
            | ManifestKindSniff::BrokenWorkspaceLike
            | ManifestKindSniff::Mixed => {
                return load_workspace_manifest_candidate(&candidate);
            }
            ManifestKindSniff::ConfigLike
            | ManifestKindSniff::Unknown
            | ManifestKindSniff::Unparsable => continue,
        }
    }

    Err(workspace_manifest_discovery_error(
        numi_config::DiscoveryError::NotFound {
            start_dir: canonical_cwd,
        },
    ))
}

fn require_workspace_manifest(loaded: LoadedManifest) -> Result<LoadedManifest, CliError> {
    match loaded.manifest {
        Manifest::Workspace(_) => Ok(loaded),
        Manifest::Config(_) => Err(CliError::new(format!(
            "expected a workspace manifest at {}; pass --config <workspace>/numi.toml or remove --workspace",
            loaded.path.display()
        ))),
    }
}

fn load_workspace_manifest_candidate(path: &Path) -> Result<LoadedManifest, CliError> {
    let loaded = numi_config::load_manifest_from_path(path).map_err(|error| {
        CliError::new(format!(
            "failed to load workspace manifest {}: {error}",
            path.display()
        ))
    })?;
    require_workspace_manifest(loaded)
}

fn workspace_manifest_discovery_error(error: numi_config::DiscoveryError) -> CliError {
    match error {
        numi_config::DiscoveryError::ExplicitPathNotFound(path) => CliError::new(format!(
            "workspace manifest not found: {}\n\npass --config <workspace>/numi.toml or remove --workspace",
            path.display()
        )),
        numi_config::DiscoveryError::NotFound { start_dir } => CliError::new(format!(
            "No workspace manifest found from {}\n\nRun this from a workspace member directory with an ancestor numi.toml, or pass --config <workspace>/numi.toml",
            start_dir.display()
        )),
        numi_config::DiscoveryError::Ambiguous { root, matches } => {
            let lines = matches
                .iter()
                .map(|path| format!("  - {}", path.display()))
                .collect::<Vec<_>>()
                .join("\n");
            CliError::new(format!(
                "Multiple workspace manifests found under {}:\n{}\n\npass --config <workspace>/numi.toml",
                root.display(),
                lines
            ))
        }
        numi_config::DiscoveryError::Io(error) => CliError::new(error.to_string()),
    }
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

fn manifest_dir(manifest_path: &Path) -> Result<&Path, CliError> {
    manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            CliError::new(format!(
                "manifest {} has no parent directory",
                manifest_path.display()
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

fn workspace_member_root(member: &WorkspaceMember) -> String {
    Path::new(&member.config)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(display_path)
        .unwrap_or_else(|| String::from("."))
}

fn workspace_member_jobs(member: &WorkspaceMember) -> Option<&[String]> {
    (!member.jobs.is_empty()).then_some(member.jobs.as_slice())
}

fn workspace_jobs<T>(args: &T, member: &WorkspaceMember) -> Option<Vec<String>>
where
    T: WorkspaceJobArgs,
{
    match (args.selected_jobs(), workspace_member_jobs(member)) {
        (None, None) => None,
        (Some(cli_jobs), None) => Some(cli_jobs.to_vec()),
        (None, Some(member_jobs)) => Some(member_jobs.to_vec()),
        (Some(cli_jobs), Some(member_jobs)) => {
            let allowed_jobs = member_jobs
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            Some(
                cli_jobs
                    .iter()
                    .filter(|job| allowed_jobs.contains(job.as_str()))
                    .cloned()
                    .collect(),
            )
        }
    }
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

fn render_config_diagnostics<I, T>(diagnostics: I) -> CliError
where
    I: IntoIterator<Item = T>,
    T: std::fmt::Display,
{
    let message = diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    CliError::new(message)
}

fn display_path(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().into_owned()
}

trait WorkspaceJobArgs {
    fn selected_jobs(&self) -> Option<&[String]>;
}

impl WorkspaceJobArgs for GenerateArgs {
    fn selected_jobs(&self) -> Option<&[String]> {
        selected_jobs(&self.jobs)
    }
}

impl WorkspaceJobArgs for CheckArgs {
    fn selected_jobs(&self) -> Option<&[String]> {
        selected_jobs(&self.jobs)
    }
}
