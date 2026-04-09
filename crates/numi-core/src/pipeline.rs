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
    parse_cache::{self, CacheKind, CachedParseData},
    parse_files::{ParseFilesError, parse_files},
    parse_l10n::{LocalizationTable, ParseL10nError, parse_strings, parse_xcstrings},
    parse_xcassets::{ParseXcassetsError, parse_catalog},
    render::{RenderError, render_builtin, render_path},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateReport {
    pub jobs: Vec<JobReport>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub job_name: String,
    pub output_path: Utf8PathBuf,
    pub outcome: WriteOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub stale_paths: Vec<Utf8PathBuf>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpContextReport {
    pub json: String,
    pub warnings: Vec<Diagnostic>,
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
    ParseXcstrings {
        job: String,
        source: ParseL10nError,
    },
    ParseFiles {
        job: String,
        source: ParseFilesError,
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
            Self::ParseXcstrings { job, source } => {
                write!(
                    f,
                    "failed to parse xcstrings input for job `{job}`: {source}"
                )
            }
            Self::ParseFiles { job, source } => {
                write!(f, "failed to parse files input for job `{job}`: {source}")
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
    let mut warnings = Vec::new();

    for job in jobs {
        let job_report = generate_job(&loaded.path, config_dir, &loaded.config.defaults, job)?;
        warnings.extend(job_report.warnings);
        reports.push(JobReport {
            job_name: job_report.job_name,
            output_path: job_report.output_path,
            outcome: job_report.outcome,
        });
    }

    Ok(GenerateReport {
        jobs: reports,
        warnings,
    })
}

pub fn dump_context(
    config_path: &Path,
    job_name: &str,
) -> Result<DumpContextReport, GenerateError> {
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

    let (context, warnings) =
        build_context(&loaded.path, config_dir, &loaded.config.defaults, job)?;
    let json = serde_json::to_string_pretty(&context).map_err(GenerateError::SerializeContext)?;
    Ok(DumpContextReport { json, warnings })
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
    let mut warnings = Vec::new();
    let mut stale_paths = Vec::new();

    for job in jobs {
        let job_report = check_job(&loaded.path, config_dir, &loaded.config.defaults, job)?;
        warnings.extend(job_report.warnings);
        if let Some(output_path) = job_report.stale_path {
            stale_paths.push(output_path);
        }
    }

    Ok(CheckReport {
        stale_paths,
        warnings,
    })
}

fn generate_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<JobExecution, GenerateError> {
    let (context, warnings) = build_context(config_path, config_dir, defaults, job)?;
    let rendered = render_job(config_dir, job, &context)?;

    let output_path = config_dir.join(&job.output);
    let outcome = write_if_changed_atomic(&output_path, &rendered).map_err(|source| {
        GenerateError::WriteOutput {
            job: job.name.clone(),
            source,
        }
    })?;

    Ok(JobExecution {
        job_name: job.name.clone(),
        output_path: to_utf8_path(&output_path)?,
        outcome,
        warnings,
    })
}

fn check_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<CheckJobExecution, GenerateError> {
    let (context, warnings) = build_context(config_path, config_dir, defaults, job)?;
    let rendered = render_job(config_dir, job, &context)?;
    let output_path = config_dir.join(&job.output);
    let stale = output_is_stale(&output_path, &rendered).map_err(|source| {
        GenerateError::InspectOutput {
            job: job.name.clone(),
            source,
        }
    })?;

    Ok(CheckJobExecution {
        stale_path: if stale {
            Some(to_utf8_path(&output_path)?)
        } else {
            None
        },
        warnings,
    })
}

fn build_context(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<(AssetTemplateContext, Vec<Diagnostic>), GenerateError> {
    let BuildModulesResult { modules, warnings } = build_modules(config_dir, job)?;
    let _graph = ResourceGraph {
        modules: modules.clone(),
        diagnostics: warnings.clone(),
        metadata: GraphMetadata {
            config_path: Some(to_utf8_path(config_path)?),
        },
    };

    let access_level = resolve_access_level(defaults, job);
    let bundle = merged_bundle(defaults, job);
    let bundle_mode = bundle.mode.as_deref().unwrap_or("module");
    validate_bundle_mode(&job.name, bundle_mode, bundle.identifier.as_deref())?;
    let context = AssetTemplateContext::new(
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
    })?;

    Ok((context, warnings))
}

struct BuildModulesResult {
    modules: Vec<ResourceModule>,
    warnings: Vec<Diagnostic>,
}

struct JobExecution {
    job_name: String,
    output_path: Utf8PathBuf,
    outcome: WriteOutcome,
    warnings: Vec<Diagnostic>,
}

struct CheckJobExecution {
    stale_path: Option<Utf8PathBuf>,
    warnings: Vec<Diagnostic>,
}

fn build_modules(config_dir: &Path, job: &JobConfig) -> Result<BuildModulesResult, GenerateError> {
    let mut modules = Vec::new();
    let mut asset_entries = Vec::new();
    let mut duplicate_table_sources = BTreeMap::<String, Utf8PathBuf>::new();
    let mut diagnostics = Vec::new();
    let mut warnings = Vec::new();

    for input in &job.inputs {
        let input_path = config_dir.join(&input.path);

        match input.kind.as_str() {
            "xcassets" => {
                let report = load_or_parse_xcassets(&input_path, &job.name)?;
                warnings.extend(
                    report
                        .warnings
                        .into_iter()
                        .map(|warning| warning.with_job(job.name.clone())),
                );
                asset_entries.extend(report.entries);
            }
            "strings" => {
                let tables = load_or_parse_strings(&input_path, &job.name)?;

                for table in tables {
                    let table_name = table.table_name.clone();
                    warnings.extend(
                        table
                            .warnings
                            .into_iter()
                            .map(|warning| warning.with_job(job.name.clone())),
                    );
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
                        kind: table.module_kind,
                        name: swift_identifier(&table_name),
                        entries,
                        metadata: Metadata::from([(
                            "tableName".to_string(),
                            Value::String(table_name),
                        )]),
                    });
                }
            }
            "xcstrings" => {
                let tables = load_or_parse_xcstrings(&input_path, &job.name)?;

                for table in tables {
                    let table_name = table.table_name.clone();
                    warnings.extend(
                        table
                            .warnings
                            .into_iter()
                            .map(|warning| warning.with_job(job.name.clone())),
                    );
                    if let Some(first_source) = duplicate_table_sources
                        .insert(table_name.clone(), table.source_path.clone())
                    {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "duplicate localization table `{table_name}` from directory-based `.xcstrings` input"
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
                        kind: table.module_kind,
                        name: swift_identifier(&table_name),
                        entries,
                        metadata: Metadata::from([(
                            "tableName".to_string(),
                            Value::String(table_name),
                        )]),
                    });
                }
            }
            "files" => {
                let raw_entries = load_or_parse_files(&input_path, &job.name)?;
                let entries =
                    normalize_scope(&job.name, raw_entries).map_err(GenerateError::Diagnostics)?;
                let module_id = input_path
                    .file_stem()
                    .or_else(|| input_path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Files")
                    .to_string();
                modules.push(ResourceModule {
                    id: module_id.clone(),
                    kind: ModuleKind::Files,
                    name: swift_identifier(&module_id),
                    entries,
                    metadata: Metadata::new(),
                });
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

    Ok(BuildModulesResult { modules, warnings })
}

fn load_or_parse_xcassets(
    input_path: &Path,
    job_name: &str,
) -> Result<crate::parse_xcassets::XcassetsReport, GenerateError> {
    load_or_parse_cached(
        CacheKind::Xcassets,
        input_path,
        || {
            parse_catalog(input_path).map_err(|source| GenerateError::ParseXcassets {
                job: job_name.to_owned(),
                source,
            })
        },
        CachedParseData::Xcassets,
        |cached| match cached {
            CachedParseData::Xcassets(report) => Some(report),
            _ => None,
        },
    )
}

fn load_or_parse_strings(
    input_path: &Path,
    job_name: &str,
) -> Result<Vec<LocalizationTable>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Strings,
        input_path,
        || {
            parse_strings(input_path).map_err(|source| GenerateError::ParseStrings {
                job: job_name.to_owned(),
                source,
            })
        },
        CachedParseData::Strings,
        |cached| match cached {
            CachedParseData::Strings(tables) => Some(tables),
            _ => None,
        },
    )
}

fn load_or_parse_xcstrings(
    input_path: &Path,
    job_name: &str,
) -> Result<Vec<LocalizationTable>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Xcstrings,
        input_path,
        || {
            parse_xcstrings(input_path).map_err(|source| GenerateError::ParseXcstrings {
                job: job_name.to_owned(),
                source,
            })
        },
        CachedParseData::Xcstrings,
        |cached| match cached {
            CachedParseData::Xcstrings(tables) => Some(tables),
            _ => None,
        },
    )
}

fn load_or_parse_files(
    input_path: &Path,
    job_name: &str,
) -> Result<Vec<numi_ir::RawEntry>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Files,
        input_path,
        || {
            parse_files(input_path).map_err(|source| GenerateError::ParseFiles {
                job: job_name.to_owned(),
                source,
            })
        },
        CachedParseData::Files,
        |cached| match cached {
            CachedParseData::Files(entries) => Some(entries),
            _ => None,
        },
    )
}

fn load_or_parse_cached<T, ParseFn, WrapFn, ExtractFn>(
    kind: CacheKind,
    input_path: &Path,
    parse: ParseFn,
    wrap: WrapFn,
    extract: ExtractFn,
) -> Result<T, GenerateError>
where
    T: Clone,
    ParseFn: FnOnce() -> Result<T, GenerateError>,
    WrapFn: Fn(T) -> CachedParseData,
    ExtractFn: Fn(CachedParseData) -> Option<T>,
{
    if let Some(parsed) = load_cached_parse(kind, input_path, extract) {
        return Ok(parsed);
    }

    let fingerprint_before_parse = parse_cache::fingerprint_input(kind, input_path).ok();
    let parsed = parse()?;
    store_cached_parse(
        kind,
        input_path,
        fingerprint_before_parse.as_deref(),
        wrap(parsed.clone()),
    );
    Ok(parsed)
}

fn load_cached_parse<T, ExtractFn>(
    kind: CacheKind,
    input_path: &Path,
    extract: ExtractFn,
) -> Option<T>
where
    ExtractFn: Fn(CachedParseData) -> Option<T>,
{
    parse_cache::load(kind, input_path)
        .ok()
        .flatten()
        .and_then(extract)
}

fn store_cached_parse(
    kind: CacheKind,
    input_path: &Path,
    fingerprint_before_parse: Option<&str>,
    data: CachedParseData,
) {
    let Some(fingerprint_before_parse) = fingerprint_before_parse else {
        return;
    };
    let Ok(fingerprint_after_parse) = parse_cache::fingerprint_input(kind, input_path) else {
        return;
    };
    if fingerprint_before_parse != fingerprint_after_parse {
        return;
    }

    let _ = parse_cache::store(kind, input_path, fingerprint_before_parse, &data);
}

fn render_job(
    config_dir: &Path,
    job: &JobConfig,
    context: &AssetTemplateContext,
) -> Result<String, GenerateError> {
    if let Some(builtin_name) = job
        .template
        .builtin
        .as_ref()
        .and_then(|builtin| builtin.swift.as_deref())
    {
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
    use crate::{
        parse_cache::{self, CacheKind, CachedParseData},
        parse_l10n::LocalizationTable,
        parse_xcassets::XcassetsReport,
    };
    use camino::Utf8PathBuf;
    use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::Path,
        path::PathBuf,
        sync::{Mutex, OnceLock},
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

    fn write_strings_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
[jobs.template.builtin]
swift = "l10n"
"#,
        )
        .expect("config should be written");
    }

    fn write_xcstrings_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.template]
[jobs.template.builtin]
swift = "l10n"
"#,
        )
        .expect("config should be written");
    }

    fn write_files_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
[jobs.template.builtin]
swift = "files"
"#,
        )
        .expect("config should be written");
    }

    fn seed_cached_parse(
        kind: CacheKind,
        input_path: &Path,
        data: CachedParseData,
    ) -> Result<(), parse_cache::CacheError> {
        let fingerprint = parse_cache::fingerprint_input(kind, input_path)?;
        parse_cache::store(kind, input_path, &fingerprint, &data)
    }

    fn cache_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lock_cache_env() -> std::sync::MutexGuard<'static, ()> {
        cache_env_lock()
            .lock()
            .expect("cache env lock should not be poisoned")
    }

    struct TempDirOverrideGuard {
        previous: Option<std::ffi::OsString>,
    }

    impl Drop for TempDirOverrideGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => unsafe {
                    std::env::set_var("TMPDIR", value);
                },
                None => unsafe {
                    std::env::remove_var("TMPDIR");
                },
            }
        }
    }

    fn override_temp_dir(temp_dir: &Path) -> TempDirOverrideGuard {
        let previous = std::env::var_os("TMPDIR");
        unsafe {
            std::env::set_var("TMPDIR", temp_dir);
        }
        TempDirOverrideGuard { previous }
    }

    fn with_locked_cache_env<T>(f: impl FnOnce() -> T) -> T {
        let _lock = lock_cache_env();
        f()
    }

    fn with_temp_dir_override<T>(temp_dir: &Path, f: impl FnOnce() -> T) -> T {
        with_locked_cache_env(|| {
            let _guard = override_temp_dir(temp_dir);
            f()
        })
    }

    fn cache_record_path(kind: CacheKind, input_path: &Path) -> PathBuf {
        let canonical = fs::canonicalize(input_path).expect("input path should canonicalize");
        let mut hasher = Sha256::new();
        hasher.update(
            match kind {
                CacheKind::Xcassets => "xcassets",
                CacheKind::Strings => "strings",
                CacheKind::Xcstrings => "xcstrings",
                CacheKind::Files => "files",
            }
            .as_bytes(),
        );
        hasher.update(b"\0");
        hasher.update(canonical.as_os_str().as_encoded_bytes());

        std::env::temp_dir()
            .join("numi-cache")
            .join("parsed-v1")
            .join(format!("{:x}.json", hasher.finalize()))
    }

    #[test]
    fn generate_rejects_duplicate_strings_table_names_from_directory_inputs() {
        let temp_dir = make_temp_dir("duplicate-strings-table");
        let config_path = temp_dir.join("numi.toml");
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
[jobs.template.builtin]
swift = "l10n"
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
    fn generate_accepts_strings_with_escaped_apostrophes_via_langcodec() {
        let temp_dir = make_temp_dir("pipeline-strings-apostrophe");
        let config_path = temp_dir.join("swiftgen.toml");
        let localization_root = temp_dir.join("Resources/Localization/en.lproj");
        fs::create_dir_all(&localization_root).expect("localization dir should exist");
        fs::write(
            localization_root.join("Localizable.strings"),
            "\"invite.accept\" = \"Can\\'t accept the invitation\";\n",
        )
        .expect("strings file should be written");
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
[jobs.template.builtin]
swift = "l10n"
"#,
        )
        .expect("config should be written");

        let report = generate(&config_path, None).expect("generation should succeed");
        let generated_path = temp_dir.join("Generated/L10n.swift");
        let generated = fs::read_to_string(&generated_path).expect("generated output should exist");

        assert!(report.warnings.is_empty());
        assert_eq!(
            generated,
            r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let inviteAccept = tr("Localizable", "invite.accept")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_renders_custom_template_includes_from_config_root() {
        let temp_dir = make_temp_dir("custom-template-shared-include");
        let config_path = temp_dir.join("numi.toml");
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

    #[test]
    fn generate_writes_builtin_files_accessors() {
        let temp_dir = make_temp_dir("pipeline-files-generate");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let generated_path = temp_dir.join("Generated/Files.swift");

        fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
            .expect("video file should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
[jobs.template.builtin]
swift = "files"
"#,
        )
        .expect("config should be written");

        let report = generate(&config_path, None).expect("generation should succeed");
        let rendered = fs::read_to_string(&generated_path).expect("output should be written");

        assert_eq!(report.jobs.len(), 1);
        assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
        assert_eq!(
            rendered,
            r#"import Foundation

internal enum Files {
    internal enum Onboarding {
        internal static let welcomeVideoMp4 = file("Onboarding/welcome-video.mp4")
    }
    internal static let faqPdf = file("faq.pdf")
}

private func resourceBundle() -> Bundle {
    Bundle.module
}

private func file(_ path: String) -> URL {
    guard let url = resourceBundle().url(forResource: path, withExtension: nil) else {
        fatalError("Missing file resource: \(path)")
    }
    return url
}
"#
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn dump_context_builds_files_module_surface() {
        let temp_dir = make_temp_dir("pipeline-files-context");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");

        fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
            .expect("video file should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
[jobs.template.builtin]
swift = "files"
"#,
        )
        .expect("config should be written");

        let report = dump_context(&config_path, "files").expect("dump context should succeed");
        let json: Value = serde_json::from_str(&report.json).expect("json should parse");

        assert_eq!(json["modules"][0]["kind"], "files");
        assert_eq!(json["modules"][0]["name"], "Fixtures");
        assert_eq!(json["modules"][0]["entries"][0]["kind"], "namespace");
        assert_eq!(
            json["modules"][0]["entries"][0]["children"][0]["properties"]["relativePath"],
            "Onboarding/welcome-video.mp4"
        );
        assert_eq!(json["modules"][0]["entries"][1]["kind"], "data");
        assert_eq!(
            json["modules"][0]["entries"][1]["properties"]["fileName"],
            "faq.pdf"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_uses_cached_xcassets_parse_payload_on_cache_hit() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-assets-cache-hit");
            let config_path = temp_dir.join("numi.toml");
            let catalog_root = temp_dir.join("Resources/Assets.xcassets");
            let color_root = catalog_root.join("Brand.colorset");

            fs::create_dir_all(&color_root).expect("catalog should exist");
            fs::write(
                catalog_root.join("Contents.json"),
                r#"{"info":{"author":"xcode","version":1}}"#,
            )
            .expect("catalog contents should exist");
            fs::write(
                color_root.join("Contents.json"),
                r#"{"colors":[{"idiom":"universal","color":{"color-space":"srgb","components":{"red":"1.000","green":"0.000","blue":"0.000","alpha":"1.000"}}}],"info":{"author":"xcode","version":1}}"#,
            )
            .expect("color contents should exist");
            fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
            fs::write(
                &config_path,
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
            .expect("config should be written");

            let cached_source = Utf8PathBuf::from_path_buf(color_root.join("Contents.json"))
                .expect("cached source path should be utf8");
            seed_cached_parse(
                CacheKind::Xcassets,
                &catalog_root,
                CachedParseData::Xcassets(XcassetsReport {
                    entries: vec![RawEntry {
                        path: "CachedPalette".to_string(),
                        source_path: cached_source,
                        kind: EntryKind::Color,
                        properties: Metadata::from([(
                            "assetName".to_string(),
                            json!("CachedPalette"),
                        )]),
                    }],
                    warnings: Vec::new(),
                }),
            )
            .expect("xcassets cache should be seeded");

            let report = generate(&config_path, None).expect("generation should succeed");
            let generated = fs::read_to_string(temp_dir.join("Generated/Assets.swift"))
                .expect("generated assets should exist");

            assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
            assert!(generated.contains("ColorAsset(name: \"CachedPalette\")"));
            assert!(!generated.contains("ColorAsset(name: \"Brand\")"));

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }

    #[test]
    fn generate_uses_cached_strings_parse_payload_on_cache_hit() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-strings-cache-hit");
            let config_path = temp_dir.join("numi.toml");
            let localization_root = temp_dir.join("Resources/Localization/en.lproj");
            let strings_path = localization_root.join("Localizable.strings");

            fs::create_dir_all(&localization_root).expect("localization directory should exist");
            fs::write(&strings_path, "\"profile.title\" = \"Profile\";\n")
                .expect("strings file should be written");
            fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
            write_strings_job_config(&config_path);

            let cached_source = Utf8PathBuf::from_path_buf(strings_path.clone())
                .expect("cached source path should be utf8");
            seed_cached_parse(
                CacheKind::Strings,
                &temp_dir.join("Resources/Localization"),
                CachedParseData::Strings(vec![LocalizationTable {
                    table_name: "Localizable".to_string(),
                    source_path: cached_source.clone(),
                    module_kind: ModuleKind::Strings,
                    entries: vec![RawEntry {
                        path: "cached.banner".to_string(),
                        source_path: cached_source,
                        kind: EntryKind::StringKey,
                        properties: Metadata::from([
                            ("key".to_string(), json!("cached.banner")),
                            ("translation".to_string(), json!("Cached banner")),
                        ]),
                    }],
                    warnings: Vec::new(),
                }]),
            )
            .expect("strings cache should be seeded");

            let report = generate(&config_path, None).expect("generation should succeed");
            let generated = fs::read_to_string(temp_dir.join("Generated/L10n.swift"))
                .expect("generated l10n should exist");

            assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
            assert!(generated.contains("cachedBanner = tr(\"Localizable\", \"cached.banner\")"));
            assert!(!generated.contains("profileTitle = tr(\"Localizable\", \"profile.title\")"));

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }

    #[test]
    fn check_uses_cached_files_parse_and_still_reports_stale_outputs() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-files-check-cache-hit");
            let config_path = temp_dir.join("numi.toml");
            let files_root = temp_dir.join("Resources/Fixtures");

            fs::create_dir_all(files_root.join("Onboarding"))
                .expect("files directory should exist");
            fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
            fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
                .expect("video file should be written");
            fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
            write_files_job_config(&config_path);

            generate(&config_path, None).expect("initial generation should succeed");
            let generated_path = temp_dir.join("Generated/Files.swift");
            let baseline =
                fs::read_to_string(&generated_path).expect("generated output should exist");
            assert!(baseline.contains("welcomeVideoMp4"));

            seed_cached_parse(
                CacheKind::Files,
                &files_root,
                CachedParseData::Files(vec![RawEntry {
                    path: "cached-guide.pdf".to_string(),
                    source_path: Utf8PathBuf::from_path_buf(files_root.join("cached-guide.pdf"))
                        .expect("cached source path should be utf8"),
                    kind: EntryKind::Data,
                    properties: Metadata::from([
                        ("relativePath".to_string(), json!("cached-guide.pdf")),
                        ("fileName".to_string(), json!("cached-guide.pdf")),
                    ]),
                }]),
            )
            .expect("files cache should be seeded");

            let report = check(&config_path, None).expect("check should succeed");

            assert_eq!(
                report.stale_paths,
                vec![Utf8PathBuf::from_path_buf(generated_path).expect("utf8 output path")]
            );

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }

    #[test]
    fn dump_context_uses_cached_xcstrings_parse_and_keeps_json_stable() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-xcstrings-context-cache-hit");
            let config_path = temp_dir.join("numi.toml");
            let localization_root = temp_dir.join("Resources/Localization");
            let xcstrings_path = localization_root.join("Localizable.xcstrings");

            fs::create_dir_all(&localization_root).expect("localization directory should exist");
            fs::write(
                &xcstrings_path,
                r#"{"version":"1.0","sourceLanguage":"en","strings":{"profile.title":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Profile"}}}}}}"#,
            )
            .expect("xcstrings file should be written");
            write_xcstrings_job_config(&config_path);

            let cached_source = Utf8PathBuf::from_path_buf(xcstrings_path.clone())
                .expect("cached source path should be utf8");
            let cached_tables = vec![LocalizationTable {
                table_name: "Localizable".to_string(),
                source_path: cached_source.clone(),
                module_kind: ModuleKind::Xcstrings,
                entries: vec![RawEntry {
                    path: "cached.banner".to_string(),
                    source_path: cached_source,
                    kind: EntryKind::StringKey,
                    properties: Metadata::from([
                        ("key".to_string(), json!("cached.banner")),
                        ("translation".to_string(), json!("Cached banner")),
                    ]),
                }],
                warnings: Vec::new(),
            }];
            seed_cached_parse(
                CacheKind::Xcstrings,
                &localization_root,
                CachedParseData::Xcstrings(cached_tables),
            )
            .expect("xcstrings cache should be seeded");

            let first = dump_context(&config_path, "l10n").expect("first dump should succeed");
            let second = dump_context(&config_path, "l10n").expect("second dump should succeed");
            let json: Value = serde_json::from_str(&first.json).expect("json should parse");

            assert_eq!(first.json, second.json);
            assert_eq!(json["modules"][0]["kind"], "xcstrings");
            assert_eq!(
                json["modules"][0]["entries"][0]["properties"]["key"],
                "cached.banner"
            );

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }

    #[test]
    fn cache_store_skips_entries_when_inputs_change_during_parse() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-cache-skip-unstable-input");
            let files_root = temp_dir.join("Resources/Fixtures");
            let input_file = files_root.join("faq.pdf");

            fs::create_dir_all(&files_root).expect("files directory should exist");
            fs::write(&input_file, "before").expect("fixture file should be written");

            let stale_entries = vec![RawEntry {
                path: "stale.pdf".to_string(),
                source_path: Utf8PathBuf::from_path_buf(input_file.clone())
                    .expect("stale source path should be utf8"),
                kind: EntryKind::Data,
                properties: Metadata::from([
                    ("relativePath".to_string(), json!("stale.pdf")),
                    ("fileName".to_string(), json!("stale.pdf")),
                ]),
            }];
            let fresh_entries = vec![RawEntry {
                path: "fresh.pdf".to_string(),
                source_path: Utf8PathBuf::from_path_buf(input_file.clone())
                    .expect("fresh source path should be utf8"),
                kind: EntryKind::Data,
                properties: Metadata::from([
                    ("relativePath".to_string(), json!("fresh.pdf")),
                    ("fileName".to_string(), json!("fresh.pdf")),
                ]),
            }];

            let first = load_or_parse_cached(
                CacheKind::Files,
                &files_root,
                || {
                    fs::write(&input_file, "after")
                        .expect("fixture file should mutate during parse");
                    Ok::<_, GenerateError>(stale_entries.clone())
                },
                CachedParseData::Files,
                |cached| match cached {
                    CachedParseData::Files(entries) => Some(entries),
                    _ => None,
                },
            )
            .expect("first parse should succeed");
            assert_eq!(first, stale_entries);

            let second = load_or_parse_cached(
                CacheKind::Files,
                &files_root,
                || Ok::<_, GenerateError>(fresh_entries.clone()),
                CachedParseData::Files,
                |cached| match cached {
                    CachedParseData::Files(entries) => Some(entries),
                    _ => None,
                },
            )
            .expect("second parse should succeed");
            assert_eq!(second, fresh_entries);

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }

    #[test]
    fn generate_degrades_when_cache_root_is_unusable() {
        let temp_dir = make_temp_dir("pipeline-cache-degrade-generate");
        let config_path = temp_dir.join("numi.toml");
        let localization_root = temp_dir.join("Resources/Localization/en.lproj");
        let bad_tmp = temp_dir.join("not-a-directory");

        fs::create_dir_all(&localization_root).expect("localization directory should exist");
        fs::write(
            localization_root.join("Localizable.strings"),
            "\"profile.title\" = \"Profile\";\n",
        )
        .expect("strings file should be written");
        fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
        fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
        write_strings_job_config(&config_path);

        let report = with_temp_dir_override(&bad_tmp, || generate(&config_path, None))
            .expect("generation should succeed without cache access");
        let generated = fs::read_to_string(temp_dir.join("Generated/L10n.swift"))
            .expect("generated output should exist");

        assert_eq!(report.jobs.len(), 1);
        assert!(generated.contains("profileTitle = tr(\"Localizable\", \"profile.title\")"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn check_degrades_when_cache_root_is_unusable() {
        let temp_dir = make_temp_dir("pipeline-cache-degrade-check");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let generated_path = temp_dir.join("Generated/Files.swift");
        let bad_tmp = temp_dir.join("not-a-directory");

        fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
            .expect("video file should be written");
        fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
        fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
        write_files_job_config(&config_path);

        generate(&config_path, None).expect("initial generation should succeed");
        fs::write(&generated_path, "stale output").expect("generated output should be mutated");

        let report = with_temp_dir_override(&bad_tmp, || check(&config_path, None))
            .expect("check should succeed without cache access");

        assert_eq!(
            report.stale_paths,
            vec![Utf8PathBuf::from_path_buf(generated_path).expect("utf8 output path")]
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn dump_context_degrades_when_cached_record_is_invalid() {
        with_locked_cache_env(|| {
            let temp_dir = make_temp_dir("pipeline-cache-degrade-dump-context");
            let config_path = temp_dir.join("numi.toml");
            let localization_root = temp_dir.join("Resources/Localization");
            let xcstrings_path = localization_root.join("Localizable.xcstrings");

            fs::create_dir_all(&localization_root).expect("localization directory should exist");
            fs::write(
                &xcstrings_path,
                r#"{"version":"1.0","sourceLanguage":"en","strings":{"profile.title":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Profile"}}}}}}"#,
            )
            .expect("xcstrings file should be written");
            write_xcstrings_job_config(&config_path);

            let cache_path = cache_record_path(CacheKind::Xcstrings, &localization_root);
            fs::create_dir_all(
                cache_path
                    .parent()
                    .expect("cache path should have a parent directory"),
            )
            .expect("cache directory should exist");
            fs::write(&cache_path, "not-json").expect("invalid cache record should be written");

            let report = dump_context(&config_path, "l10n")
                .expect("dump context should succeed with invalid cache record");
            let json: Value = serde_json::from_str(&report.json).expect("json should parse");

            assert_eq!(json["modules"][0]["kind"], "xcstrings");
            assert_eq!(
                json["modules"][0]["entries"][0]["properties"]["key"],
                "profile.title"
            );

            fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
        });
    }
}
