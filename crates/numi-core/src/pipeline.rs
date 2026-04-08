use camino::Utf8PathBuf;
use numi_config::{BundleConfig, DefaultsConfig, JobConfig};
use numi_diagnostics::Diagnostic;
use numi_ir::{
    GraphMetadata, Metadata, ModuleKind, ResourceGraph, ResourceModule, normalize_scope,
    swift_identifier,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    context::{AssetTemplateContext, ContextError},
    output::{OutputError, WriteOutcome, output_is_stale, write_if_changed_atomic},
    parse_l10n::{ParseL10nError, parse_strings},
    parse_xcassets::{ParseXcassetsError, parse_catalog},
    render::{RenderError, render_builtin, render_path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateReport {
    pub jobs: Vec<JobReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub job_name: String,
    pub output_path: Utf8PathBuf,
    pub outcome: WriteOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckReport {
    UpToDate,
    Stale(Vec<Utf8PathBuf>),
}

#[derive(Debug)]
pub enum GenerateError {
    LoadConfig(numi_config::ConfigError),
    Diagnostics(Vec<Diagnostic>),
    UnsupportedJob {
        job: String,
        detail: String,
    },
    ParseXcassets {
        job: String,
        source: ParseXcassetsError,
    },
    ParseStrings {
        job: String,
        source: ParseL10nError,
    },
    BuildContext {
        job: String,
        source: ContextError,
    },
    Render {
        job: String,
        source: RenderError,
    },
    SerializeContext(serde_json::Error),
    WriteOutput {
        job: String,
        source: OutputError,
    },
    InspectOutput {
        job: String,
        source: OutputError,
    },
    InvalidOutputPath {
        path: PathBuf,
    },
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadConfig(error) => write!(f, "{error}"),
            Self::Diagnostics(diagnostics) => {
                for (index, diagnostic) in diagnostics.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{diagnostic}")?;
                }
                Ok(())
            }
            Self::UnsupportedJob { job, detail } => {
                write!(f, "job `{job}` is not supported yet: {detail}")
            }
            Self::ParseXcassets { job, source } => {
                write!(
                    f,
                    "failed to parse xcassets input for job `{job}`: {source}"
                )
            }
            Self::ParseStrings { job, source } => {
                write!(f, "failed to parse strings input for job `{job}`: {source}")
            }
            Self::BuildContext { job, source } => {
                write!(
                    f,
                    "failed to build render context for job `{job}`: {source}"
                )
            }
            Self::Render { job, source } => {
                write!(f, "failed to render output for job `{job}`: {source}")
            }
            Self::SerializeContext(source) => {
                write!(f, "failed to serialize context as JSON: {source}")
            }
            Self::WriteOutput { job, source } => {
                write!(f, "failed to write output for job `{job}`: {source}")
            }
            Self::InspectOutput { job, source } => {
                write!(f, "failed to inspect output for job `{job}`: {source}")
            }
            Self::InvalidOutputPath { path } => write!(
                f,
                "generated output path {} is not valid UTF-8 and cannot be recorded",
                path.display()
            ),
        }
    }
}

impl std::error::Error for GenerateError {}

pub fn generate(
    config_path: &Path,
    selected_jobs: Option<&[String]>,
) -> Result<GenerateReport, GenerateError> {
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    let config_dir = loaded
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let jobs = numi_config::resolve_selected_jobs(&loaded.config, selected_jobs)
        .map_err(GenerateError::Diagnostics)?;

    let mut reports = Vec::with_capacity(jobs.len());

    for job in jobs {
        reports.push(generate_job(
            &loaded.path,
            config_dir,
            &loaded.config.defaults,
            job,
        )?);
    }

    Ok(GenerateReport { jobs: reports })
}

pub fn dump_context(config_path: &Path, job_name: &str) -> Result<String, GenerateError> {
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    let config_dir = loaded
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let selected_jobs = vec![job_name.to_owned()];
    let jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
        .map_err(GenerateError::Diagnostics)?;
    let job = jobs
        .into_iter()
        .next()
        .expect("selected one job should resolve to one job");

    let context = build_context(&loaded.path, config_dir, &loaded.config.defaults, job)?;
    serde_json::to_string_pretty(&context).map_err(GenerateError::SerializeContext)
}

pub fn check(
    config_path: &Path,
    selected_jobs: Option<&[String]>,
) -> Result<CheckReport, GenerateError> {
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    let config_dir = loaded
        .path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let jobs = numi_config::resolve_selected_jobs(&loaded.config, selected_jobs)
        .map_err(GenerateError::Diagnostics)?;
    let mut stale_paths = Vec::new();

    for job in jobs {
        if let Some(output_path) =
            check_job(&loaded.path, config_dir, &loaded.config.defaults, job)?
        {
            stale_paths.push(output_path);
        }
    }

    if stale_paths.is_empty() {
        Ok(CheckReport::UpToDate)
    } else {
        Ok(CheckReport::Stale(stale_paths))
    }
}

fn generate_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<JobReport, GenerateError> {
    let context = build_context(config_path, config_dir, defaults, job)?;
    let rendered = render_job(config_dir, job, &context)?;

    let output_path = config_dir.join(&job.output);
    let outcome = write_if_changed_atomic(&output_path, &rendered).map_err(|source| {
        GenerateError::WriteOutput {
            job: job.name.clone(),
            source,
        }
    })?;

    Ok(JobReport {
        job_name: job.name.clone(),
        output_path: to_utf8_path(&output_path)?,
        outcome,
    })
}

fn check_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<Option<Utf8PathBuf>, GenerateError> {
    let context = build_context(config_path, config_dir, defaults, job)?;
    let rendered = render_job(config_dir, job, &context)?;
    let output_path = config_dir.join(&job.output);
    let stale = output_is_stale(&output_path, &rendered).map_err(|source| {
        GenerateError::InspectOutput {
            job: job.name.clone(),
            source,
        }
    })?;

    if stale {
        Ok(Some(to_utf8_path(&output_path)?))
    } else {
        Ok(None)
    }
}

fn build_context(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<AssetTemplateContext, GenerateError> {
    let modules = build_modules(config_dir, job)?;
    let _graph = ResourceGraph {
        modules: modules.clone(),
        diagnostics: Vec::new(),
        metadata: GraphMetadata {
            config_path: Some(to_utf8_path(config_path)?),
        },
    };

    let access_level = resolve_access_level(defaults, job);
    let bundle = merged_bundle(defaults, job);
    let bundle_mode = bundle.mode.as_deref().unwrap_or("module");
    validate_bundle_mode(&job.name, bundle_mode, bundle.identifier.as_deref())?;
    AssetTemplateContext::new(
        &job.name,
        &job.output,
        access_level,
        bundle_mode,
        bundle.identifier.as_deref(),
        &modules,
    )
    .map_err(|source| GenerateError::BuildContext {
        job: job.name.clone(),
        source,
    })
}

fn build_modules(config_dir: &Path, job: &JobConfig) -> Result<Vec<ResourceModule>, GenerateError> {
    let mut modules = Vec::new();
    let mut asset_entries = Vec::new();
    let mut duplicate_table_sources = BTreeMap::<String, Utf8PathBuf>::new();
    let mut diagnostics = Vec::new();

    for input in &job.inputs {
        let input_path = config_dir.join(&input.path);

        match input.kind.as_str() {
            "xcassets" => {
                asset_entries.extend(parse_catalog(&input_path).map_err(|source| {
                    GenerateError::ParseXcassets {
                        job: job.name.clone(),
                        source,
                    }
                })?);
            }
            "strings" => {
                let tables =
                    parse_strings(&input_path).map_err(|source| GenerateError::ParseStrings {
                        job: job.name.clone(),
                        source,
                    })?;

                for table in tables {
                    let table_name = table.table_name.clone();
                    if let Some(first_source) = duplicate_table_sources
                        .insert(table_name.clone(), table.source_path.clone())
                    {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "duplicate localization table `{table_name}` from directory-based `.strings` input"
                            ))
                            .with_job(job.name.clone())
                            .with_path(table.source_path.as_std_path())
                            .with_hint(format!(
                                "found `{}` and `{}`; merge these inputs before generation or select a single localized source",
                                first_source,
                                table.source_path
                            )),
                        );
                        continue;
                    }
                    let entries = normalize_scope(&job.name, table.entries)
                        .map_err(GenerateError::Diagnostics)?;
                    modules.push(ResourceModule {
                        id: table_name.clone(),
                        kind: ModuleKind::Strings,
                        name: swift_identifier(&table_name),
                        entries,
                        metadata: Metadata::from([(
                            "tableName".to_string(),
                            Value::String(table_name),
                        )]),
                    });
                }
            }
            other => {
                return Err(GenerateError::UnsupportedJob {
                    job: job.name.clone(),
                    detail: format!("input kind `{other}`"),
                });
            }
        }
    }

    if !asset_entries.is_empty() {
        let entries =
            normalize_scope(&job.name, asset_entries).map_err(GenerateError::Diagnostics)?;
        modules.insert(
            0,
            ResourceModule {
                id: job.name.clone(),
                kind: ModuleKind::Xcassets,
                name: swift_identifier(&job.name),
                entries,
                metadata: Metadata::new(),
            },
        );
    }

    if !diagnostics.is_empty() {
        return Err(GenerateError::Diagnostics(diagnostics));
    }

    Ok(modules)
}

fn render_job(
    config_dir: &Path,
    job: &JobConfig,
    context: &AssetTemplateContext,
) -> Result<String, GenerateError> {
    if let Some(builtin_name) = job.template.builtin.as_deref() {
        return render_builtin(builtin_name, context).map_err(|source| GenerateError::Render {
            job: job.name.clone(),
            source,
        });
    }

    if let Some(template_path) = job.template.path.as_deref() {
        let resolved_path = config_dir.join(template_path);
        return render_path(&resolved_path, config_dir, context).map_err(|source| {
            GenerateError::Render {
                job: job.name.clone(),
                source,
            }
        });
    }

    Err(GenerateError::UnsupportedJob {
        job: job.name.clone(),
        detail: "job template must set a built-in or custom template path".to_string(),
    })
}

fn resolve_access_level<'a>(defaults: &'a DefaultsConfig, job: &'a JobConfig) -> &'a str {
    job.access_level
        .as_deref()
        .or(defaults.access_level.as_deref())
        .unwrap_or("internal")
}

fn validate_bundle_mode(
    job_name: &str,
    mode: &str,
    identifier: Option<&str>,
) -> Result<(), GenerateError> {
    match mode {
        "module" | "main" => Ok(()),
        "custom" => {
            let _identifier = identifier.ok_or_else(|| GenerateError::UnsupportedJob {
                job: job_name.to_owned(),
                detail: "bundle mode `custom` requires an identifier".to_string(),
            })?;
            Ok(())
        }
        other => Err(GenerateError::UnsupportedJob {
            job: job_name.to_owned(),
            detail: format!("bundle mode `{other}`"),
        }),
    }
}

fn merged_bundle(defaults: &DefaultsConfig, job: &JobConfig) -> BundleConfig {
    BundleConfig {
        mode: job
            .bundle
            .mode
            .clone()
            .or_else(|| defaults.bundle.mode.clone()),
        identifier: job
            .bundle
            .identifier
            .clone()
            .or_else(|| defaults.bundle.identifier.clone()),
    }
}

fn to_utf8_path(path: &Path) -> Result<Utf8PathBuf, GenerateError> {
    Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| GenerateError::InvalidOutputPath { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn make_temp_dir(test_name: &str) -> PathBuf {
        let unique = format!(
            "numi-{test_name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn generate_rejects_duplicate_strings_table_names_from_directory_inputs() {
        let temp_dir = make_temp_dir("duplicate-strings-table");
        let config_path = temp_dir.join("swiftgen.toml");
        let localization_root = temp_dir.join("Resources/Localization");
        let en_dir = localization_root.join("en.lproj");
        let fr_dir = localization_root.join("fr.lproj");
        fs::create_dir_all(&en_dir).expect("en dir should exist");
        fs::create_dir_all(&fr_dir).expect("fr dir should exist");
        fs::write(
            en_dir.join("Localizable.strings"),
            "\"profile.title\" = \"Profile\";\n",
        )
        .expect("en strings should be written");
        fs::write(
            fr_dir.join("Localizable.strings"),
            "\"profile.title\" = \"Profil\";\n",
        )
        .expect("fr strings should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
builtin = "l10n"
"#,
        )
        .expect("config should be written");

        let error = generate(&config_path, None).expect_err("duplicate tables should fail");
        let message = error.to_string();

        assert!(message.contains("duplicate localization table `Localizable`"));
        assert!(message.contains("en.lproj/Localizable.strings"));
        assert!(message.contains("fr.lproj/Localizable.strings"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_renders_custom_template_includes_from_config_root() {
        let temp_dir = make_temp_dir("custom-template-shared-include");
        let config_path = temp_dir.join("swiftgen.toml");
        let localization_root = temp_dir.join("Resources/Localization");
        let templates_dir = temp_dir.join("Templates");
        let generated_path = temp_dir.join("Generated/L10n.swift");

        fs::create_dir_all(localization_root.join("en.lproj"))
            .expect("localization dir should exist");
        fs::create_dir_all(&templates_dir).expect("templates dir should exist");
        fs::create_dir_all(temp_dir.join("partials")).expect("shared partial dir should exist");

        fs::write(
            localization_root.join("en.lproj/Localizable.strings"),
            "\"profile.title\" = \"Profile\";\n",
        )
        .expect("strings file should be written");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
        )
        .expect("template should be written");
        fs::write(temp_dir.join("partials/header.jinja"), "SHARED")
            .expect("shared include should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
path = "Templates/main.jinja"
"#,
        )
        .expect("config should be written");

        let report = generate(&config_path, None).expect("generation should succeed");
        let rendered = fs::read_to_string(&generated_path).expect("output should be written");

        assert_eq!(report.jobs.len(), 1);
        assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
        assert_eq!(rendered, "SHARED|L10n|Localizable\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
