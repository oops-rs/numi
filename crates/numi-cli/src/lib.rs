pub mod cli;

use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    io::{self, IsTerminal},
    path::{Component, Path, PathBuf},
};

use cli::{
    CheckArgs, Cli, Command, ConfigSubcommand, DumpContextArgs, GenerateArgs, InitArgs, LocateArgs,
    PrintArgs,
};
use numi_config::{
    CONFIG_FILE_NAME, Config, LoadedManifest, Manifest, ManifestKindSniff, WorkspaceConfig,
    WorkspaceMember, resolve_workspace_member_config, workspace_member_config_path,
};

const STARTER_CONFIG_FALLBACK: &str = include_str!("../assets/starter-numi.toml");
const STATUS_LABEL_WIDTH: usize = 10;

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
    let loaded = load_execution_manifest(args.config.as_deref(), args.workspace)?;
    cli_ui().manifest(&loaded.manifest, &loaded.path);
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
    let incremental = args.incremental_override.resolve();
    let ui = cli_ui();
    let report = numi_core::generate_with_options_and_progress(
        config_path,
        selected_jobs,
        numi_core::GenerateOptions {
            incremental: incremental.incremental,
            parse_cache: incremental.parse_cache,
            force_regenerate: incremental.force_regenerate,
            workspace_manifest_path: None,
        },
        |progress| ui.progress(progress),
    )
    .map_err(|error| CliError::new(error.to_string()))?;
    let output_root = manifest_dir(config_path)?;
    ui.job_reports(output_root, &report.jobs);
    print_warnings(&report.warnings);
    let mut summary = JobSummary::default();
    summary.record_jobs(&report.jobs);
    ui.generation_summary(summary);
    Ok(())
}

fn run_check(args: &CheckArgs) -> Result<(), CliError> {
    let loaded = load_execution_manifest(args.config.as_deref(), args.workspace)?;
    cli_ui().manifest(&loaded.manifest, &loaded.path);
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
        cli_ui().status(
            StatusTone::Success,
            "Polished",
            "generated outputs look fresh",
        );
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
    cli_ui().status(
        StatusTone::Success,
        "Stitched",
        format!("starter {}", display_contextual_path(&config_path)),
    );

    Ok(())
}

fn run_generate_workspace(
    manifest_path: &Path,
    workspace: &WorkspaceConfig,
    args: &GenerateArgs,
) -> Result<(), CliError> {
    let workspace_dir = manifest_dir(manifest_path)?;
    let ui = cli_ui();
    let mut summary = JobSummary::default();

    for member in workspace.members() {
        let member_root = workspace_member_root(&member);
        let config_path = workspace_member_config_path(workspace_dir, &member_root);
        let loaded_member = numi_config::load_unvalidated_from_path(&config_path)
            .map_err(|error| CliError::new(error.to_string()))?;
        let merged_config = resolve_workspace_member_config(
            workspace_dir,
            workspace,
            &member_root,
            &loaded_member.config,
        )
        .map_err(render_config_diagnostics)?;
        let selected_jobs = workspace_jobs(args, &member);
        let incremental = args.incremental_override.resolve();
        let report = numi_core::generate_loaded_config_with_progress(
            &config_path,
            &merged_config,
            selected_jobs.as_deref(),
            numi_core::GenerateOptions {
                incremental: incremental.incremental,
                parse_cache: incremental.parse_cache,
                force_regenerate: incremental.force_regenerate,
                workspace_manifest_path: Some(manifest_path.to_path_buf()),
            },
            |progress| ui.progress(progress),
        )
        .map_err(|error| CliError::new(error.to_string()))?;
        ui.job_reports(workspace_dir, &report.jobs);
        print_warnings(&report.warnings);
        summary.record_jobs(&report.jobs);
    }

    ui.generation_summary(summary);
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
        let merged_config = resolve_workspace_member_config(
            workspace_dir,
            workspace,
            &member_root,
            &loaded_member.config,
        )
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
                .map(|path| normalize_workspace_stale_path(path.as_std_path(), workspace_dir)),
        );
    }

    if stale_paths.is_empty() {
        cli_ui().status(
            StatusTone::Success,
            "Polished",
            "workspace outputs look fresh",
        );
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

fn load_execution_manifest(
    explicit_path: Option<&Path>,
    workspace: bool,
) -> Result<LoadedManifest, CliError> {
    if workspace || explicit_path.is_some() {
        return load_cli_manifest(explicit_path, workspace);
    }

    let cwd = current_dir()?;
    let manifest_path = numi_config::discover_config(&cwd, None)
        .map_err(|error| CliError::new(error.to_string()))?;
    let manifest_kind =
        numi_config::sniff_manifest_kind_from_path(&manifest_path).map_err(|error| {
            CliError::new(format!(
                "failed to read manifest {}: {error}",
                manifest_path.display()
            ))
        })?;

    if matches!(manifest_kind, ManifestKindSniff::ConfigLike)
        && let Ok(workspace_loaded) = load_workspace_cli_manifest(None)
        && workspace_loaded.path != manifest_path
    {
        return Ok(workspace_loaded);
    }

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
    Ok(Cow::Borrowed(STARTER_CONFIG_FALLBACK))
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
        cli_ui().warning(&warning.to_string());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Accent,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy)]
struct CliUi {
    interactive: bool,
    color: bool,
}

impl CliUi {
    fn stderr() -> Self {
        let interactive = io::stderr().is_terminal();
        let color = interactive && std::env::var_os("NO_COLOR").is_none();
        Self { interactive, color }
    }

    fn manifest(&self, manifest: &Manifest, path: &Path) {
        let kind = match manifest {
            Manifest::Config(_) => "config",
            Manifest::Workspace(_) => "workspace",
        };
        self.status(
            StatusTone::Accent,
            "Summoning",
            format!("{kind} {}", display_contextual_path(path)),
        );
    }

    fn progress(&self, progress: &numi_core::GenerateProgress) {
        match progress {
            numi_core::GenerateProgress::JobStarted { job_name } => {
                let (label, tone, message) = job_started_status(job_name);
                self.status(tone, label, message);
            }
        }
    }

    fn job_reports(&self, root: &Path, jobs: &[numi_core::JobReport]) {
        for job in jobs {
            for hook in &job.hook_reports {
                let (label, tone, message) = hook_status(&job.job_name, hook);
                self.status(tone, label, message);
            }

            let (label, tone) = match job.outcome {
                numi_core::WriteOutcome::Created => ("Stitched", StatusTone::Success),
                numi_core::WriteOutcome::Updated => ("Restitched", StatusTone::Success),
                numi_core::WriteOutcome::Unchanged => ("Keeping", StatusTone::Accent),
                numi_core::WriteOutcome::Skipped => ("Skipping", StatusTone::Warning),
            };
            let output_path = display_relative_path(root, job.output_path.as_std_path());
            self.status(tone, label, format!("{} -> {}", job.job_name, output_path));
        }
    }

    fn generation_summary(&self, summary: JobSummary) {
        if summary.total == 0 {
            self.status(StatusTone::Accent, "Keeping", "no jobs were selected");
            return;
        }

        let mut parts = Vec::new();
        if summary.created > 0 {
            parts.push(format!("{} stitched", summary.created));
        }
        if summary.updated > 0 {
            parts.push(format!("{} re-stitched", summary.updated));
        }
        if summary.unchanged > 0 {
            parts.push(format!("{} kept", summary.unchanged));
        }
        if summary.skipped > 0 {
            parts.push(format!("{} skipped", summary.skipped));
        }

        let message = if parts.is_empty() {
            format!("{} jobs settled", summary.total)
        } else {
            format!("{} jobs settled ({})", summary.total, parts.join(", "))
        };
        self.status(StatusTone::Success, "Polished", message);
    }

    fn warning(&self, message: &str) {
        let message = rewrite_diagnostic_paths_in_cwd(message);
        if self.interactive {
            let body = message.strip_prefix("warning: ").unwrap_or(&message);
            self.block(StatusTone::Warning, "Noted", body);
        } else {
            eprintln!("{message}");
        }
    }

    fn error(&self, message: &str) {
        let message = rewrite_diagnostic_paths_in_cwd(message);
        if self.interactive {
            self.block(StatusTone::Error, "Oops", &message);
        } else {
            eprintln!("{message}");
        }
    }

    fn status(&self, tone: StatusTone, label: &str, message: impl AsRef<str>) {
        if !self.interactive {
            return;
        }
        self.block(tone, label, message.as_ref());
    }

    fn block(&self, tone: StatusTone, label: &str, message: &str) {
        let rendered = format_status_block(label, tone, message, self.color);
        eprint!("{rendered}");
    }
}

fn hook_status(job_name: &str, hook: &numi_core::HookReport) -> (&'static str, StatusTone, String) {
    let label = match hook.phase {
        numi_core::HookPhase::PreGenerate => "Preparing",
        numi_core::HookPhase::PostGenerate => "Tidying",
    };

    let message = if hook.command.is_empty() {
        format!("{job_name} hook")
    } else {
        format!("{job_name} hook -> {}", render_hook_command(&hook.command))
    };

    (label, StatusTone::Accent, message)
}

fn job_started_status(job_name: &str) -> (&'static str, StatusTone, String) {
    ("Weaving", StatusTone::Accent, format!("{job_name}..."))
}

fn render_hook_command(command: &[String]) -> String {
    command.join(" ")
}

fn rewrite_diagnostic_paths_in_cwd(message: &str) -> String {
    std::env::current_dir()
        .ok()
        .map(|cwd| rewrite_diagnostic_paths(message, &cwd))
        .unwrap_or_else(|| message.to_string())
}

fn rewrite_diagnostic_paths(message: &str, cwd: &Path) -> String {
    let mut rewritten = String::with_capacity(message.len());
    let mut remaining = message;

    while let Some(marker_index) = remaining.find("[path: ") {
        let (prefix, after_prefix) = remaining.split_at(marker_index);
        rewritten.push_str(prefix);

        let after_marker = &after_prefix["[path: ".len()..];
        let Some(path_end) = after_marker.find(']') else {
            rewritten.push_str(after_prefix);
            return rewritten;
        };

        let (path_text, suffix) = after_marker.split_at(path_end);
        rewritten.push_str("[path: ");
        rewritten.push_str(&rewrite_diagnostic_path(path_text, cwd));
        rewritten.push(']');
        remaining = &suffix[1..];
    }

    rewritten.push_str(remaining);
    rewritten
}

fn rewrite_diagnostic_path(path_text: &str, cwd: &Path) -> String {
    Path::new(path_text)
        .strip_prefix(cwd)
        .map(display_path)
        .unwrap_or_else(|_| path_text.to_string())
}

fn cli_ui() -> CliUi {
    CliUi::stderr()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct JobSummary {
    total: usize,
    created: usize,
    updated: usize,
    unchanged: usize,
    skipped: usize,
}

impl JobSummary {
    fn record_jobs(&mut self, jobs: &[numi_core::JobReport]) {
        for job in jobs {
            self.record_outcome(job.outcome);
        }
    }

    fn record_outcome(&mut self, outcome: numi_core::WriteOutcome) {
        self.total += 1;
        match outcome {
            numi_core::WriteOutcome::Created => self.created += 1,
            numi_core::WriteOutcome::Updated => self.updated += 1,
            numi_core::WriteOutcome::Unchanged => self.unchanged += 1,
            numi_core::WriteOutcome::Skipped => self.skipped += 1,
        }
    }
}

fn format_status_block(label: &str, tone: StatusTone, message: &str, color: bool) -> String {
    let padded_label = format!("{label:>width$}", width = STATUS_LABEL_WIDTH);
    let rendered_label = format_status_label(&padded_label, tone, color);
    let continuation = " ".repeat(STATUS_LABEL_WIDTH);
    let mut lines = message.lines();
    let mut rendered = String::new();

    if let Some(first_line) = lines.next() {
        rendered.push_str(&format!("{rendered_label} {first_line}\n"));
    } else {
        rendered.push_str(&format!("{rendered_label}\n"));
    }

    for line in lines {
        rendered.push_str(&format!("{continuation} {line}\n"));
    }

    rendered
}

fn format_status_label(label: &str, tone: StatusTone, color: bool) -> String {
    if !color {
        return label.to_string();
    }

    let code = match tone {
        StatusTone::Accent => "36",
        StatusTone::Success => "32",
        StatusTone::Warning => "33",
        StatusTone::Error => "31",
    };
    format!("\x1b[{code};1m{label}\x1b[0m")
}

fn display_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn display_contextual_path(path: &Path) -> String {
    let absolute_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    if let Ok(cwd) = std::env::current_dir() {
        let absolute_cwd = cwd.canonicalize().unwrap_or(cwd);
        if let Some(relative) = lexical_relative_path(&absolute_path, &absolute_cwd) {
            return display_path(relative);
        }
    }

    display_path(absolute_path)
}

fn lexical_relative_path(path: &Path, base: &Path) -> Option<PathBuf> {
    let path_components = path.components().collect::<Vec<_>>();
    let base_components = base.components().collect::<Vec<_>>();

    let mut common_len = 0;
    while common_len < path_components.len()
        && common_len < base_components.len()
        && path_components[common_len] == base_components[common_len]
    {
        common_len += 1;
    }

    if common_len == 0 {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &base_components[common_len..] {
        match component {
            Component::Normal(_) => relative.push(".."),
            Component::CurDir => {}
            Component::ParentDir => relative.push(".."),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    for component in &path_components[common_len..] {
        relative.push(component.as_os_str());
    }

    if relative.as_os_str().is_empty() {
        relative.push(".");
    }

    Some(relative)
}

pub fn print_error(error: &CliError) {
    cli_ui().error(&error.message);
}

#[cfg(test)]
mod cli_ui_tests {
    use super::*;

    #[test]
    fn format_status_block_renders_single_line_plain() {
        let rendered = format_status_block(
            "Summoning",
            StatusTone::Accent,
            "workspace numi.toml",
            false,
        );
        assert_eq!(rendered, " Summoning workspace numi.toml\n");
    }

    #[test]
    fn format_status_block_indents_multiline_messages() {
        let rendered =
            format_status_block("Oops", StatusTone::Error, "first line\nsecond line", false);
        assert_eq!(rendered, "      Oops first line\n           second line\n");
    }

    #[test]
    fn format_status_label_wraps_color_when_enabled() {
        let rendered = format_status_label("Stitched", StatusTone::Success, true);
        assert!(rendered.starts_with("\u{1b}[32;1m"));
        assert!(rendered.ends_with("\u{1b}[0m"));
        assert!(rendered.contains("Stitched"));
    }

    #[test]
    fn generation_summary_reports_breakdown() {
        let mut summary = JobSummary::default();
        summary.record_outcome(numi_core::WriteOutcome::Created);
        summary.record_outcome(numi_core::WriteOutcome::Unchanged);
        summary.record_outcome(numi_core::WriteOutcome::Skipped);
        let rendered = format_status_block(
            "Polished",
            StatusTone::Success,
            "3 jobs settled (1 stitched, 1 kept, 1 skipped)",
            false,
        );

        assert_eq!(
            rendered,
            "  Polished 3 jobs settled (1 stitched, 1 kept, 1 skipped)\n"
        );
        assert_eq!(
            summary,
            JobSummary {
                total: 3,
                created: 1,
                updated: 0,
                unchanged: 1,
                skipped: 1,
            }
        );
    }

    #[test]
    fn hook_status_message_includes_configured_command() {
        let hook = numi_core::HookReport {
            phase: numi_core::HookPhase::PostGenerate,
            command: vec!["utils/numi-post-generate-format.sh".to_string()],
        };

        let (label, tone, message) = hook_status("assets", &hook);

        assert_eq!(label, "Tidying");
        assert_eq!(tone, StatusTone::Accent);
        assert_eq!(message, "assets hook -> utils/numi-post-generate-format.sh");
    }

    #[test]
    fn hook_status_message_falls_back_to_hook_name_when_command_is_empty() {
        let hook = numi_core::HookReport {
            phase: numi_core::HookPhase::PreGenerate,
            command: Vec::new(),
        };

        let (label, tone, message) = hook_status("files", &hook);

        assert_eq!(label, "Preparing");
        assert_eq!(tone, StatusTone::Accent);
        assert_eq!(message, "files hook");
    }

    #[test]
    fn job_started_status_message_describes_current_job() {
        let (label, tone, message) = job_started_status("assets");

        assert_eq!(label, "Weaving");
        assert_eq!(tone, StatusTone::Accent);
        assert_eq!(message, "assets...");
    }

    #[test]
    fn rewrite_diagnostic_paths_relativizes_paths_under_cwd() {
        let cwd = Path::new("/tmp/workspace");
        let message =
            "warning: skipped entry [path: /tmp/workspace/AppUI/Resources/Localizable.xcstrings]";

        let rewritten = rewrite_diagnostic_paths(message, cwd);

        assert_eq!(
            rewritten,
            "warning: skipped entry [path: AppUI/Resources/Localizable.xcstrings]"
        );
    }

    #[test]
    fn rewrite_diagnostic_paths_keeps_paths_outside_cwd() {
        let cwd = Path::new("/tmp/workspace");
        let message = "warning: skipped entry [path: /tmp/other/Localizable.xcstrings]";

        let rewritten = rewrite_diagnostic_paths(message, cwd);

        assert_eq!(
            rewritten,
            "warning: skipped entry [path: /tmp/other/Localizable.xcstrings]"
        );
    }

    #[test]
    fn lexical_relative_path_walks_up_to_workspace_manifest() {
        let path = Path::new("/tmp/workspace/numi.toml");
        let base = Path::new("/tmp/workspace/AppUI");

        let relative = lexical_relative_path(path, base).expect("relative path should resolve");

        assert_eq!(relative, PathBuf::from("../numi.toml"));
    }
}
