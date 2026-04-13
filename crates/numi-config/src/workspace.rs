use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use numi_diagnostics::Diagnostic;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ConfigError,
    model::{BUILTIN_TEMPLATE_LANGUAGES, HooksConfig, TemplateConfig},
    validate::{validate_hooks, validate_template},
    workspace_member_config_path,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub workspace: WorkspaceSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawWorkspaceConfig {
    Workspace(RawWorkspaceManifest),
    Legacy(RawLegacyWorkspaceManifest),
}

impl<'de> Deserialize<'de> for WorkspaceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RawWorkspaceConfig::deserialize(deserializer)? {
            RawWorkspaceConfig::Workspace(raw) => Ok(raw.into_config()),
            RawWorkspaceConfig::Legacy(raw) => Ok(raw.into_config()),
        }
    }
}

impl WorkspaceConfig {
    pub fn members(&self) -> Vec<WorkspaceMember> {
        derive_workspace_members(&self.workspace)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMember {
    pub config: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSettings {
    #[serde(default)]
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
    #[serde(default, skip_serializing_if = "HooksConfig::is_empty")]
    pub hooks: HooksConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMemberOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<String>>,
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
                    "failed to read workspace numi.toml {}: {source}",
                    path.display()
                )
            }
            Self::ParseToml(error) => {
                write!(f, "failed to parse workspace numi.toml TOML: {error}")
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

impl From<ConfigError> for WorkspaceError {
    fn from(value: ConfigError) -> Self {
        match value {
            ConfigError::Read { path, source } => Self::Read { path, source },
            ConfigError::ParseToml(error) => Self::ParseToml(error),
            ConfigError::Invalid(diagnostics) => Self::Invalid(diagnostics),
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

pub(crate) fn parse_workspace_str(input: &str) -> Result<WorkspaceConfig, WorkspaceError> {
    let value: toml::Value = toml::from_str(input).map_err(WorkspaceError::ParseToml)?;
    let legacy_diagnostics = detect_legacy_flat_builtin_template_syntax(&value);
    if !legacy_diagnostics.is_empty() {
        return Err(WorkspaceError::Invalid(legacy_diagnostics));
    }

    let config: WorkspaceConfig = value.try_into().map_err(WorkspaceError::ParseToml)?;
    let diagnostics = validate_workspace(&config);

    if diagnostics.is_empty() {
        Ok(config)
    } else {
        Err(WorkspaceError::Invalid(diagnostics))
    }
}

fn detect_legacy_flat_builtin_template_syntax(value: &toml::Value) -> Vec<Diagnostic> {
    let Some(workspace) = value.get("workspace").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let Some(defaults) = workspace.get("defaults").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let Some(jobs) = defaults.get("jobs").and_then(toml::Value::as_table) else {
        return Vec::new();
    };

    jobs.iter()
        .filter_map(|(job_name, job)| {
            let template = job.get("template")?.as_table()?;
            let builtin = template.get("builtin")?;
            let builtin_name = builtin.as_str()?;

            let diagnostic = Diagnostic::error(
                "legacy flat built-in template syntax is no longer supported",
            )
            .with_hint(format!(
                "use `[workspace.defaults.jobs.{job_name}.template.builtin] language = \"...\"` instead; for example, replace `[workspace.defaults.jobs.{job_name}.template] builtin = \"{builtin_name}\"` with `[workspace.defaults.jobs.{job_name}.template.builtin] language = \"swift\"`"
            ))
            .with_job(job_name.to_owned());

            Some(diagnostic)
        })
        .collect()
}

fn validate_workspace(config: &WorkspaceConfig) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if config.version != 1 {
        diagnostics.push(
            Diagnostic::error("workspace version must be 1")
                .with_hint("set `version = 1` in numi.toml"),
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
        let Some(normalized_member) = normalize_member_root(member) else {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be relative member roots")
                    .with_hint("use values like `AppUI` or `Core`, not empty paths")
                    .with_path(PathBuf::from(member)),
            );
            continue;
        };

        if normalized_member == "." {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must not point at the workspace root")
                    .with_hint(
                        "declare member directories like `AppUI` or `Core`; the workspace root numi.toml carries `[workspace]`, not a member config path",
                    )
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

        if !members.insert(normalized_member.clone()) {
            diagnostics.push(
                Diagnostic::error("workspace.members entries must be unique")
                    .with_hint("remove duplicate entries from `workspace.members`")
                    .with_path(PathBuf::from(normalized_member)),
            );
        }
    }

    let mut override_members = BTreeSet::new();

    for (member_path, override_config) in &config.workspace.member_overrides {
        let Some(normalized_member_path) = normalize_member_root(member_path) else {
            diagnostics.push(
                Diagnostic::error("workspace.member_overrides keys must match declared members")
                    .with_hint("use a relative member root that matches `workspace.members`")
                    .with_path(PathBuf::from(member_path)),
            );
            continue;
        };

        if !override_members.insert(normalized_member_path.clone()) {
            diagnostics.push(
                Diagnostic::error("workspace.member_overrides keys must be unique")
                    .with_hint("remove duplicate entries that normalize to the same member root")
                    .with_path(PathBuf::from(normalized_member_path)),
            );
            continue;
        }

        if !members.contains(normalized_member_path.as_str()) {
            diagnostics.push(
                Diagnostic::error("workspace.member_overrides keys must match declared members")
                    .with_hint("add the member to `workspace.members` or remove the override")
                    .with_path(PathBuf::from(normalized_member_path)),
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
        validate_workspace_default_template(
            &mut diagnostics,
            &job_defaults.template,
            &format!("workspace.defaults.jobs.{job_name}.template"),
            Some(job_name.as_str()),
        );
        validate_hooks(
            &mut diagnostics,
            &job_defaults.hooks,
            "workspace default job hook",
            &format!("workspace.defaults.jobs.{job_name}.hooks"),
            Some(job_name.as_str()),
        );
    }

    diagnostics
}

fn validate_workspace_default_template(
    diagnostics: &mut Vec<Diagnostic>,
    template: &TemplateConfig,
    field_path: &str,
    job: Option<&str>,
) {
    if template.is_empty() {
        return;
    }

    let Some(builtin) = template.builtin.as_ref() else {
        validate_template(
            diagnostics,
            template,
            "workspace default job template",
            field_path,
            job,
        );
        return;
    };

    if builtin.name.is_some() {
        let diagnostic = Diagnostic::error(
            "workspace default job template builtin must not set `name`",
        )
        .with_hint(
            "workspace defaults only inherit builtin language; set the job-level builtin name instead",
        );
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
        return;
    }

    if template.path.is_some() && builtin.language.is_some() {
        let diagnostic = Diagnostic::error(
            "workspace default job template must set exactly one source",
        )
        .with_hint(
            "remove either `path` or `builtin.language` from `[workspace.defaults.jobs.<job>.template]`",
        );
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
        return;
    }

    if let Some(language) = builtin.language.as_deref() {
        validate_workspace_default_builtin_language(diagnostics, language, field_path, job);
        return;
    }

    validate_template(
        diagnostics,
        template,
        "workspace default job template",
        field_path,
        job,
    );
}

fn validate_workspace_default_builtin_language(
    diagnostics: &mut Vec<Diagnostic>,
    language: &str,
    field_path: &str,
    job: Option<&str>,
) {
    if !BUILTIN_TEMPLATE_LANGUAGES.contains(&language) {
        let diagnostic = Diagnostic::error(format!(
            "{field_path}.builtin.language must be one of {} (got `{language}`)",
            BUILTIN_TEMPLATE_LANGUAGES
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .with_hint(format!(
            "use one of: {}",
            BUILTIN_TEMPLATE_LANGUAGES
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        diagnostics.push(match job {
            Some(job) => diagnostic.with_job(job.to_owned()),
            None => diagnostic,
        });
    }
}

fn is_config_path(member: &str) -> bool {
    Path::new(member)
        .file_name()
        .is_some_and(|file_name| file_name == "numi.toml")
}

fn normalize_member_root(member: &str) -> Option<String> {
    let path = Path::new(member);
    if path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    let mut saw_relative_component = false;
    for component in path.components() {
        match component {
            Component::CurDir => saw_relative_component = true,
            Component::Normal(part) => {
                saw_relative_component = true;
                normalized.push(part);
            }
            _ => return None,
        }
    }

    if normalized.as_os_str().is_empty() {
        saw_relative_component.then(|| String::from("."))
    } else {
        Some(normalized.to_string_lossy().into_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceManifest {
    version: u32,
    workspace: RawWorkspaceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLegacyWorkspaceManifest {
    version: u32,
    members: Vec<RawLegacyWorkspaceMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceSettings {
    #[serde(default)]
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
    #[serde(default)]
    hooks: HooksConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceMemberOverride {
    jobs: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLegacyWorkspaceMember {
    config: String,
    jobs: Option<Vec<String>>,
}

impl RawWorkspaceManifest {
    fn into_config(self) -> WorkspaceConfig {
        WorkspaceConfig {
            version: self.version,
            workspace: self.workspace.into_workspace(),
        }
    }
}

impl RawLegacyWorkspaceManifest {
    fn into_config(self) -> WorkspaceConfig {
        let RawLegacyWorkspaceManifest { version, members } = self;
        let workspace_members = members
            .iter()
            .map(|member| member_root_from_config_path(&member.config))
            .collect::<Vec<_>>();
        let member_overrides = members
            .iter()
            .map(|member| {
                (
                    member_root_from_config_path(&member.config),
                    WorkspaceMemberOverride {
                        jobs: member.jobs.clone(),
                    },
                )
            })
            .collect();
        let workspace = WorkspaceSettings {
            members: workspace_members,
            defaults: WorkspaceDefaults::default(),
            member_overrides,
        };

        WorkspaceConfig { version, workspace }
    }
}

fn derive_workspace_members(workspace: &WorkspaceSettings) -> Vec<WorkspaceMember> {
    workspace
        .members
        .iter()
        .filter_map(|member_root| {
            normalize_member_root(member_root).map(|normalized_member_root| WorkspaceMember {
                config: member_config_path(&normalized_member_root),
                jobs: workspace_member_override_jobs(workspace, &normalized_member_root),
            })
        })
        .collect()
}

fn workspace_member_override_jobs(workspace: &WorkspaceSettings, member_root: &str) -> Vec<String> {
    workspace
        .member_overrides
        .iter()
        .find_map(|(override_member, override_config)| {
            normalize_member_root(override_member).and_then(|normalized| {
                (normalized == member_root).then(|| override_config.jobs.clone())
            })
        })
        .flatten()
        .unwrap_or_default()
}

impl From<RawWorkspaceManifest> for WorkspaceConfig {
    fn from(value: RawWorkspaceManifest) -> Self {
        value.into_config()
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
            hooks: self.hooks,
        }
    }
}

impl RawWorkspaceMemberOverride {
    fn into_workspace(self) -> WorkspaceMemberOverride {
        WorkspaceMemberOverride { jobs: self.jobs }
    }
}

impl WorkspaceDefaults {
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

fn member_config_path(member_root: &str) -> String {
    workspace_member_config_path(Path::new(""), member_root)
        .to_string_lossy()
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .to_owned()
}

fn member_root_from_config_path(config_path: &str) -> String {
    let path = Path::new(config_path);
    path.parent()
        .and_then(|parent| normalize_member_root(parent.to_string_lossy().as_ref()))
        .unwrap_or_else(|| String::from("."))
}
