mod discovery;
mod model;
mod validate;
mod workspace;

use std::{
    fs,
    path::{Path, PathBuf},
};

use numi_diagnostics::Diagnostic;

pub use discovery::{CONFIG_FILE_NAME, DiscoveryError, discover_config};
pub use model::{
    ACCESS_LEVEL_VALUES, BUNDLE_MODE_VALUES, BuiltinTemplateConfig, BundleConfig, Config,
    DEFAULT_ACCESS_LEVEL, DEFAULT_BUNDLE_MODE, DefaultsConfig, INPUT_KIND_VALUES, InputConfig,
    JobConfig, TemplateConfig,
};
pub use workspace::{
    LoadedWorkspace, WORKSPACE_FILE_NAME, WorkspaceConfig, WorkspaceDiscoveryError, WorkspaceError,
    WorkspaceMember, discover_workspace, load_workspace_from_path,
};

#[derive(Debug)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub config: Config,
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

pub fn parse_str(input: &str) -> Result<Config, ConfigError> {
    let value: toml::Value = toml::from_str(input).map_err(ConfigError::ParseToml)?;
    let legacy_diagnostics = detect_legacy_flat_builtin_template_syntax(&value);
    if !legacy_diagnostics.is_empty() {
        return Err(ConfigError::Invalid(legacy_diagnostics));
    }

    let config: Config = value.try_into().map_err(ConfigError::ParseToml)?;
    let diagnostics = validate::validate_config(&config);

    if diagnostics.is_empty() {
        Ok(config)
    } else {
        Err(ConfigError::Invalid(diagnostics))
    }
}

fn detect_legacy_flat_builtin_template_syntax(value: &toml::Value) -> Vec<Diagnostic> {
    value
        .get("jobs")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|job| {
            let template = job.get("template")?.as_table()?;
            let builtin = template.get("builtin")?;
            let builtin_name = builtin.as_str()?;

            let mut diagnostic =
                Diagnostic::error("legacy flat built-in template syntax is no longer supported")
                    .with_hint(format!(
                        "use `[jobs.template.builtin] swift = \"...\"` instead; for example, replace `[jobs.template] builtin = \"{builtin_name}\"` with `[jobs.template.builtin] swift = \"{builtin_name}\"`"
                    ));

            if let Some(job_name) = job.get("name").and_then(toml::Value::as_str) {
                diagnostic = diagnostic.with_job(job_name);
            }

            Some(diagnostic)
        })
        .collect()
}

pub fn load_from_path(path: &Path) -> Result<LoadedConfig, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let config = parse_str(&contents)?;

    Ok(LoadedConfig {
        path: path.to_path_buf(),
        config,
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

    resolved
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
    fn parses_defaults_and_jobs_from_toml() {
        let config = parse_str(
            r#"
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
[jobs.template.builtin]
swift = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
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
                .and_then(|builtin| builtin.swift.as_deref()),
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

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template.builtin]
swift = "swiftui-assets"
"#,
        )
        .expect("config should parse");

        assert_eq!(
            config.jobs[0]
                .template
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.swift.as_deref()),
            Some("swiftui-assets")
        );
    }

    #[test]
    fn rejects_template_configs_that_set_both_builtin_and_path() {
        let error = parse_str(
            r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
path = "Templates/assets.stencil"

[jobs.template.builtin]
swift = "swiftui-assets"
"#,
        )
        .expect_err("config with both template sources should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template must set exactly one source"));
        assert!(message.contains("set either `[jobs.template.builtin] swift = \"...\"` or `[jobs.template] path = \"...\"`"));
    }

    #[test]
    fn rejects_empty_builtin_template_namespace() {
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
"#,
        )
        .expect_err("empty built-in namespace should fail validation");

        let message = error.to_string();
        assert!(message.contains("job template builtin must set exactly one namespace"));
        assert!(message.contains("set `[jobs.template.builtin] swift = \"...\"`"));
    }

    #[test]
    fn rejects_legacy_flat_builtin_template_shape_with_migration_hint() {
        let error = parse_str(
            r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"
"#,
        )
        .expect_err("legacy flat builtin syntax should fail with a migration diagnostic");

        let message = error.to_string();
        assert!(message.contains("legacy flat built-in template syntax is no longer supported"));
        assert!(message.contains("[jobs.template.builtin] swift = \"...\""));
        assert!(!message.contains("invalid type: string"));
    }

    #[test]
    fn rejects_empty_swift_builtin_template_name() {
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
swift = ""
"#,
        )
        .expect_err("empty swift builtin name should fail validation");

        let message = error.to_string();
        assert!(message.contains("jobs.template.builtin.swift must be one of"));
        assert!(message.contains("got ``"));
    }

    #[test]
    fn rejects_unknown_swift_builtin_template_name() {
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
swift = "not-a-real-template"
"#,
        )
        .expect_err("unknown swift builtin name should fail validation");

        let message = error.to_string();
        assert!(message.contains("jobs.template.builtin.swift must be one of"));
        assert!(message.contains("not-a-real-template"));
    }

    #[test]
    fn accepts_path_template_with_empty_builtin_table() {
        let config = parse_str(
            r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
path = "Templates/assets.jinja"

[jobs.template.builtin]
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
                .is_some_and(|builtin| builtin.swift.is_none())
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
                bundle: BundleConfig::default(),
                inputs: vec![InputConfig {
                    kind: "xcassets".to_string(),
                    path: "Resources/Assets.xcassets".to_string(),
                }],
                template: TemplateConfig {
                    builtin: Some(BuiltinTemplateConfig { swift: None }),
                    path: None,
                },
            }],
        };

        let serialized = toml::to_string(&config).expect("config should serialize");

        assert!(!serialized.contains("[jobs.template]"));
        assert!(!serialized.contains("[jobs.template.builtin]"));
        assert!(!serialized.contains("swift ="));
    }

    #[test]
    fn serializing_workspace_member_without_jobs_omits_jobs_field() {
        let workspace = WorkspaceConfig {
            version: 1,
            members: vec![
                WorkspaceMember {
                    config: "App/numi.toml".to_string(),
                    jobs: Vec::new(),
                },
                WorkspaceMember {
                    config: "Core/numi.toml".to_string(),
                    jobs: vec!["assets".to_string()],
                },
            ],
        };

        let serialized = toml::to_string(&workspace).expect("workspace should serialize");

        assert!(!serialized.contains("config = \"App/numi.toml\"\njobs = []"));
        assert!(serialized.contains("config = \"Core/numi.toml\"\njobs = [\"assets\"]"));

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

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"
access_level = "open"

[[jobs.inputs]]
type = "images"
path = "Resources/Assets.xcassets"

[jobs.template]
[jobs.template.builtin]
swift = "swiftui-assets"
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

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources"

[jobs.template]
path = "Templates/files.stencil"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.jobs.len(), 1);
        assert_eq!(config.jobs[0].inputs[0].kind, "files");
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

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
pth = "Resources/Typo.xcassets"

[jobs.template]
[jobs.template.builtin]
swift = "swiftui-assets"
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

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
[jobs.template.builtin]
swift = "swiftui-assets"
"#,
        )
        .expect("config should parse");

        let resolved = resolve_config(&config);

        assert_eq!(resolved.defaults.access_level.as_deref(), Some("internal"));
        assert_eq!(resolved.defaults.bundle.mode.as_deref(), Some("module"));
        assert!(resolved.jobs[0].bundle.is_empty());
    }

    #[test]
    fn parses_workspace_manifest() {
        let temp_dir = create_temp_dir("parse-workspace-manifest");
        let manifest_path = temp_dir.join("numi-workspace.toml");
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
            load_workspace_from_path(&manifest_path).expect("workspace manifest should parse");

        assert_eq!(loaded.config.version, 1);
        assert_eq!(loaded.config.members.len(), 2);
        assert_eq!(loaded.config.members[0].config, "App/numi.toml");
        assert_eq!(loaded.config.members[0].jobs, vec!["assets", "l10n"]);
        assert!(loaded.config.members[1].jobs.is_empty());
    }

    #[test]
    fn rejects_duplicate_workspace_members() {
        let temp_dir = create_temp_dir("duplicate-workspace-members");
        let manifest_path = temp_dir.join("numi-workspace.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[[members]]
config = "App/numi.toml"

[[members]]
config = "App/numi.toml"
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("duplicate workspace members should fail validation");

        assert!(
            error
                .to_string()
                .contains("members[].config must be unique")
        );
    }

    #[test]
    fn rejects_empty_workspace_members() {
        let temp_dir = create_temp_dir("empty-workspace-members");
        let manifest_path = temp_dir.join("numi-workspace.toml");
        write_file(&manifest_path, "version = 1\nmembers = []\n");

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
        let manifest_path = temp_dir.join("numi-workspace.toml");
        write_file(
            &manifest_path,
            r#"
version = 2

[[members]]
config = "App/numi.toml"
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("workspace manifest should reject unsupported versions");

        assert!(error.to_string().contains("workspace version must be 1"));
    }

    #[test]
    fn rejects_empty_and_duplicate_workspace_jobs() {
        let temp_dir = create_temp_dir("invalid-workspace-jobs");
        let manifest_path = temp_dir.join("numi-workspace.toml");
        write_file(
            &manifest_path,
            r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = []

[[members]]
config = "Core/numi.toml"
jobs = ["assets", "assets"]
"#,
        );

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("workspace jobs should reject empty and duplicate selections");

        let message = error.to_string();
        assert!(message.contains("members[].jobs must not be empty when present"));
        assert!(message.contains("members[].jobs must not contain duplicates"));
    }

    #[test]
    fn discovers_workspace_manifest_with_same_rules_as_single_config() {
        let ancestor_root = create_temp_dir("workspace-discovery-ancestor");
        let ancestor_manifest = ancestor_root.join("numi-workspace.toml");
        write_file(
            &ancestor_manifest,
            "version = 1\n[[members]]\nconfig = \"App/numi.toml\"\n",
        );

        let nested = ancestor_root.join("apps/ios/App");
        fs::create_dir_all(&nested).expect("nested directory should exist");

        let discovered = discover_workspace(&nested, None)
            .expect("ancestor workspace manifest should be discovered");
        assert_eq!(
            discovered,
            ancestor_manifest
                .canonicalize()
                .expect("manifest path should canonicalize")
        );

        let descendant_root = create_temp_dir("workspace-discovery-descendant");
        let descendant_manifest = descendant_root.join("apps/App/numi-workspace.toml");
        write_file(
            &descendant_manifest,
            "version = 1\n[[members]]\nconfig = \"apps/App/numi.toml\"\n",
        );

        let discovered = discover_workspace(&descendant_root, None)
            .expect("single descendant workspace manifest should be discovered");
        assert_eq!(
            discovered,
            descendant_manifest
                .canonicalize()
                .expect("manifest path should canonicalize")
        );

        let ambiguous_root = create_temp_dir("workspace-discovery-ambiguous");
        write_file(
            &ambiguous_root.join("apps/App/numi-workspace.toml"),
            "version = 1\n[[members]]\nconfig = \"apps/App/numi.toml\"\n",
        );
        write_file(
            &ambiguous_root.join("packages/Core/numi-workspace.toml"),
            "version = 1\n[[members]]\nconfig = \"packages/Core/numi.toml\"\n",
        );

        let error = discover_workspace(&ambiguous_root, None)
            .expect_err("multiple descendant workspace manifests should be ambiguous");

        match error {
            WorkspaceDiscoveryError::Ambiguous { root, matches } => {
                assert_eq!(
                    root,
                    ambiguous_root
                        .canonicalize()
                        .expect("path should canonicalize")
                );
                assert_eq!(
                    matches,
                    vec![
                        PathBuf::from("apps/App/numi-workspace.toml"),
                        PathBuf::from("packages/Core/numi-workspace.toml"),
                    ]
                );
            }
            other => panic!("expected ambiguous discovery error, got {other:?}"),
        }
    }

    #[test]
    fn workspace_load_errors_use_workspace_manifest_language() {
        let missing = create_temp_dir("workspace-load-error").join("missing-workspace.toml");
        let error = load_workspace_from_path(&missing)
            .expect_err("missing workspace manifest should return a read error");
        let message = error.to_string();
        assert!(message.contains("workspace manifest"));
        assert!(!message.contains("failed to read config"));

        let temp_dir = create_temp_dir("workspace-parse-error");
        let manifest_path = temp_dir.join("numi-workspace.toml");
        write_file(&manifest_path, "not = [valid");

        let error = load_workspace_from_path(&manifest_path)
            .expect_err("invalid workspace manifest should return a parse error");
        let message = error.to_string();
        assert!(message.contains("workspace manifest TOML"));
        assert!(!message.contains("config TOML"));
    }

    #[test]
    fn workspace_discovery_errors_use_workspace_manifest_language() {
        let temp_dir = create_temp_dir("workspace-discovery-not-found");
        let error = discover_workspace(&temp_dir, None)
            .expect_err("missing workspace manifest should be reported");
        let message = error.to_string();
        assert!(message.contains("workspace manifest"));
        assert!(message.contains("numi workspace locate --workspace <path>"));
        assert!(!message.contains("numi config locate --config <path>"));

        let explicit = temp_dir.join("missing-workspace.toml");
        let error = discover_workspace(&temp_dir, Some(&explicit))
            .expect_err("missing explicit workspace manifest should be reported");
        assert!(error.to_string().contains("workspace manifest not found"));
    }

    #[test]
    fn discovers_config_manifest_with_original_rules() {
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
        let descendant_manifest = descendant_root.join("apps/App/numi.toml");
        write_file(&descendant_manifest, "version = 1\njobs = []\n");

        let discovered = discover_config(&descendant_root, None)
            .expect("single descendant config manifest should be discovered");
        assert_eq!(
            discovered,
            descendant_manifest
                .canonicalize()
                .expect("manifest path should canonicalize")
        );

        let ambiguous_root = create_temp_dir("config-discovery-ambiguous");
        write_file(
            &ambiguous_root.join("apps/App/numi.toml"),
            "version = 1\njobs = []\n",
        );
        write_file(
            &ambiguous_root.join("packages/Core/numi.toml"),
            "version = 1\njobs = []\n",
        );

        let error = discover_config(&ambiguous_root, None)
            .expect_err("multiple descendant config manifests should be ambiguous");

        match error {
            DiscoveryError::Ambiguous { root, matches } => {
                assert_eq!(
                    root,
                    ambiguous_root
                        .canonicalize()
                        .expect("path should canonicalize")
                );
                assert_eq!(
                    matches,
                    vec![
                        PathBuf::from("apps/App/numi.toml"),
                        PathBuf::from("packages/Core/numi.toml"),
                    ]
                );
            }
            other => panic!("expected ambiguous discovery error, got {other:?}"),
        }
    }
}
