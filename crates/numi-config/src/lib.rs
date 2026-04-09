mod discovery;
mod model;
mod validate;

use std::{
    fs,
    path::{Path, PathBuf},
};

use numi_diagnostics::Diagnostic;

pub use discovery::{CONFIG_FILE_NAME, DiscoveryError, discover_config};
pub use model::{
    ACCESS_LEVEL_VALUES, BUNDLE_MODE_VALUES, BundleConfig, BuiltinTemplateConfig, Config,
    DEFAULT_ACCESS_LEVEL, DEFAULT_BUNDLE_MODE, DefaultsConfig, INPUT_KIND_VALUES, InputConfig,
    JobConfig, TemplateConfig,
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
    let config: Config = toml::from_str(input).map_err(ConfigError::ParseToml)?;
    let diagnostics = validate::validate_config(&config);

    if diagnostics.is_empty() {
        Ok(config)
    } else {
        Err(ConfigError::Invalid(diagnostics))
    }
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
}
