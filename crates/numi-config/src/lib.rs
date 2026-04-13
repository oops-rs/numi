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
    DEFAULT_ACCESS_LEVEL, DEFAULT_BUNDLE_MODE, DEFAULT_INCREMENTAL, DefaultsConfig,
    INPUT_KIND_VALUES, InputConfig, JobConfig, TemplateConfig,
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
            && defaults.template.path.is_some()
        {
            job.template.path = defaults
                .template
                .path
                .as_deref()
                .map(|path| rebase_workspace_template_path(workspace_root, member_root, path));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn create_temp_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("numi-config-{label}-{nanos}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }

        fs::write(path, contents).expect("file should be written");
    }

    #[test]
    fn parses_unified_single_config_manifest() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect("single-config manifest should parse");

        match manifest {
            Manifest::Config(config) => {
                assert_eq!(config.version, 1);
                assert_eq!(config.jobs.len(), 1);
                assert_eq!(config.jobs[0].name, "assets");
            }
            other => panic!("expected config manifest, got {other:?}"),
        }
    }

    #[test]
    fn parses_unified_workspace_manifest() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"

[workspace.member_overrides.AppUI]
jobs = ["assets", "l10n"]
"#,
        )
        .expect("workspace manifest should parse");

        match manifest {
            Manifest::Workspace(workspace) => {
                assert_eq!(workspace.version, 1);
                assert_eq!(workspace.workspace.members, vec!["AppUI", "Core"]);
                assert_eq!(
                    workspace
                        .members()
                        .iter()
                        .map(|member| member.config.as_str())
                        .collect::<Vec<_>>(),
                    vec!["AppUI/numi.toml", "Core/numi.toml"]
                );
                assert_eq!(
                    workspace.workspace.defaults.jobs["l10n"]
                        .template
                        .builtin
                        .as_ref()
                        .and_then(|builtin| builtin.language.as_deref()),
                    Some("swift")
                );
                assert!(
                    workspace.workspace.defaults.jobs["l10n"]
                        .template
                        .builtin
                        .as_ref()
                        .and_then(|builtin| builtin.name.as_deref())
                        .is_none()
                );
                assert_eq!(
                    workspace.workspace.member_overrides["AppUI"].jobs,
                    Some(vec!["assets".to_string(), "l10n".to_string()])
                );
            }
            other => panic!("expected workspace manifest, got {other:?}"),
        }
    }

    #[test]
    fn rejects_legacy_workspace_default_builtin_shape_with_migration_hint() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
builtin = "l10n"
"#,
        )
        .expect_err("legacy workspace default builtin shape should fail");

        let message = error.to_string();
        assert!(message.contains("legacy flat built-in template syntax is no longer supported"));
        assert!(
            message.contains("[workspace.defaults.jobs.l10n.template.builtin] language = \"...\"")
        );
    }

    #[test]
    fn sniffs_inline_table_workspace_manifest_as_workspace_like() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
workspace={members=["AppUI"]}
"#
            ),
            ManifestKindSniff::WorkspaceLike
        );
    }

    #[test]
    fn sniffs_broken_workspace_like_manifests_without_classifying_them_as_unknown() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
[workspace]
members = [
"#
            ),
            ManifestKindSniff::BrokenWorkspaceLike
        );
    }

    #[test]
    fn sniffs_broken_mixed_manifests_as_mixed() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
jobs = {}
members = [
"#
            ),
            ManifestKindSniff::Mixed
        );
    }

    #[test]
    fn sniffs_legacy_top_level_members_manifest_as_workspace_like() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
members = [{ config = "AppUI/numi.toml" }]
"#
            ),
            ManifestKindSniff::WorkspaceLike
        );
    }

    #[test]
    fn sniffs_mixed_manifests_without_fully_loading_them() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
jobs = {}
members = [{ config = "AppUI/numi.toml" }]
"#
            ),
            ManifestKindSniff::Mixed
        );
    }

    #[test]
    fn sniffs_workspaceish_unparsable_manifests_as_broken_workspace_like() {
        assert_eq!(
            sniff_manifest_kind_str(
                r#"
version = 1
members = [
"#
            ),
            ManifestKindSniff::BrokenWorkspaceLike
        );
    }

    #[test]
    fn rejects_manifest_that_mixes_jobs_and_workspace() {
        let error = parse_manifest_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[workspace]
members = ["AppUI"]
"#,
        )
        .expect_err("mixed manifest should be rejected");

        assert!(
            error
                .to_string()
                .contains("must not define both `jobs` and `workspace`")
        );
    }

    #[test]
    fn rejects_workspace_members_that_look_like_config_paths() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI/numi.toml"]
"#,
        )
        .expect_err("workspace members that look like config paths should be rejected");

        assert!(
            error
                .to_string()
                .contains("workspace.members entries must be relative member roots")
        );
    }

    #[test]
    fn accepts_workspace_members_whose_names_end_with_toml() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["App.toml"]
"#,
        )
        .expect("non-config .toml member root should parse");

        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        assert_eq!(workspace.workspace.members, vec!["App.toml"]);
    }

    #[test]
    fn rejects_workspace_members_that_normalize_to_the_same_root() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["App", "App/"]
"#,
        )
        .expect_err("equivalent workspace member roots should be rejected");

        assert!(
            error
                .to_string()
                .contains("workspace.members entries must be unique")
        );
    }

    #[test]
    fn parses_workspace_defaults_job_template_shape() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.l10n.template]
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"
"#,
        )
        .expect("workspace defaults template should parse");

        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        assert_eq!(
            workspace.workspace.defaults.jobs["l10n"]
                .template
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.language.as_deref()),
            Some("swift")
        );
        assert!(
            workspace.workspace.defaults.jobs["l10n"]
                .template
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.name.as_deref())
                .is_none()
        );
    }

    #[test]
    fn rejects_workspace_member_overrides_for_undeclared_members() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.member_overrides.Core]
jobs = ["assets"]
"#,
        )
        .expect_err("undeclared member override should fail validation");

        assert!(
            error
                .to_string()
                .contains("workspace.member_overrides keys must match declared members")
        );
    }

    #[test]
    fn rejects_normalized_duplicate_workspace_member_overrides() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["App"]

[workspace.member_overrides.App]
jobs = ["assets"]

[workspace.member_overrides."App/"]
jobs = ["l10n"]
"#,
        )
        .expect_err("normalized duplicate override keys should fail validation");

        assert!(
            error
                .to_string()
                .contains("workspace.member_overrides keys must be unique")
        );
    }

    #[test]
    fn rejects_invalid_workspace_default_job_template_shape() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n.stencil"
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
        )
        .expect_err("invalid workspace default template should fail validation");

        assert!(
            error
                .to_string()
                .contains("workspace default job template builtin must not set `name`")
        );
    }

    #[test]
    fn rejects_mixed_workspace_default_path_and_builtin_language() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
        )
        .expect_err("mixed workspace default template sources should fail");

        let message = error.to_string();
        assert!(message.contains("workspace default job template must set exactly one source"));
        assert!(message.contains("remove either `path` or `builtin.language`"));
    }

    #[test]
    fn workspace_member_config_path_joins_member_root_with_numi_toml() {
        assert_eq!(
            workspace_member_config_path(Path::new("/tmp/workspace"), "AppUI"),
            PathBuf::from("/tmp/workspace/AppUI/numi.toml")
        );
    }

    #[test]
    fn workspace_defaults_can_supply_builtin_language() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
        )
        .expect("workspace should parse");
        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        let member_config = toml::from_str::<Config>(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
name = "assets"
"#,
        )
        .expect("member config should deserialize");

        let resolved = resolve_workspace_member_config(
            Path::new("/tmp/workspace"),
            &workspace,
            "AppUI",
            &member_config,
        )
        .expect("workspace defaults should resolve");

        let builtin = resolved.jobs[0]
            .template
            .builtin
            .as_ref()
            .expect("builtin should exist");
        assert_eq!(builtin.language.as_deref(), Some("objc"));
        assert_eq!(builtin.name.as_deref(), Some("assets"));
    }

    #[test]
    fn rejects_workspace_default_builtin_name() {
        let error = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
        )
        .expect_err("workspace default builtin name should fail validation");

        assert!(
            error
                .to_string()
                .contains("workspace default job template builtin must not set `name`")
        );
    }

    #[test]
    fn workspace_defaults_path_inherit_for_empty_member_template() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
"#,
        )
        .expect("workspace should parse");
        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        let member_config = toml::from_str::<Config>(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
"#,
        )
        .expect("member config should deserialize");

        let resolved = resolve_workspace_member_config(
            Path::new("/tmp/workspace"),
            &workspace,
            "AppUI",
            &member_config,
        )
        .expect("workspace defaults should resolve");
        let expected_path = PathBuf::from("..")
            .join("Templates")
            .join("assets.stencil")
            .display()
            .to_string();

        assert_eq!(
            resolved.jobs[0].template.path.as_deref(),
            Some(expected_path.as_str())
        );
        assert!(resolved.jobs[0].template.builtin.is_none());
    }

    #[test]
    fn workspace_defaults_path_inherit_handles_nested_member_roots_with_native_separators() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["apps/AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
"#,
        )
        .expect("workspace should parse");
        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        let member_config = toml::from_str::<Config>(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
"#,
        )
        .expect("member config should deserialize");

        let resolved = resolve_workspace_member_config(
            Path::new("/tmp/workspace"),
            &workspace,
            "apps/AppUI",
            &member_config,
        )
        .expect("workspace defaults should resolve");
        let expected_path = PathBuf::from("..")
            .join("..")
            .join("Templates")
            .join("assets.stencil")
            .display()
            .to_string();

        assert_eq!(
            resolved.jobs[0].template.path.as_deref(),
            Some(expected_path.as_str())
        );
    }

    #[test]
    fn workspace_defaults_missing_member_builtin_name_remains_invalid() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
        )
        .expect("workspace should parse");
        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        let member_config = toml::from_str::<Config>(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
"#,
        )
        .expect("member config should deserialize");

        let error = resolve_workspace_member_config(
            Path::new("/tmp/workspace"),
            &workspace,
            "AppUI",
            &member_config,
        )
        .expect_err("missing builtin name should remain invalid after resolution");

        let message = error
            .into_iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(message.contains("job template builtin must set both language and name"));
        assert!(message.contains("job template must set exactly one source"));
    }

    #[test]
    fn job_level_builtin_language_overrides_workspace_default_language() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
        )
        .expect("workspace should parse");
        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        let member_config = toml::from_str::<Config>(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect("member config should deserialize");

        let resolved = resolve_workspace_member_config(
            Path::new("/tmp/workspace"),
            &workspace,
            "AppUI",
            &member_config,
        )
        .expect("workspace defaults should resolve");

        let builtin = resolved.jobs[0]
            .template
            .builtin
            .as_ref()
            .expect("builtin should exist");
        assert_eq!(builtin.language.as_deref(), Some("swift"));
        assert_eq!(builtin.name.as_deref(), Some("swiftui-assets"));
    }

    #[test]
    fn parses_defaults_and_jobs_from_toml() {
        let config = parse_str(
            r#"
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/l10n.stencil"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.access_level.as_deref(), Some("internal"));
        assert_eq!(config.defaults.bundle.mode.as_deref(), Some("module"));
        assert_eq!(config.jobs.len(), 2);
        assert_eq!(config.jobs[0].name, "assets");
        assert_eq!(config.jobs[0].inputs.len(), 1);
        assert_eq!(
            config.jobs[0]
                .template
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.name.as_deref()),
            Some("swiftui-assets")
        );
        assert_eq!(
            config.jobs[1].template.path.as_deref(),
            Some("Templates/l10n.stencil")
        );
    }

    #[test]
    fn parses_namespaced_builtin_template_config() {
        let config = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect("config should parse");

        assert_eq!(
            config.jobs[0]
                .template
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.name.as_deref()),
            Some("swiftui-assets")
        );
    }

    #[test]
    fn parses_builtin_template_language_and_name() {
        let config = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
        )
        .expect("config should parse");

        let builtin = config.jobs[0]
            .template
            .builtin
            .as_ref()
            .expect("builtin should exist");
        assert_eq!(builtin.language.as_deref(), Some("objc"));
        assert_eq!(builtin.name.as_deref(), Some("assets"));
    }

    #[test]
    fn rejects_builtin_template_language_without_name() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
"#,
        )
        .expect_err("partial builtin language should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template builtin must set both language and name"));
        assert!(
            message
                .contains("set `[jobs.assets.template.builtin] language = \"...\" name = \"...\"`")
        );
    }

    #[test]
    fn rejects_builtin_template_name_without_language() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
name = "swiftui-assets"
"#,
        )
        .expect_err("partial builtin name should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template builtin must set both language and name"));
        assert!(
            message
                .contains("set `[jobs.assets.template.builtin] language = \"...\" name = \"...\"`")
        );
    }

    #[test]
    fn rejects_unknown_builtin_language() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "kotlin"
name = "assets"
"#,
        )
        .expect_err("unknown builtin language should fail");

        let message = error.to_string();
        assert!(message.contains("jobs.assets.template.builtin.language must be one of"));
    }

    #[test]
    fn rejects_legacy_swift_builtin_namespace_shape() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"
"#,
        )
        .expect_err("legacy builtin namespace shape should fail");

        let message = error.to_string();
        assert!(message.contains("unknown field `swift`"));
    }

    #[test]
    fn parses_incremental_generation_settings_from_defaults_and_job() {
        let config = parse_str(
            r#"
version = 1

[defaults]
incremental = false

[jobs.assets]
output = "Generated/Assets.swift"
incremental = true

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.defaults.incremental, Some(false));
        assert_eq!(config.jobs[0].incremental, Some(true));
    }

    #[test]
    fn rejects_template_configs_that_set_both_builtin_and_path() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
path = "Templates/assets.stencil"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect_err("config with both template sources should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template must set exactly one source"));
        assert!(message.contains("set either `[jobs.assets.template.builtin] language = \"...\" name = \"...\"` or `[jobs.assets.template] path = \"...\"`"));
    }

    #[test]
    fn rejects_empty_builtin_template_namespace() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
"#,
        )
        .expect_err("empty built-in namespace should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template must set exactly one source"));
        assert!(message.contains("set either `[jobs.assets.template.builtin] language = \"...\" name = \"...\"` or `[jobs.assets.template] path = \"...\"`"));
    }

    #[test]
    fn rejects_legacy_jobs_array_syntax_with_migration_hint() {
        let error = parse_str(
            r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect_err("legacy jobs array syntax should fail with a migration diagnostic");

        let message = error.to_string();
        assert!(message.contains("legacy `[[jobs]]` syntax is no longer supported"));
        assert!(message.contains("[jobs.assets]"));
        assert!(message.contains("[[jobs.assets.inputs]]"));
    }

    #[test]
    fn rejects_legacy_flat_builtin_template_shape_with_migration_hint() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
builtin = "swiftui-assets"
"#,
        )
        .expect_err("legacy flat builtin syntax should fail with a migration diagnostic");

        let message = error.to_string();
        assert!(message.contains("legacy flat built-in template syntax is no longer supported"));
        assert!(
            message.contains("[jobs.assets.template.builtin] language = \"...\" name = \"...\"")
        );
        assert!(!message.contains("invalid type: string"));
    }

    #[test]
    fn rejects_empty_builtin_template_name() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = ""
"#,
        )
        .expect_err("empty builtin name should fail validation");

        let message = error.to_string();
        assert!(message.contains("jobs.assets.template.builtin.name must be one of"));
        assert!(message.contains("got ``"));
    }

    #[test]
    fn rejects_unknown_builtin_template_name() {
        let error = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "not-a-real-template"
"#,
        )
        .expect_err("unknown builtin name should fail validation");

        let message = error.to_string();
        assert!(message.contains("jobs.assets.template.builtin.name must be one of"));
        assert!(message.contains("not-a-real-template"));
    }

    #[test]
    fn accepts_path_template_with_empty_builtin_table() {
        let config = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
path = "Templates/assets.jinja"

[jobs.assets.template.builtin]
"#,
        )
        .expect("path template should remain valid when an empty builtin table is present");

        assert_eq!(
            config.jobs[0].template.path.as_deref(),
            Some("Templates/assets.jinja")
        );
        assert!(
            config.jobs[0]
                .template
                .builtin
                .as_ref()
                .is_some_and(|builtin| builtin.is_empty())
        );
    }

    #[test]
    fn serializing_empty_builtin_namespace_omits_builtin_table() {
        let config = Config {
            version: 1,
            defaults: DefaultsConfig::default(),
            jobs: vec![JobConfig {
                name: "assets".to_string(),
                output: "Generated/Assets.swift".to_string(),
                access_level: None,
                incremental: None,
                bundle: BundleConfig::default(),
                inputs: vec![InputConfig {
                    kind: "xcassets".to_string(),
                    path: "Resources/Assets.xcassets".to_string(),
                }],
                template: TemplateConfig {
                    builtin: Some(BuiltinTemplateConfig {
                        language: None,
                        name: None,
                    }),
                    path: None,
                },
            }],
        };

        let serialized = toml::to_string(&config).expect("config should serialize");

        assert!(!serialized.contains("[jobs.assets.template]"));
        assert!(!serialized.contains("[jobs.assets.template.builtin]"));
        assert!(!serialized.contains("swift ="));
    }

    #[test]
    fn serializing_workspace_member_without_jobs_omits_jobs_field() {
        let workspace = WorkspaceConfig {
            version: 1,
            workspace: WorkspaceSettings {
                members: vec!["App".to_string(), "Core".to_string()],
                defaults: WorkspaceDefaults::default(),
                member_overrides: std::collections::BTreeMap::from([
                    ("App".to_string(), WorkspaceMemberOverride { jobs: None }),
                    (
                        "Core".to_string(),
                        WorkspaceMemberOverride {
                            jobs: Some(vec!["assets".to_string()]),
                        },
                    ),
                ]),
            },
        };

        let serialized = toml::to_string(&workspace).expect("workspace should serialize");

        assert!(!serialized.contains("jobs = []"));
        assert!(serialized.contains("[workspace.member_overrides.Core]"));
        assert!(serialized.contains("jobs = [\"assets\"]"));

        let reparsed = toml::from_str::<WorkspaceConfig>(&serialized)
            .expect("serialized workspace should parse back");
        assert_eq!(reparsed, workspace);
    }

    #[test]
    fn rejects_invalid_v1_enum_values() {
        let error = parse_str(
            r#"
version = 1

[defaults]
access_level = "private"

[defaults.bundle]
mode = "feature"

[jobs.assets]
output = "Generated/Assets.swift"
access_level = "open"

[[jobs.assets.inputs]]
type = "images"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect_err("invalid v1 enum values should fail validation");

        let message = error.to_string();
        assert!(message.contains("defaults.access_level"));
        assert!(message.contains("defaults.bundle.mode"));
        assert!(message.contains("[job: assets]"));
        assert!(message.contains("jobs.inputs[].type"));
    }

    #[test]
    fn accepts_files_as_valid_input_kind() {
        let config = parse_str(
            r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources"

[jobs.files.template]
path = "Templates/files.stencil"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.jobs.len(), 1);
        assert_eq!(config.jobs[0].inputs[0].kind, "files");
    }

    #[test]
    fn accepts_fonts_as_valid_input_kind() {
        let config = parse_str(
            r#"
version = 1

[jobs.fonts]
output = "Generated/Fonts.swift"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/Fonts"

[jobs.fonts.template]
path = "Templates/fonts.jinja"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.jobs.len(), 1);
        assert_eq!(config.jobs[0].inputs[0].kind, "fonts");
    }

    #[test]
    fn rejects_unknown_keys_during_parsing() {
        let error = parse_str(
            r#"
version = 1
verison = 2

[defaults]
access_level = "internal"
accessLevel = "public"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
pth = "Resources/Typo.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect_err("unknown keys should fail during parsing");

        match error {
            ConfigError::ParseToml(parse_error) => {
                let message = parse_error.to_string();
                assert!(
                    message.contains("unknown field"),
                    "unexpected parse error: {message}"
                );
            }
            other => panic!("expected parse error for unknown key, got {other:?}"),
        }
    }

    #[test]
    fn resolve_config_materializes_v1_default_values() {
        let config = parse_str(
            r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
        )
        .expect("config should parse");

        let resolved = resolve_config(&config);

        assert_eq!(resolved.defaults.access_level.as_deref(), Some("internal"));
        assert_eq!(resolved.defaults.bundle.mode.as_deref(), Some("module"));
        assert_eq!(resolved.defaults.incremental, Some(true));
        assert!(resolved.jobs[0].bundle.is_empty());
    }

    #[test]
    fn parses_workspace_manifest() {
        let temp_dir = create_temp_dir("parse-workspace-manifest");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[workspace]
members = ["App", "Core"]

[workspace.member_overrides.App]
jobs = ["assets", "l10n"]
"#,
        );

        let loaded =
            load_workspace_from_path(&manifest_path).expect("workspace manifest should parse");

        assert_eq!(loaded.config.version, 1);
        assert_eq!(loaded.config.workspace.members, vec!["App", "Core"]);
        assert_eq!(
            loaded.config.workspace.member_overrides["App"].jobs,
            Some(vec!["assets".to_string(), "l10n".to_string()])
        );
    }

    #[test]
    fn unified_manifest_entrypoint_accepts_legacy_workspace_shape() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets"]
"#,
        )
        .expect("legacy workspace shape should parse through manifest entrypoint");

        let Manifest::Workspace(workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        assert_eq!(workspace.workspace.members, vec!["App"]);
        assert_eq!(workspace.members()[0].config, "App/numi.toml");
        assert_eq!(workspace.members()[0].jobs, vec!["assets"]);
    }

    #[test]
    fn deserializes_legacy_workspace_manifest_into_workspace_config() {
        let workspace = toml::from_str::<WorkspaceConfig>(
            r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets", "l10n"]

[[members]]
config = "Core/numi.toml"
"#,
        )
        .expect("legacy workspace manifest should deserialize into WorkspaceConfig");

        assert_eq!(workspace.workspace.members, vec!["App", "Core"]);
        assert_eq!(
            workspace
                .members()
                .iter()
                .map(|member| member.config.as_str())
                .collect::<Vec<_>>(),
            vec!["App/numi.toml", "Core/numi.toml"]
        );
    }

    #[test]
    fn parses_legacy_workspace_manifest_for_compatibility() {
        let temp_dir = create_temp_dir("parse-legacy-workspace-manifest");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets", "l10n"]

[[members]]
config = "Core/numi.toml"
"#,
        );

        let loaded =
            load_workspace_from_path(&manifest_path).expect("legacy workspace should parse");

        assert_eq!(loaded.config.workspace.members, vec!["App", "Core"]);
        assert_eq!(
            loaded
                .config
                .members()
                .iter()
                .map(|member| member.config.as_str())
                .collect::<Vec<_>>(),
            vec!["App/numi.toml", "Core/numi.toml"]
        );
        assert_eq!(loaded.config.members()[0].jobs, vec!["assets", "l10n"]);
    }

    #[test]
    fn rejects_workspace_root_members_under_unified_manifest_model() {
        for member in [".", "./"] {
            let error = parse_manifest_str(&format!(
                r#"
version = 1

[workspace]
members = ["{member}"]
"#
            ))
            .expect_err("workspace root member should be rejected");

            let message = error.to_string();
            assert!(
                message.contains("workspace.members entries must not point at the workspace root"),
                "message was: {message}"
            );
            assert!(
                message.contains(
                    "declare member directories like `AppUI` or `Core`; the workspace root numi.toml carries `[workspace]`, not a member config path"
                ),
                "message was: {message}"
            );
        }
    }

    #[test]
    fn workspace_members_are_derived_from_current_workspace_state() {
        let manifest = parse_manifest_str(
            r#"
version = 1

[workspace]
members = ["App"]
"#,
        )
        .expect("workspace manifest should parse");

        let Manifest::Workspace(mut workspace) = manifest else {
            panic!("expected workspace manifest");
        };

        assert_eq!(workspace.members()[0].jobs, Vec::<String>::new());

        workspace.workspace.member_overrides.insert(
            "App".to_string(),
            WorkspaceMemberOverride {
                jobs: Some(vec!["assets".to_string(), "l10n".to_string()]),
            },
        );

        assert_eq!(workspace.members()[0].jobs, vec!["assets", "l10n"]);
    }

    #[test]
    fn rejects_duplicate_workspace_members() {
        let temp_dir = create_temp_dir("duplicate-workspace-members");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[workspace]
members = ["App", "App"]
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("duplicate workspace members should fail validation");

        assert!(
            error
                .to_string()
                .contains("workspace.members entries must be unique")
        );
    }

    #[test]
    fn rejects_empty_workspace_members() {
        let temp_dir = create_temp_dir("empty-workspace-members");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(&manifest_path, "version = 1\n[workspace]\n");

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("workspace manifest requires at least one member");

        assert!(
            error
                .to_string()
                .contains("workspace must declare at least one member")
        );
    }

    #[test]
    fn rejects_unsupported_workspace_version() {
        let temp_dir = create_temp_dir("unsupported-workspace-version");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(
            &manifest_path,
            r#"
version = 2

[workspace]
members = ["App"]
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("workspace manifest should reject unsupported versions");

        let message = error.to_string();
        assert!(message.contains("workspace version must be 1"));
        assert!(message.contains("set `version = 1` in numi.toml"));
        assert!(!message.contains("numi-workspace.toml"));
    }

    #[test]
    fn rejects_empty_and_duplicate_workspace_jobs() {
        let temp_dir = create_temp_dir("invalid-workspace-jobs");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[workspace]
members = ["App", "Core"]

[workspace.member_overrides.App]
jobs = []

[workspace.member_overrides.Core]
jobs = ["assets", "assets"]
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("workspace jobs should reject empty and duplicate selections");

        let message = error.to_string();
        assert!(message.contains("workspace member override jobs must not be empty"));
        assert!(message.contains("workspace member override jobs must be unique"));
    }

    #[test]
    fn discovers_workspace_manifest_in_ancestors_only() {
        let ancestor_root = create_temp_dir("workspace-discovery-ancestor");
        let ancestor_manifest = ancestor_root.join("numi.toml");
        write_file(
            &ancestor_manifest,
            "version = 1\n[workspace]\nmembers = [\"App\"]\n",
        );

        let nested = ancestor_root.join("apps/ios/App");
        fs::create_dir_all(&nested).expect("nested directory should exist");

        let discovered = discover_workspace_ancestor(&nested, None)
            .expect("ancestor workspace manifest should be discovered");
        assert_eq!(
            discovered,
            ancestor_manifest
                .canonicalize()
                .expect("manifest path should canonicalize")
        );

        let descendant_root = create_temp_dir("workspace-discovery-descendant");
        write_file(
            &descendant_root.join("apps/App/numi.toml"),
            "version = 1\n[workspace]\nmembers = [\"apps/App\"]\n",
        );

        let error = discover_workspace_ancestor(&descendant_root, None)
            .expect_err("descendant workspace manifests should not be discovered");
        match error {
            DiscoveryError::NotFound { start_dir } => assert_eq!(
                start_dir,
                descendant_root
                    .canonicalize()
                    .expect("path should canonicalize")
            ),
            other => panic!("expected not found discovery error, got {other:?}"),
        }
    }

    #[test]
    fn workspace_load_errors_use_workspace_manifest_language() {
        let missing = create_temp_dir("workspace-load-error").join("missing-workspace.toml");
        let error = load_workspace_from_path(&missing)
            .expect_err("missing workspace manifest should return a read error");
        let message = error.to_string();
        assert!(message.contains("workspace numi.toml"));
        assert!(!message.contains("failed to read config"));

        let temp_dir = create_temp_dir("workspace-parse-error");
        let manifest_path = temp_dir.join("numi.toml");
        write_file(&manifest_path, "not = [valid");

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("invalid workspace manifest should return a parse error");
        let message = error.to_string();
        assert!(message.contains("workspace numi.toml TOML"));
        assert!(!message.contains("config TOML"));
    }

    #[test]
    fn workspace_discovery_errors_use_workspace_manifest_language() {
        let temp_dir = create_temp_dir("workspace-discovery-not-found");
        let error = discover_workspace_ancestor(&temp_dir, None)
            .expect_err("missing workspace manifest should be reported");
        let message = error.to_string();
        assert!(message.contains("No configuration file found from"));
        assert!(message.contains("numi config locate --config <path>"));

        let explicit = temp_dir.join("missing-workspace.toml");
        let error = discover_workspace_ancestor(&temp_dir, Some(&explicit))
            .expect_err("missing explicit workspace manifest should be reported");
        assert!(error.to_string().contains("config file not found"));
    }

    #[test]
    fn discovers_config_manifest_in_ancestors_only() {
        let ancestor_root = create_temp_dir("config-discovery-ancestor");
        let ancestor_manifest = ancestor_root.join("numi.toml");
        write_file(&ancestor_manifest, "version = 1\njobs = []\n");

        let nested = ancestor_root.join("apps/ios/App");
        fs::create_dir_all(&nested).expect("nested directory should exist");

        let discovered =
            discover_config(&nested, None).expect("ancestor config manifest should be discovered");
        assert_eq!(
            discovered,
            ancestor_manifest
                .canonicalize()
                .expect("manifest path should canonicalize")
        );

        let descendant_root = create_temp_dir("config-discovery-descendant");
        write_file(
            &descendant_root.join("apps/App/numi.toml"),
            "version = 1\njobs = []\n",
        );

        let error = discover_config(&descendant_root, None)
            .expect_err("descendant config manifests should not be discovered");
        match error {
            DiscoveryError::NotFound { start_dir } => assert_eq!(
                start_dir,
                descendant_root
                    .canonicalize()
                    .expect("path should canonicalize")
            ),
            other => panic!("expected not found discovery error, got {other:?}"),
        }
    }
}
