use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use numi_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, DiscoveryError};

pub const WORKSPACE_FILE_NAME: &str = "numi-workspace.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub members: Vec<WorkspaceMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMember {
    pub config: String,
    #[serde(default)]
    pub jobs: Vec<String>,
}

#[derive(Debug)]
pub struct LoadedWorkspace {
    pub path: PathBuf,
    pub config: WorkspaceConfig,
}

pub fn load_workspace_from_path(path: &Path) -> Result<LoadedWorkspace, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
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
) -> Result<PathBuf, DiscoveryError> {
    crate::discovery::discover_named_file(start_dir, explicit_path, WORKSPACE_FILE_NAME)
}

fn parse_workspace_str(input: &str) -> Result<WorkspaceConfig, ConfigError> {
    let raw: RawWorkspaceConfig = toml::from_str(input).map_err(ConfigError::ParseToml)?;
    let diagnostics = validate_workspace(&raw);

    if diagnostics.is_empty() {
        Ok(raw.into_workspace())
    } else {
        Err(ConfigError::Invalid(diagnostics))
    }
}

fn validate_workspace(config: &RawWorkspaceConfig) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if config.version != 1 {
        diagnostics.push(
            Diagnostic::error("workspace version must be 1")
                .with_hint("set `version = 1` in numi-workspace.toml"),
        );
    }

    if config.members.is_empty() {
        diagnostics.push(
            Diagnostic::error("workspace must declare at least one member")
                .with_hint("add at least one `[[members]]` entry"),
        );
    }

    let mut member_configs = BTreeSet::new();

    for member in &config.members {
        if !member_configs.insert(member.config.as_str()) {
            diagnostics.push(
                Diagnostic::error("members[].config must be unique")
                    .with_hint("each workspace member must point at a different config path")
                    .with_path(PathBuf::from(&member.config)),
            );
        }

        let Some(job_list) = member.jobs.as_ref() else {
            continue;
        };

        if job_list.is_empty() {
            diagnostics.push(
                Diagnostic::error("members[].jobs must not be empty when present")
                    .with_hint("omit `jobs` to select all jobs, or provide at least one job name")
                    .with_path(PathBuf::from(&member.config)),
            );
            continue;
        }

        let mut job_names = BTreeSet::new();
        for job in job_list {
            if !job_names.insert(job.as_str()) {
                diagnostics.push(
                    Diagnostic::error("members[].jobs must not contain duplicates")
                        .with_hint("remove duplicate job names from the workspace member")
                        .with_job(job.clone())
                        .with_path(PathBuf::from(&member.config)),
                );
            }
        }
    }

    diagnostics
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceConfig {
    version: u32,
    members: Vec<RawWorkspaceMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspaceMember {
    config: String,
    jobs: Option<Vec<String>>,
}

impl RawWorkspaceConfig {
    fn into_workspace(self) -> WorkspaceConfig {
        WorkspaceConfig {
            version: self.version,
            members: self
                .members
                .into_iter()
                .map(RawWorkspaceMember::into_workspace)
                .collect(),
        }
    }
}

impl RawWorkspaceMember {
    fn into_workspace(self) -> WorkspaceMember {
        WorkspaceMember {
            config: self.config,
            jobs: self.jobs.unwrap_or_default(),
        }
    }
}
