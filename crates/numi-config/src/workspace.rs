use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use numi_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};

use crate::{
    ConfigError, DiscoveryError,
    model::{SWIFT_BUILTIN_TEMPLATE_VALUES, TemplateConfig},
};

pub const WORKSPACE_FILE_NAME: &str = "numi-workspace.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub workspace: WorkspaceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSettings {
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "WorkspaceDefaults::is_empty")]
    pub defaults: WorkspaceDefaults,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub member_overrides: BTreeMap<String, WorkspaceMemberOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDefaults {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub jobs: BTreeMap<String, WorkspaceJobDefaults>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceJobDefaults {
    #[serde(default, skip_serializing_if = "TemplateConfig::is_empty")]
    pub template: TemplateConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMemberOverride {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
}

#[derive(Debug)]
pub struct LoadedWorkspace {
    pub path: PathBuf,
    pub config: WorkspaceConfig,
}

#[derive(Debug)]
pub enum WorkspaceError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseToml(toml::de::Error),
    Invalid(Vec<Diagnostic>),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read workspace manifest {}: {source}",
                    path.display()
                )
            }
            Self::ParseToml(error) => {
                write!(f, "failed to parse workspace manifest TOML: {error}")
            }
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

impl std::error::Error for WorkspaceError {}

#[derive(Debug)]
pub enum WorkspaceDiscoveryError {
    ExplicitPathNotFound(PathBuf),
    NotFound {
        start_dir: PathBuf,
    },
    Ambiguous {
        root: PathBuf,
        matches: Vec<PathBuf>,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for WorkspaceDiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitPathNotFound(path) => {
                write!(f, "workspace manifest not found: {}", path.display())
            }
            Self::NotFound { start_dir } => write!(
                f,
                "No workspace manifest found from {}\n\nPlease specify one with:\n  numi workspace locate --workspace <path>",
                start_dir.display()
            ),
            Self::Ambiguous { root, matches } => {
                writeln!(
                    f,
                    "Multiple workspace manifests found under {}:",
                    root.display()
                )?;
                for path in matches {
                    writeln!(f, "  - {}", path.display())?;
                }
                write!(
                    f,
                    "\nPlease specify one with:\n  numi workspace locate --workspace <path>"
                )
            }
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for WorkspaceDiscoveryError {}

impl From<ConfigError> for WorkspaceError {
    fn from(value: ConfigError) -> Self {
        match value {
            ConfigError::Read { path, source } => Self::Read { path, source },
            ConfigError::ParseToml(error) => Self::ParseToml(error),
            ConfigError::Invalid(diagnostics) => Self::Invalid(diagnostics),
        }
    }
}

impl From<DiscoveryError> for WorkspaceDiscoveryError {
    fn from(value: DiscoveryError) -> Self {
        match value {
            DiscoveryError::ExplicitPathNotFound(path) => Self::ExplicitPathNotFound(path),
            DiscoveryError::NotFound { start_dir } => Self::NotFound { start_dir },
            DiscoveryError::Ambiguous { root, matches } => Self::Ambiguous { root, matches },
            DiscoveryError::Io(error) => Self::Io(error),
        }
    }
}

pub fn load_workspace_from_path(path: &Path) -> Result<LoadedWorkspace, WorkspaceError> {
    let contents = fs::read_to_string(path).map_err(|source| WorkspaceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config = parse_workspace_str(&contents)?;

    Ok(LoadedWorkspace {
        path: path.to_path_buf(),
        config,
    })
}

pub fn discover_workspace(
    start_dir: &Path,
    explicit_path: Option<&Path>,
) -> Result<PathBuf, WorkspaceDiscoveryError> {
    crate::discovery::discover_named_file(start_dir, explicit_path, WORKSPACE_FILE_NAME)
        .map_err(WorkspaceDiscoveryError::from)
}

pub(crate) fn parse_workspace_str(input: &str) -> Result<WorkspaceConfig, WorkspaceError> {
    let raw: RawWorkspaceManifest = toml::from_str(input).map_err(WorkspaceError::ParseToml)?;
    let diagnostics = validate_workspace(&raw);

    if diagnostics.is_empty() {
        Ok(raw.into_workspace())
    } else {
        Err(WorkspaceError::Invalid(diagnostics))
    }
}

fn validate_workspace(config: &RawWorkspaceManifest) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if config.version != 1 {
        diagnostics.push(
            Diagnostic::error("workspace version must be 1")
                .with_hint("set `version = 1` in numi-workspace.toml"),
        );
    }

    if config.workspace.members.is_empty() {
        diagnostics.push(
            Diagnostic::error("workspace must declare at least one member")
                .with_hint("add at least one entry to `workspace.members`"),
        );
    }

    let mut members = BTreeSet::new();

    for member in &config.workspace.members {
        if member.trim().is_empty() {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be relative member roots")
                    .with_hint("use values like `AppUI` or `Core`, not empty paths")
                    .with_path(PathBuf::from(member)),
            );
            continue;
        }

        if is_config_path(member) {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be relative member roots")
                    .with_hint("use values like `AppUI` or `Core`, not config-file paths like `AppUI/numi.toml`")
                    .with_path(PathBuf::from(member)),
            );
            continue;
        }

        if !is_relative_root(member) {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be relative member roots")
                    .with_hint("use relative paths like `AppUI` or `packages/Core`")
                    .with_path(PathBuf::from(member)),
            );
            continue;
        }

        if !members.insert(member.as_str()) {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be unique")
                    .with_hint("remove duplicate entries from `workspace.members`")
                    .with_path(PathBuf::from(member)),
            );
        }
    }

    for (member_path, override_config) in &config.workspace.member_overrides {
        if !members.contains(member_path.as_str()) {
            diagnostics.push(
                Diagnostic::error("workspace.member_overrides keys must match declared members")
                    .with_hint("add the member to `workspace.members` or remove the override")
                    .with_path(PathBuf::from(member_path)),
            );
        }

        let Some(job_list) = override_config.jobs.as_ref() else {
            continue;
        };

        if job_list.is_empty() {
            diagnostics.push(
                Diagnostic::error("workspace member override jobs must not be empty")
                    .with_hint("omit `jobs` or provide at least one job name")
                    .with_path(PathBuf::from(member_path)),
            );
            continue;
        }

        let mut job_names = BTreeSet::new();
        for job in job_list {
            if !job_names.insert(job.as_str()) {
                diagnostics.push(
                    Diagnostic::error("workspace member override jobs must be unique")
                        .with_hint("remove duplicate job names from the override")
                        .with_job(job.clone())
                        .with_path(PathBuf::from(member_path)),
                );
            }
        }
    }

    for (job_name, job_defaults) in &config.workspace.defaults.jobs {
        validate_template(
            &mut diagnostics,
            job_name,
            &job_defaults.template,
            "workspace default job template",
        );
    }

    diagnostics
}

fn is_relative_root(member: &str) -> bool {
    let path = Path::new(member);
    if path.is_absolute() {
        return false;
    }

    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
}

fn is_config_path(member: &str) -> bool {
    Path::new(member)
        .extension()
        .is_some_and(|extension| extension == "toml")
}

fn validate_template(
    diagnostics: &mut Vec<Diagnostic>,
    job_name: &str,
    template: &TemplateConfig,
    label: &str,
) {
    let template_sources = usize::from(
        template
            .builtin
            .as_ref()
            .is_some_and(|builtin| !builtin.is_empty()),
    ) + usize::from(template.path.is_some());

    if template_sources != 1 {
        diagnostics.push(
            Diagnostic::error(format!("{label} must set exactly one source"))
                .with_job(job_name.to_owned())
                .with_hint(
                    "set either `[workspace.defaults.jobs.<name>.template.builtin] swift = \"...\"` or `[workspace.defaults.jobs.<name>.template] path = \"...\"`",
                ),
        );
    }

    if let Some(builtin) = &template.builtin {
        if builtin.swift.is_none() && template.path.is_none() {
            diagnostics.push(
                Diagnostic::error(format!("{label} builtin must set exactly one namespace"))
                    .with_job(job_name.to_owned())
                    .with_hint(
                        "set `[workspace.defaults.jobs.<name>.template.builtin] swift = \"...\"`",
                    ),
            );
        } else if let Some(swift_builtin) = builtin.swift.as_deref() {
            if !SWIFT_BUILTIN_TEMPLATE_VALUES.contains(&swift_builtin) {
                diagnostics.push(
                    Diagnostic::error(format!(
                        "workspace defaults template builtin.swift must be one of {} (got `{swift_builtin}`)",
                        join_allowed_values(SWIFT_BUILTIN_TEMPLATE_VALUES)
                    ))
                    .with_job(job_name.to_owned())
                    .with_hint(format!(
                        "use one of: {}",
                        join_allowed_values(SWIFT_BUILTIN_TEMPLATE_VALUES)
                    )),
                );
            }
        }
    }
}

fn join_allowed_values(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceManifest {
    version: u32,
    workspace: RawWorkspaceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceSettings {
    members: Vec<String>,
    #[serde(default)]
    defaults: RawWorkspaceDefaults,
    #[serde(default)]
    member_overrides: BTreeMap<String, RawWorkspaceMemberOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceDefaults {
    #[serde(default)]
    jobs: BTreeMap<String, RawWorkspaceJobDefaults>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceJobDefaults {
    #[serde(default)]
    template: TemplateConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceMemberOverride {
    jobs: Option<Vec<String>>,
}

impl RawWorkspaceManifest {
    fn into_workspace(self) -> WorkspaceConfig {
        WorkspaceConfig {
            version: self.version,
            workspace: self.workspace.into_workspace(),
        }
    }
}

impl RawWorkspaceSettings {
    fn into_workspace(self) -> WorkspaceSettings {
        WorkspaceSettings {
            members: self.members,
            defaults: self.defaults.into_workspace(),
            member_overrides: self
                .member_overrides
                .into_iter()
                .map(|(member, override_config)| (member, override_config.into_workspace()))
                .collect(),
        }
    }
}

impl RawWorkspaceDefaults {
    fn into_workspace(self) -> WorkspaceDefaults {
        WorkspaceDefaults {
            jobs: self
                .jobs
                .into_iter()
                .map(|(job_name, defaults)| (job_name, defaults.into_workspace()))
                .collect(),
        }
    }
}

impl RawWorkspaceJobDefaults {
    fn into_workspace(self) -> WorkspaceJobDefaults {
        WorkspaceJobDefaults {
            template: self.template,
        }
    }
}

impl RawWorkspaceMemberOverride {
    fn into_workspace(self) -> WorkspaceMemberOverride {
        WorkspaceMemberOverride {
            jobs: self.jobs.unwrap_or_default(),
        }
    }
}

impl WorkspaceDefaults {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}
