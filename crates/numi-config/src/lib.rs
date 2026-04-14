mod discovery;
mod model;
mod validate;
mod workspace;

use std::{
    fs,
    path::{Path, PathBuf},
};

use numi_diagnostics::Diagnostic;

pub use discovery::{
    CONFIG_FILE_NAME, DiscoveryError, discover_config, discover_workspace_ancestor,
};
pub use model::{
    ACCESS_LEVEL_VALUES, BUNDLE_MODE_VALUES, BuiltinTemplateConfig, BundleConfig, Config,
    DEFAULT_ACCESS_LEVEL, DEFAULT_BUNDLE_MODE, DEFAULT_INCREMENTAL, DefaultsConfig, HookConfig,
    HooksConfig, INPUT_KIND_VALUES, InputConfig, JobConfig, TemplateConfig,
};
pub use workspace::{
    LoadedWorkspace, WorkspaceConfig, WorkspaceDefaults, WorkspaceError, WorkspaceJobDefaults,
    WorkspaceMember, WorkspaceMemberOverride, WorkspaceSettings, load_workspace_from_path,
};

#[derive(Debug)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: Config,
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Manifest {
    Config(Config),
    Workspace(WorkspaceConfig),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestKindSniff {
    ConfigLike,
    WorkspaceLike,
    BrokenWorkspaceLike,
    Mixed,
    Unknown,
    Unparsable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedManifest {
    pub path: PathBuf,
    pub manifest: Manifest,
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseToml(toml::de::Error),
    Invalid(Vec<Diagnostic>),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            Self::ParseToml(error) => write!(f, "failed to parse config TOML: {error}"),
            Self::Invalid(diagnostics) => {
                for (index, diagnostic) in diagnostics.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{diagnostic}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<WorkspaceError> for ConfigError {
    fn from(value: WorkspaceError) -> Self {
        match value {
            WorkspaceError::Read { path, source } => Self::Read { path, source },
            WorkspaceError::ParseToml(error) => Self::ParseToml(error),
            WorkspaceError::Invalid(diagnostics) => Self::Invalid(diagnostics),
        }
    }
}

pub fn parse_str(input: &str) -> Result<Config, ConfigError> {
    parse_str_with_validation(input, true)
}

fn parse_str_with_validation(input: &str, should_validate: bool) -> Result<Config, ConfigError> {
    let value: toml::Value = toml::from_str(input).map_err(ConfigError::ParseToml)?;
    let legacy_job_diagnostics = detect_legacy_jobs_array_syntax(&value);
    if !legacy_job_diagnostics.is_empty() {
        return Err(ConfigError::Invalid(legacy_job_diagnostics));
    }
    let legacy_diagnostics = detect_legacy_flat_builtin_template_syntax(&value);
    if !legacy_diagnostics.is_empty() {
        return Err(ConfigError::Invalid(legacy_diagnostics));
    }

    let config: Config = value.try_into().map_err(ConfigError::ParseToml)?;
    if !should_validate {
        return Ok(config);
    }

    let diagnostics = validate::validate_config(&config);

    if diagnostics.is_empty() {
        Ok(config)
    } else {
        Err(ConfigError::Invalid(diagnostics))
    }
}

pub fn parse_manifest_str(input: &str) -> Result<Manifest, ConfigError> {
    match sniff_manifest_kind_str(input) {
        ManifestKindSniff::ConfigLike => parse_str(input).map(Manifest::Config),
        ManifestKindSniff::WorkspaceLike => workspace::parse_workspace_str(input)
            .map(Manifest::Workspace)
            .map_err(ConfigError::from),
        ManifestKindSniff::BrokenWorkspaceLike => toml::from_str::<toml::Value>(input)
            .map(|_| unreachable!("successful TOML parsing must produce a known sniff kind"))
            .map_err(ConfigError::ParseToml),
        ManifestKindSniff::Mixed => Err(ConfigError::Invalid(vec![
            Diagnostic::error("manifest must not define both `jobs` and `workspace`")
                .with_hint(
                    "use `jobs` for a single-config manifest or `workspace` for a workspace manifest",
                ),
        ])),
        ManifestKindSniff::Unknown => Err(ConfigError::Invalid(vec![
            Diagnostic::error("manifest must define either `jobs` or `workspace`")
                .with_hint(
                    "add `[jobs.<name>]` for a single-config manifest, `[workspace]` for a workspace manifest, or legacy `[[members]]` while migrating",
                ),
        ])),
        ManifestKindSniff::Unparsable => toml::from_str::<toml::Value>(input)
            .map(|_| unreachable!("successful TOML parsing must produce a known sniff kind"))
            .map_err(ConfigError::ParseToml),
    }
}

pub fn sniff_manifest_kind_str(input: &str) -> ManifestKindSniff {
    match toml::from_str::<toml::Value>(input) {
        Ok(value) => sniff_manifest_kind_value(&value),
        Err(_) => sniff_manifest_kind_lossy(input),
    }
}

pub fn sniff_manifest_kind_from_path(path: &Path) -> Result<ManifestKindSniff, std::io::Error> {
    let contents = fs::read_to_string(path)?;
    Ok(sniff_manifest_kind_str(&contents))
}

fn sniff_manifest_kind_value(value: &toml::Value) -> ManifestKindSniff {
    classify_manifest_shape(
        value.get("jobs").is_some(),
        value.get("workspace").is_some() || value.get("members").is_some(),
        false,
    )
}

fn sniff_manifest_kind_lossy(input: &str) -> ManifestKindSniff {
    let mut in_root = true;
    let mut has_jobs = false;
    let mut has_workspaceish = false;

    for line in input.lines() {
        let Some(trimmed) = strip_toml_comment(line) else {
            continue;
        };

        if let Some(header) = parse_toml_table_header(trimmed) {
            in_root = false;

            if header.is_array {
                if header.path.len() == 1 && header.path[0] == "members" {
                    has_workspaceish = true;
                }
            } else if header
                .path
                .first()
                .is_some_and(|segment| *segment == "workspace")
            {
                has_workspaceish = true;
            }

            continue;
        }

        if in_root
            && let Some(path) = parse_toml_key_path_before_equals(trimmed)
            && let Some(segment) = path.first().copied()
        {
            match segment {
                "jobs" => has_jobs = true,
                "workspace" | "members" => has_workspaceish = true,
                _ => {}
            }
        }
    }

    classify_manifest_shape(has_jobs, has_workspaceish, true)
}

fn classify_manifest_shape(
    has_jobs: bool,
    has_workspaceish: bool,
    lossy: bool,
) -> ManifestKindSniff {
    match (has_jobs, has_workspaceish, lossy) {
        (true, false, _) => ManifestKindSniff::ConfigLike,
        (false, true, false) => ManifestKindSniff::WorkspaceLike,
        (false, true, true) => ManifestKindSniff::BrokenWorkspaceLike,
        (true, true, _) => ManifestKindSniff::Mixed,
        (false, false, false) => ManifestKindSniff::Unknown,
        (false, false, true) => ManifestKindSniff::Unparsable,
    }
}

fn strip_toml_comment(line: &str) -> Option<&str> {
    let mut in_basic = false;
    let mut in_literal = false;
    let mut escape = false;

    for (index, ch) in line.char_indices() {
        match ch {
            '"' if !in_literal && !escape => in_basic = !in_basic,
            '\'' if !in_basic => in_literal = !in_literal,
            '#' if !in_basic && !in_literal => {
                let trimmed = line[..index].trim();
                return (!trimmed.is_empty()).then_some(trimmed);
            }
            _ => {}
        }

        escape = in_basic && ch == '\\' && !escape;
        if ch != '\\' {
            escape = false;
        }
    }

    let trimmed = line.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

struct TomlHeader<'a> {
    is_array: bool,
    path: Vec<&'a str>,
}

fn parse_toml_table_header(line: &str) -> Option<TomlHeader<'_>> {
    if let Some(inner) = line
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
    {
        return Some(TomlHeader {
            is_array: true,
            path: parse_toml_path(inner)?,
        });
    }

    line.strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(parse_toml_path)
        .map(|path| TomlHeader {
            is_array: false,
            path,
        })
}

fn parse_toml_key_path_before_equals(line: &str) -> Option<Vec<&str>> {
    let mut in_basic = false;
    let mut in_literal = false;
    let mut escape = false;

    for (index, ch) in line.char_indices() {
        match ch {
            '"' if !in_literal && !escape => in_basic = !in_basic,
            '\'' if !in_basic => in_literal = !in_literal,
            '=' if !in_basic && !in_literal => return parse_toml_path(&line[..index]),
            _ => {}
        }

        escape = in_basic && ch == '\\' && !escape;
        if ch != '\\' {
            escape = false;
        }
    }

    None
}

fn parse_toml_path(input: &str) -> Option<Vec<&str>> {
    let path = input
        .split('.')
        .map(|segment| segment.trim())
        .filter(|segment| !segment.is_empty())
        .map(unquote_toml_key_segment)
        .collect::<Vec<_>>();

    (!path.is_empty()).then_some(path)
}

fn unquote_toml_key_segment(segment: &str) -> &str {
    if segment.len() >= 2 {
        if let Some(unquoted) = segment
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            return unquoted.trim();
        }
        if let Some(unquoted) = segment
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            return unquoted.trim();
        }
    }

    segment
}

fn detect_legacy_jobs_array_syntax(value: &toml::Value) -> Vec<Diagnostic> {
    value
        .get("jobs")
        .and_then(toml::Value::as_array)
        .map(|_| {
            vec![
                Diagnostic::error("legacy `[[jobs]]` syntax is no longer supported").with_hint(
                    "use named job tables such as `[jobs.assets]`, `[[jobs.assets.inputs]]`, and `[jobs.assets.template]`",
                ),
            ]
        })
        .unwrap_or_default()
}

fn detect_legacy_flat_builtin_template_syntax(value: &toml::Value) -> Vec<Diagnostic> {
    value
        .get("jobs")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(job_name, job)| {
            let template = job.get("template")?.as_table()?;
            let builtin = template.get("builtin")?;
            let builtin_name = builtin.as_str()?;

            let mut diagnostic =
                Diagnostic::error("legacy flat built-in template syntax is no longer supported")
                    .with_hint(format!(
                        "use `[jobs.{job_name}.template.builtin] language = \"...\" name = \"...\"` instead; for example, replace `[jobs.{job_name}.template] builtin = \"{builtin_name}\"` with `[jobs.{job_name}.template.builtin] language = \"swift\" name = \"{builtin_name}\"`"
                    ));

            diagnostic = diagnostic.with_job(job_name.to_owned());

            Some(diagnostic)
        })
        .collect()
}

pub fn load_from_path(path: &Path) -> Result<LoadedConfig, ConfigError> {
    load_from_path_with_validation(path, true)
}

pub fn load_unvalidated_from_path(path: &Path) -> Result<LoadedConfig, ConfigError> {
    load_from_path_with_validation(path, false)
}

fn load_from_path_with_validation(
    path: &Path,
    should_validate: bool,
) -> Result<LoadedConfig, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config = parse_str_with_validation(&contents, should_validate)?;

    Ok(LoadedConfig {
        path: path.to_path_buf(),
        config,
    })
}

pub fn load_manifest_from_path(path: &Path) -> Result<LoadedManifest, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest = parse_manifest_str(&contents)?;

    Ok(LoadedManifest {
        path: path.to_path_buf(),
        manifest,
    })
}

pub fn resolve_selected_jobs<'a>(
    config: &'a Config,
    selected_jobs: Option<&[String]>,
) -> Result<Vec<&'a JobConfig>, Vec<Diagnostic>> {
    match selected_jobs {
        None => Ok(config.jobs.iter().collect()),
        Some(selected_jobs) => {
            let mut resolved = Vec::with_capacity(selected_jobs.len());
            let mut diagnostics = Vec::new();

            for selected_job in selected_jobs {
                match config.jobs.iter().find(|job| job.name == *selected_job) {
                    Some(job) => resolved.push(job),
                    None => diagnostics.push(
                        Diagnostic::error(format!("job `{selected_job}` was not found"))
                            .with_job(selected_job.clone())
                            .with_hint("select one of the job names declared in numi.toml"),
                    ),
                }
            }

            if diagnostics.is_empty() {
                Ok(resolved)
            } else {
                Err(diagnostics)
            }
        }
    }
}

pub fn resolve_config(config: &Config) -> Config {
    let mut resolved = config.clone();

    if resolved.defaults.access_level.is_none() {
        resolved.defaults.access_level = Some(DEFAULT_ACCESS_LEVEL.to_string());
    }

    if resolved.defaults.bundle.mode.is_none() {
        resolved.defaults.bundle.mode = Some(DEFAULT_BUNDLE_MODE.to_string());
    }

    if resolved.defaults.incremental.is_none() {
        resolved.defaults.incremental = Some(DEFAULT_INCREMENTAL);
    }

    resolved
}

pub fn workspace_member_config_path(workspace_root: &Path, member_root: &str) -> PathBuf {
    workspace_root.join(member_root).join(CONFIG_FILE_NAME)
}

pub fn resolve_workspace_member_config(
    workspace_root: &Path,
    workspace: &WorkspaceConfig,
    member_root: &str,
    member_config: &Config,
) -> Result<Config, Vec<Diagnostic>> {
    let mut resolved = member_config.clone();

    for job in &mut resolved.jobs {
        if let Some(defaults) = workspace.workspace.defaults.jobs.get(&job.name)
            && job.template.is_empty()
        {
            if defaults.template.path.is_some() {
                job.template.path =
                    defaults.template.path.as_deref().map(|path| {
                        rebase_workspace_template_path(workspace_root, member_root, path)
                    });
            }

            if defaults.template.auto_lookup.is_some() {
                job.template.auto_lookup = defaults.template.auto_lookup;
            }
        }

        if let Some(defaults) = workspace.workspace.defaults.jobs.get(&job.name)
            && let (Some(job_builtin), Some(default_builtin)) = (
                job.template.builtin.as_mut(),
                defaults.template.builtin.as_ref(),
            )
            && job_builtin.language.is_none()
        {
            job_builtin.language = default_builtin.language.clone();
        }

        if job.hooks.pre_generate.is_none() {
            job.hooks.pre_generate = workspace_hook_for_phase(
                workspace_root,
                workspace,
                member_root,
                &job.name,
                HookPhaseSelector::PreGenerate,
            );
        }

        if job.hooks.post_generate.is_none() {
            job.hooks.post_generate = workspace_hook_for_phase(
                workspace_root,
                workspace,
                member_root,
                &job.name,
                HookPhaseSelector::PostGenerate,
            );
        }
    }

    if let Some(override_config) = workspace.workspace.member_overrides.get(member_root) {
        let _ = override_config;
    }

    let diagnostics = validate::validate_config(&resolved);
    if diagnostics.is_empty() {
        Ok(resolved)
    } else {
        Err(diagnostics)
    }
}

fn rebase_workspace_template_path(
    workspace_root: &Path,
    member_root: &str,
    template_path: &str,
) -> String {
    let member_dir = workspace_root.join(member_root);
    let workspace_template_path = workspace_root.join(template_path);
    relative_path_from(&member_dir, &workspace_template_path)
        .to_string_lossy()
        .into_owned()
}

fn rebase_workspace_hook(
    workspace_root: &Path,
    member_root: &str,
    hook: &HookConfig,
) -> HookConfig {
    let mut rebased = hook.clone();
    let Some(command0) = rebased.command.first_mut() else {
        return rebased;
    };

    if !command_looks_like_path(command0) {
        return rebased;
    }

    let member_dir = workspace_root.join(member_root);
    let workspace_command_path = workspace_root.join(&*command0);
    *command0 = relative_path_from(&member_dir, &workspace_command_path)
        .to_string_lossy()
        .into_owned();

    rebased
}

#[derive(Clone, Copy)]
enum HookPhaseSelector {
    PreGenerate,
    PostGenerate,
}

fn workspace_hook_for_phase(
    workspace_root: &Path,
    workspace: &WorkspaceConfig,
    member_root: &str,
    job_name: &str,
    phase: HookPhaseSelector,
) -> Option<HookConfig> {
    let hook = workspace
        .workspace
        .defaults
        .jobs
        .get(job_name)
        .and_then(|defaults| match phase {
            HookPhaseSelector::PreGenerate => defaults.hooks.pre_generate.as_ref(),
            HookPhaseSelector::PostGenerate => defaults.hooks.post_generate.as_ref(),
        })
        .or(match phase {
            HookPhaseSelector::PreGenerate => {
                workspace.workspace.defaults.hooks.pre_generate.as_ref()
            }
            HookPhaseSelector::PostGenerate => {
                workspace.workspace.defaults.hooks.post_generate.as_ref()
            }
        })?;

    Some(rebase_workspace_hook(workspace_root, member_root, hook))
}

fn command_looks_like_path(command: &str) -> bool {
    let path = Path::new(command);
    path.is_absolute()
        || command.starts_with('.')
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        || path.components().count() > 1
}

fn relative_path_from(from: &Path, to: &Path) -> PathBuf {
    let from_components = from.components().collect::<Vec<_>>();
    let to_components = to.components().collect::<Vec<_>>();

    let mut common_prefix = 0;
    while common_prefix < from_components.len()
        && common_prefix < to_components.len()
        && from_components[common_prefix] == to_components[common_prefix]
    {
        common_prefix += 1;
    }

    let mut result = PathBuf::new();

    for component in &from_components[common_prefix..] {
        if !matches!(component, std::path::Component::CurDir) {
            result.push("..");
        }
    }

    for component in &to_components[common_prefix..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        result.push(".");
    }

    result
}
