use blake3::Hasher;
use camino::Utf8PathBuf;
use numi_config::{BundleConfig, DefaultsConfig, HookConfig, JobConfig};
use numi_diagnostics::Diagnostic;
use numi_ir::{
    GraphMetadata, Metadata, ModuleKind, ResourceGraph, ResourceModule,
    normalize_flat_entries_preserve_order, normalize_scope, swift_identifier,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    context::{AssetTemplateContext, ContextError},
    generation_cache,
    output::{OutputError, WriteOutcome, output_is_stale, write_if_changed_atomic},
    parse_cache::{self, CacheKind, CachedParseData},
    parse_files::{ParseFilesError, parse_files},
    parse_fonts::{ParseFontsError, parse_font_entries},
    parse_l10n::{LocalizationTable, ParseL10nError, parse_strings, parse_xcstrings},
    parse_xcassets::{ParseXcassetsError, parse_catalog},
    render::{
        RenderError, builtin_template_source, collect_custom_template_dependencies, render_builtin,
        render_path, resolve_template_entry_path,
    },
};

const GENERATION_FINGERPRINT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateReport {
    pub jobs: Vec<JobReport>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenerateProgress {
    JobStarted { job_name: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerateOptions {
    pub incremental: Option<bool>,
    pub parse_cache: Option<bool>,
    pub force_regenerate: bool,
    pub workspace_manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub job_name: String,
    pub output_path: Utf8PathBuf,
    pub outcome: WriteOutcome,
    pub hook_reports: Vec<HookReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReport {
    pub phase: HookPhase,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookPhase {
    PreGenerate,
    PostGenerate,
}

impl HookPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreGenerate => "pre_generate",
            Self::PostGenerate => "post_generate",
        }
    }
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
    ParseFonts {
        job: String,
        source: ParseFontsError,
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
    HookSpawn {
        job: String,
        phase: HookPhase,
        command: Vec<String>,
        source: std::io::Error,
    },
    HookExit {
        job: String,
        phase: HookPhase,
        command: Vec<String>,
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
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
            Self::ParseFonts { job, source } => {
                write!(f, "failed to parse fonts input for job `{job}`: {source}")
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
            Self::HookSpawn {
                job,
                phase,
                command,
                source,
            } => write!(
                f,
                "failed to run {} hook for job `{job}` ({}): {source}",
                phase.as_str(),
                render_hook_command(command)
            ),
            Self::HookExit {
                job,
                phase,
                command,
                status,
                stdout,
                stderr,
            } => {
                write!(
                    f,
                    "{} hook for job `{job}` failed ({}) with status {}",
                    phase.as_str(),
                    render_hook_command(command),
                    status
                )?;
                if !stderr.trim().is_empty() {
                    write!(f, "\nstderr:\n{}", stderr.trim_end())?;
                }
                if !stdout.trim().is_empty() {
                    write!(f, "\nstdout:\n{}", stdout.trim_end())?;
                }
                Ok(())
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
    generate_with_options(config_path, selected_jobs, GenerateOptions::default())
}

pub fn generate_with_options(
    config_path: &Path,
    selected_jobs: Option<&[String]>,
    options: GenerateOptions,
) -> Result<GenerateReport, GenerateError> {
    generate_with_options_and_progress(config_path, selected_jobs, options, |_| {})
}

pub fn generate_with_options_and_progress<F>(
    config_path: &Path,
    selected_jobs: Option<&[String]>,
    options: GenerateOptions,
    progress: F,
) -> Result<GenerateReport, GenerateError>
where
    F: FnMut(&GenerateProgress),
{
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    generate_loaded_config_with_progress(
        &loaded.path,
        &loaded.config,
        selected_jobs,
        options,
        progress,
    )
}

pub fn generate_loaded_config(
    config_path: &Path,
    config: &numi_config::Config,
    selected_jobs: Option<&[String]>,
    options: GenerateOptions,
) -> Result<GenerateReport, GenerateError> {
    generate_loaded_config_with_progress(config_path, config, selected_jobs, options, |_| {})
}

pub fn generate_loaded_config_with_progress<F>(
    config_path: &Path,
    config: &numi_config::Config,
    selected_jobs: Option<&[String]>,
    options: GenerateOptions,
    mut progress: F,
) -> Result<GenerateReport, GenerateError>
where
    F: FnMut(&GenerateProgress),
{
    let config_dir = config_dir(config_path);
    let jobs = numi_config::resolve_selected_jobs(config, selected_jobs)
        .map_err(GenerateError::Diagnostics)?;

    let mut reports = Vec::with_capacity(jobs.len());
    let mut warnings = Vec::new();

    for job in jobs {
        progress(&GenerateProgress::JobStarted {
            job_name: job.name.clone(),
        });
        let job_report = generate_job(config_path, config_dir, &config.defaults, job, &options)?;
        warnings.extend(job_report.warnings);
        reports.push(JobReport {
            job_name: job_report.job_name,
            output_path: job_report.output_path,
            outcome: job_report.outcome,
            hook_reports: job_report.hook_reports,
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
        build_context(&loaded.path, config_dir, &loaded.config.defaults, job, None)?;
    let json = serde_json::to_string_pretty(&context).map_err(GenerateError::SerializeContext)?;
    Ok(DumpContextReport { json, warnings })
}

pub fn check(
    config_path: &Path,
    selected_jobs: Option<&[String]>,
) -> Result<CheckReport, GenerateError> {
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    check_loaded_config(&loaded.path, &loaded.config, selected_jobs)
}

pub fn check_loaded_config(
    config_path: &Path,
    config: &numi_config::Config,
    selected_jobs: Option<&[String]>,
) -> Result<CheckReport, GenerateError> {
    let config_dir = config_dir(config_path);
    let jobs = numi_config::resolve_selected_jobs(config, selected_jobs)
        .map_err(GenerateError::Diagnostics)?;
    let mut warnings = Vec::new();
    let mut stale_paths = Vec::new();

    for job in jobs {
        let job_report = check_job(config_path, config_dir, &config.defaults, job)?;
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

fn config_dir(config_path: &Path) -> &Path {
    config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn generate_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
    options: &GenerateOptions,
) -> Result<JobExecution, GenerateError> {
    let output_path = config_dir.join(&job.output);
    let mut hook_reports = Vec::new();
    let incremental = resolve_incremental(defaults, job, options);
    let parse_cache = resolve_parse_cache(options);
    let should_check_generation_cache = incremental
        && !options.force_regenerate
        && generation_cache::cache_record_exists(config_path, &job.name)
            .ok()
            .unwrap_or(false);
    let mut generation_plan = None;

    if should_check_generation_cache || parse_cache {
        generation_plan = compute_generation_fingerprint(config_dir, defaults, job);
    }

    if incremental
        && !options.force_regenerate
        && let Some(plan) = generation_plan.as_ref()
        && generation_cache::is_fresh(config_path, &job.name, &plan.fingerprint, &output_path)
            .ok()
            .unwrap_or(false)
    {
        return Ok(JobExecution {
            job_name: job.name.clone(),
            output_path: to_utf8_path(&output_path)?,
            outcome: WriteOutcome::Skipped,
            hook_reports,
            warnings: Vec::new(),
        });
    }

    if generation_plan.is_none() && incremental {
        generation_plan = compute_generation_fingerprint(config_dir, defaults, job);
    }

    let hook_env = HookEnvironment::new(
        config_path,
        options.workspace_manifest_path.as_deref(),
        &job.name,
        &output_path,
    )?;
    if let Some(hook) = job.hooks.pre_generate.as_ref() {
        hook_reports.push(run_hook(
            config_dir,
            hook,
            HookPhase::PreGenerate,
            &job.name,
            &hook_env,
            None,
        )?);
    }

    let (context, warnings) = build_context(
        config_path,
        config_dir,
        defaults,
        job,
        parse_cache.then_some(()).and(generation_plan.as_ref()),
    )?;
    let rendered = render_job(config_dir, job, &context)?;
    let outcome = write_if_changed_atomic(&output_path, &rendered).map_err(|source| {
        GenerateError::WriteOutput {
            job: job.name.clone(),
            source,
        }
    })?;

    if let Some(plan) = generation_plan.as_ref() {
        let _ = generation_cache::store(config_path, &job.name, &plan.fingerprint, &output_path);
    }

    let should_run_post_hook = matches!(outcome, WriteOutcome::Created | WriteOutcome::Updated)
        || (options.force_regenerate && matches!(outcome, WriteOutcome::Unchanged));

    if should_run_post_hook && let Some(hook) = job.hooks.post_generate.as_ref() {
        hook_reports.push(run_hook(
            config_dir,
            hook,
            HookPhase::PostGenerate,
            &job.name,
            &hook_env,
            Some(outcome),
        )?);
    }

    Ok(JobExecution {
        job_name: job.name.clone(),
        output_path: to_utf8_path(&output_path)?,
        outcome,
        hook_reports,
        warnings,
    })
}

fn check_job(
    config_path: &Path,
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Result<CheckJobExecution, GenerateError> {
    let (context, warnings) = build_context(config_path, config_dir, defaults, job, None)?;
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
    generation_plan: Option<&GenerationFingerprintPlan>,
) -> Result<(AssetTemplateContext, Vec<Diagnostic>), GenerateError> {
    let BuildModulesResult { modules, warnings } = build_modules(config_dir, job, generation_plan)?;
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
    hook_reports: Vec<HookReport>,
    warnings: Vec<Diagnostic>,
}

struct CheckJobExecution {
    stale_path: Option<Utf8PathBuf>,
    warnings: Vec<Diagnostic>,
}

struct GenerationFingerprintPlan {
    fingerprint: String,
    cache_input_fingerprints: BTreeMap<PathBuf, ParseCacheInputPlan>,
}

struct ParseCacheInputPlan {
    fingerprint: String,
    snapshot: parse_cache::InputSnapshot,
}

#[derive(Debug, Serialize)]
struct GenerationFingerprintRecord {
    schema_version: u32,
    job_name: String,
    output: String,
    access_level: String,
    bundle_mode: String,
    bundle_identifier: Option<String>,
    inputs: Vec<GenerationInputFingerprintRecord>,
    template: GenerationTemplateFingerprintRecord,
}

#[derive(Debug, Serialize)]
struct GenerationInputFingerprintRecord {
    kind: String,
    path: String,
    fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
enum GenerationTemplateFingerprintRecord {
    Builtin {
        language: String,
        name: String,
        fingerprint: String,
    },
    Custom {
        path: String,
        dependencies: Vec<GenerationDependencyFingerprintRecord>,
    },
}

#[derive(Debug, Serialize)]
struct GenerationDependencyFingerprintRecord {
    path: String,
    fingerprint: String,
}

fn build_modules(
    config_dir: &Path,
    job: &JobConfig,
    generation_plan: Option<&GenerationFingerprintPlan>,
) -> Result<BuildModulesResult, GenerateError> {
    let mut modules = Vec::new();
    let mut asset_entries = Vec::new();
    let mut duplicate_table_sources = BTreeMap::<String, Utf8PathBuf>::new();
    let mut duplicate_files_module_sources = BTreeMap::<String, (String, PathBuf)>::new();
    let mut duplicate_fonts_module_sources = BTreeMap::<String, (String, PathBuf)>::new();
    let mut diagnostics = Vec::new();
    let mut warnings = Vec::new();

    for input in &job.inputs {
        let input_path = config_dir.join(&input.path);
        let known_cache_input =
            generation_plan.and_then(|plan| plan.cache_input_fingerprints.get(&input_path));
        let known_cache_fingerprint = known_cache_input.map(|plan| plan.fingerprint.as_str());
        let known_cache_snapshot = known_cache_input.map(|plan| &plan.snapshot);

        match input.kind.as_str() {
            "xcassets" => {
                let report = load_or_parse_xcassets(
                    &input_path,
                    &job.name,
                    known_cache_fingerprint,
                    known_cache_snapshot,
                )?;
                warnings.extend(
                    report
                        .warnings
                        .into_iter()
                        .map(|warning| warning.with_job(job.name.clone())),
                );
                asset_entries.extend(report.entries);
            }
            "strings" => {
                let tables = load_or_parse_strings(
                    &input_path,
                    &job.name,
                    known_cache_fingerprint,
                    known_cache_snapshot,
                )?;

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
                    let entries = normalize_flat_entries_preserve_order(&job.name, table.entries)
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
                let tables = load_or_parse_xcstrings(
                    &input_path,
                    &job.name,
                    known_cache_fingerprint,
                    known_cache_snapshot,
                )?;

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
                let raw_entries = load_or_parse_files(
                    &input_path,
                    &job.name,
                    known_cache_fingerprint,
                    known_cache_snapshot,
                )?;
                let mut entries =
                    normalize_scope(&job.name, raw_entries).map_err(GenerateError::Diagnostics)?;
                annotate_swiftgen_file_sort_keys(&mut entries);
                let module_id = input_path
                    .file_stem()
                    .or_else(|| input_path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Files")
                    .to_string();
                let module_name = swift_identifier(&module_id);
                if let Some(diagnostic) = duplicate_input_module_diagnostic(
                    &mut duplicate_files_module_sources,
                    "files",
                    &job.name,
                    &module_id,
                    &module_name,
                    &input_path,
                ) {
                    diagnostics.push(diagnostic);
                    continue;
                }
                modules.push(ResourceModule {
                    id: module_id.clone(),
                    kind: ModuleKind::Files,
                    name: module_name,
                    entries,
                    metadata: Metadata::new(),
                });
            }
            "fonts" => {
                let parsed_fonts = parse_font_entries(&input_path).map_err(|source| {
                    GenerateError::ParseFonts {
                        job: job.name.clone(),
                        source,
                    }
                })?;
                let raw_entries = parsed_fonts
                    .iter()
                    .cloned()
                    .map(crate::parse_fonts::ParsedFontEntry::into_raw_entry)
                    .collect::<Vec<_>>();
                let entries =
                    normalize_scope(&job.name, raw_entries).map_err(GenerateError::Diagnostics)?;
                let module_id = input_path
                    .file_stem()
                    .or_else(|| input_path.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Fonts")
                    .to_string();
                let module_name = swift_identifier(&module_id);
                if let Some(diagnostic) = duplicate_input_module_diagnostic(
                    &mut duplicate_fonts_module_sources,
                    "fonts",
                    &job.name,
                    &module_id,
                    &module_name,
                    &input_path,
                ) {
                    diagnostics.push(diagnostic);
                    continue;
                }
                modules.push(ResourceModule {
                    id: module_id.clone(),
                    kind: ModuleKind::Fonts,
                    name: module_name,
                    entries,
                    metadata: build_font_module_metadata(&parsed_fonts),
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
        let mut entries =
            normalize_scope(&job.name, asset_entries).map_err(GenerateError::Diagnostics)?;
        sort_entries_for_assets(&mut entries);
        annotate_swiftgen_sort_keys(&mut entries);
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

fn duplicate_input_module_diagnostic(
    seen_modules: &mut BTreeMap<String, (String, PathBuf)>,
    input_kind: &str,
    job_name: &str,
    module_id: &str,
    module_name: &str,
    input_path: &Path,
) -> Option<Diagnostic> {
    if let Some((first_module_id, first_source)) = seen_modules.insert(
        module_name.to_string(),
        (module_id.to_string(), input_path.to_path_buf()),
    ) {
        let detail = if first_module_id == module_id {
            format!("both inputs normalize to module `{module_name}`")
        } else {
            format!(
                "module names `{first_module_id}` and `{module_id}` both normalize to `{module_name}`"
            )
        };
        return Some(
            Diagnostic::error(format!("duplicate {input_kind} module `{module_name}`"))
                .with_job(job_name)
                .with_path(input_path)
                .with_hint(format!(
                    "found `{}` and `{}`; {detail}",
                    first_source.display(),
                    input_path.display()
                )),
        );
    }

    None
}

fn annotate_swiftgen_sort_keys(entries: &mut [numi_ir::ResourceEntry]) {
    for entry in entries {
        entry.metadata.insert(
            "sortKey".to_string(),
            Value::String(swiftgen_sort_key(&entry.swift_identifier)),
        );
        annotate_swiftgen_sort_keys(&mut entry.children);
    }
}

fn annotate_swiftgen_file_sort_keys(entries: &mut [numi_ir::ResourceEntry]) {
    let sibling_names = entries
        .iter()
        .map(|entry| entry.name.to_ascii_lowercase())
        .collect::<Vec<_>>();

    for entry in entries {
        entry.metadata.insert(
            "sortKey".to_string(),
            Value::String(swiftgen_file_sort_key(&entry.name, &sibling_names)),
        );
        annotate_swiftgen_file_sort_keys(&mut entry.children);
    }
}

fn swiftgen_file_sort_key(name: &str, sibling_names: &[String]) -> String {
    let _ = sibling_names;
    name.to_ascii_lowercase()
}

fn sort_entries_for_assets(entries: &mut [numi_ir::ResourceEntry]) {
    for entry in entries.iter_mut() {
        sort_entries_for_assets(&mut entry.children);
    }
    entries.sort_by(compare_asset_entries);
}

fn compare_asset_entries(
    left: &numi_ir::ResourceEntry,
    right: &numi_ir::ResourceEntry,
) -> Ordering {
    compare_asset_names(&left.name, &right.name).then_with(|| left.id.cmp(&right.id))
}

fn compare_asset_names(left: &str, right: &str) -> Ordering {
    match (left.strip_suffix(".9"), right.strip_suffix(".9")) {
        (Some(base), None) if base == right => Ordering::Less,
        (None, Some(base)) if left == base => Ordering::Greater,
        _ => left.cmp(right),
    }
}

fn swiftgen_sort_key(identifier: &str) -> String {
    let identifier = identifier
        .strip_prefix('`')
        .and_then(|trimmed| trimmed.strip_suffix('`'))
        .unwrap_or(identifier);
    let chars = identifier.chars().collect::<Vec<_>>();
    if chars.is_empty() || !chars[0].is_ascii_uppercase() {
        return identifier.to_ascii_lowercase();
    }

    let mut prefix_len = 1;
    while prefix_len < chars.len() && chars[prefix_len].is_ascii_uppercase() {
        prefix_len += 1;
    }

    let lower_count = if prefix_len == chars.len() {
        prefix_len
    } else if prefix_len == 1 {
        1
    } else {
        prefix_len - 1
    };

    let mut lowered = String::with_capacity(identifier.len());
    for ch in &chars[..lower_count] {
        lowered.push(ch.to_ascii_lowercase());
    }
    for ch in &chars[lower_count..] {
        lowered.push(*ch);
    }
    lowered.to_ascii_lowercase()
}

fn build_font_module_metadata(parsed_fonts: &[crate::parse_fonts::ParsedFontEntry]) -> Metadata {
    #[derive(Clone)]
    struct CanonicalFont {
        file_name: String,
        relative_path: String,
        family_name: String,
        style_name: String,
        full_name: String,
        post_script_name: String,
    }

    let mut unique_fonts = BTreeMap::<String, CanonicalFont>::new();
    for font in parsed_fonts {
        let (family_name, style_name) = canonicalize_font_family_and_style(
            &font.family_name,
            &font.style_name,
            &font.post_script_name,
        );
        unique_fonts.insert(
            font.post_script_name.clone(),
            CanonicalFont {
                file_name: font.file_name.clone(),
                relative_path: font.relative_path.clone(),
                family_name,
                style_name,
                full_name: font.full_name.clone(),
                post_script_name: font.post_script_name.clone(),
            },
        );
    }

    let mut families = BTreeMap::<String, Vec<CanonicalFont>>::new();
    for font in unique_fonts.into_values() {
        families
            .entry(font.family_name.clone())
            .or_default()
            .push(font);
    }

    let mut family_items = Vec::new();
    for (family_name, mut fonts) in families {
        fonts.sort_by(|left, right| left.post_script_name.cmp(&right.post_script_name));
        let display_name = if fonts.len() == 1
            && fonts[0].style_name != "Regular"
            && fonts[0].full_name.ends_with(&fonts[0].style_name)
            && fonts[0].full_name != family_name
        {
            fonts[0].full_name.clone()
        } else {
            family_name.clone()
        };
        family_items.push(Value::Object(
            [
                ("name".to_string(), Value::String(display_name.clone())),
                (
                    "swiftIdentifier".to_string(),
                    Value::String(swift_identifier(&display_name)),
                ),
                (
                    "fonts".to_string(),
                    Value::Array(
                        fonts
                            .into_iter()
                            .map(|font| {
                                Value::Object(
                                    [
                                        (
                                            "postScriptName".to_string(),
                                            Value::String(font.post_script_name.clone()),
                                        ),
                                        (
                                            "styleName".to_string(),
                                            Value::String(font.style_name.clone()),
                                        ),
                                        (
                                            "familyName".to_string(),
                                            Value::String(display_name.clone()),
                                        ),
                                        (
                                            "fileName".to_string(),
                                            Value::String(font.file_name.clone()),
                                        ),
                                        (
                                            "relativePath".to_string(),
                                            Value::String(font.relative_path.clone()),
                                        ),
                                    ]
                                    .into_iter()
                                    .collect(),
                                )
                            })
                            .collect(),
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        ));
    }

    Metadata::from([("families".to_string(), Value::Array(family_items))])
}

fn canonicalize_font_family_and_style(
    family_name: &str,
    style_name: &str,
    post_script_name: &str,
) -> (String, String) {
    if style_name != "Regular" {
        return (family_name.to_string(), style_name.to_string());
    }

    let Some((_, post_script_style)) = post_script_name.rsplit_once('-') else {
        return (family_name.to_string(), style_name.to_string());
    };

    if post_script_style == "Regular" {
        return (family_name.to_string(), style_name.to_string());
    }

    if let Some(prefix) = family_name.strip_suffix(&format!(" {post_script_style}")) {
        return (prefix.to_string(), post_script_style.to_string());
    }

    (family_name.to_string(), style_name.to_string())
}

fn load_or_parse_xcassets(
    input_path: &Path,
    job_name: &str,
    known_fingerprint: Option<&str>,
    known_snapshot: Option<&parse_cache::InputSnapshot>,
) -> Result<crate::parse_xcassets::XcassetsReport, GenerateError> {
    load_or_parse_cached(
        CacheKind::Xcassets,
        input_path,
        known_fingerprint,
        known_snapshot,
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
    known_fingerprint: Option<&str>,
    known_snapshot: Option<&parse_cache::InputSnapshot>,
) -> Result<Vec<LocalizationTable>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Strings,
        input_path,
        known_fingerprint,
        known_snapshot,
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
    known_fingerprint: Option<&str>,
    known_snapshot: Option<&parse_cache::InputSnapshot>,
) -> Result<Vec<LocalizationTable>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Xcstrings,
        input_path,
        known_fingerprint,
        known_snapshot,
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
    known_fingerprint: Option<&str>,
    known_snapshot: Option<&parse_cache::InputSnapshot>,
) -> Result<Vec<numi_ir::RawEntry>, GenerateError> {
    load_or_parse_cached(
        CacheKind::Files,
        input_path,
        known_fingerprint,
        known_snapshot,
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
    known_fingerprint: Option<&str>,
    known_snapshot: Option<&parse_cache::InputSnapshot>,
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
    if let Some(parsed) = load_cached_parse(kind, input_path, known_fingerprint, extract) {
        return Ok(parsed);
    }

    let mut snapshot_before_parse = known_snapshot.cloned();
    let fingerprint_before_parse = if let Some(fingerprint) = known_fingerprint {
        Some(fingerprint.to_owned())
    } else {
        let fingerprinted = parse_cache::fingerprint_input_with_snapshot(kind, input_path).ok();
        if let Some(fingerprinted) = fingerprinted {
            snapshot_before_parse = Some(fingerprinted.snapshot.clone());
            Some(fingerprinted.fingerprint)
        } else {
            None
        }
    };
    let parsed = parse()?;
    store_cached_parse(
        kind,
        input_path,
        fingerprint_before_parse.as_deref(),
        snapshot_before_parse.as_ref(),
        wrap(parsed.clone()),
    );
    Ok(parsed)
}

fn load_cached_parse<T, ExtractFn>(
    kind: CacheKind,
    input_path: &Path,
    known_fingerprint: Option<&str>,
    extract: ExtractFn,
) -> Option<T>
where
    ExtractFn: Fn(CachedParseData) -> Option<T>,
{
    let loaded = match known_fingerprint {
        Some(fingerprint) => parse_cache::load_with_fingerprint(kind, input_path, fingerprint),
        None => parse_cache::load(kind, input_path),
    };
    loaded.ok().flatten().and_then(extract)
}

fn store_cached_parse(
    kind: CacheKind,
    input_path: &Path,
    fingerprint_before_parse: Option<&str>,
    snapshot_before_parse: Option<&parse_cache::InputSnapshot>,
    data: CachedParseData,
) {
    let Some(fingerprint_before_parse) = fingerprint_before_parse else {
        return;
    };
    if let Some(snapshot_before_parse) = snapshot_before_parse {
        let Ok(snapshot_matches) =
            parse_cache::input_matches_snapshot(kind, input_path, snapshot_before_parse)
        else {
            return;
        };
        if !snapshot_matches {
            return;
        }
    } else {
        let Ok(fingerprint_after_parse) = parse_cache::fingerprint_input(kind, input_path) else {
            return;
        };
        if fingerprint_before_parse != fingerprint_after_parse {
            return;
        }
    }

    let _ = parse_cache::store(kind, input_path, fingerprint_before_parse, &data);
}

fn render_job(
    config_dir: &Path,
    job: &JobConfig,
    context: &AssetTemplateContext,
) -> Result<String, GenerateError> {
    let builtin = job.template.builtin.as_ref();
    let builtin_language = builtin.and_then(|builtin| builtin.language.as_deref());
    let builtin_name = builtin.and_then(|builtin| builtin.name.as_deref());

    if let (Some(language), Some(name)) = (builtin_language, builtin_name) {
        return render_builtin((language, name), context).map_err(|source| GenerateError::Render {
            job: job.name.clone(),
            source,
        });
    }

    if let Some(template_path) = job.template.path.as_deref() {
        let resolved_path =
            resolve_template_entry_path(config_dir, template_path).map_err(|source| {
                GenerateError::Render {
                    job: job.name.clone(),
                    source,
                }
            })?;
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

fn resolve_incremental(
    defaults: &DefaultsConfig,
    job: &JobConfig,
    options: &GenerateOptions,
) -> bool {
    options
        .incremental
        .or(job.incremental)
        .or(defaults.incremental)
        .unwrap_or(true)
}

fn resolve_parse_cache(options: &GenerateOptions) -> bool {
    options.parse_cache.unwrap_or(true)
}

fn compute_generation_fingerprint(
    config_dir: &Path,
    defaults: &DefaultsConfig,
    job: &JobConfig,
) -> Option<GenerationFingerprintPlan> {
    let mut cache_input_fingerprints = BTreeMap::new();
    let inputs = job
        .inputs
        .iter()
        .map(|input| {
            let resolved_path = config_dir.join(&input.path);
            let fingerprint = if let Some(kind) = cache_kind_for_input(&input.kind) {
                let fingerprinted =
                    parse_cache::fingerprint_input_with_snapshot(kind, &resolved_path).ok()?;
                let fingerprint = fingerprinted.fingerprint;
                cache_input_fingerprints.insert(
                    resolved_path.clone(),
                    ParseCacheInputPlan {
                        fingerprint: fingerprint.clone(),
                        snapshot: fingerprinted.snapshot,
                    },
                );
                fingerprint
            } else {
                fingerprint_path_contents(&resolved_path).ok()?
            };
            Some(GenerationInputFingerprintRecord {
                kind: input.kind.clone(),
                path: input.path.clone(),
                fingerprint,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let builtin = job.template.builtin.as_ref();
    let builtin_language = builtin.and_then(|builtin| builtin.language.as_deref());
    let builtin_name = builtin.and_then(|builtin| builtin.name.as_deref());

    let template = if let (Some(language), Some(name)) = (builtin_language, builtin_name) {
        let source = builtin_template_source((language, name)).ok()?;
        GenerationTemplateFingerprintRecord::Builtin {
            language: language.to_owned(),
            name: name.to_owned(),
            fingerprint: generation_cache::blake3_hex([source.as_bytes()]),
        }
    } else if let Some(template_path) = job.template.path.as_deref() {
        let resolved_path = resolve_template_entry_path(config_dir, template_path).ok()?;
        let dependencies = collect_custom_template_dependencies(&resolved_path, config_dir)
            .ok()??
            .into_iter()
            .map(|dependency_path| {
                Some(GenerationDependencyFingerprintRecord {
                    path: display_relative_path(&dependency_path, config_dir),
                    fingerprint: fingerprint_path_contents(&dependency_path).ok()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        GenerationTemplateFingerprintRecord::Custom {
            path: template_path.to_owned(),
            dependencies,
        }
    } else {
        return None;
    };

    let bundle = merged_bundle(defaults, job);
    let record = GenerationFingerprintRecord {
        schema_version: GENERATION_FINGERPRINT_SCHEMA_VERSION,
        job_name: job.name.clone(),
        output: job.output.clone(),
        access_level: resolve_access_level(defaults, job).to_owned(),
        bundle_mode: bundle.mode.clone().unwrap_or_else(|| "module".to_string()),
        bundle_identifier: bundle.identifier.clone(),
        inputs,
        template,
    };
    let payload = serde_json::to_vec(&record).ok()?;
    Some(GenerationFingerprintPlan {
        fingerprint: generation_cache::blake3_hex([payload.as_slice()]),
        cache_input_fingerprints,
    })
}

fn cache_kind_for_input(input_kind: &str) -> Option<CacheKind> {
    match input_kind {
        "xcassets" => Some(CacheKind::Xcassets),
        "strings" => Some(CacheKind::Strings),
        "xcstrings" => Some(CacheKind::Xcstrings),
        "files" => Some(CacheKind::Files),
        _ => None,
    }
}

fn fingerprint_path_contents(path: &Path) -> std::io::Result<String> {
    let mut files = Vec::new();

    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        collect_fingerprint_files(path, &mut files)?;
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path {} does not exist", path.display()),
        ));
    }

    files.sort();

    let mut hasher = Hasher::new();
    hasher.update(if path.is_file() { b"file" } else { b"dir" });
    hasher.update(b"\0");

    for file in files {
        let relative = if path.is_file() {
            file.file_name().unwrap_or_default().as_encoded_bytes()
        } else {
            file.strip_prefix(path)
                .unwrap_or(&file)
                .as_os_str()
                .as_encoded_bytes()
        };
        let contents = fs::read(&file)?;

        hasher.update(relative);
        hasher.update(b"\0");
        hasher.update(&contents);
        hasher.update(b"\0");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

fn collect_fingerprint_files(root: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if path.file_name().is_some_and(|name| name == ".DS_Store") {
            continue;
        }

        if file_type.is_dir() {
            collect_fingerprint_files(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }

    Ok(())
}

fn display_relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
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

#[derive(Debug, Clone)]
struct HookEnvironment {
    config_path: PathBuf,
    workspace_manifest_path: Option<PathBuf>,
    output_path: PathBuf,
    output_dir: PathBuf,
    job_name: String,
}

impl HookEnvironment {
    fn new(
        config_path: &Path,
        workspace_manifest_path: Option<&Path>,
        job_name: &str,
        output_path: &Path,
    ) -> Result<Self, GenerateError> {
        Ok(Self {
            config_path: absolute_path(config_path)?,
            workspace_manifest_path: workspace_manifest_path.map(absolute_path).transpose()?,
            output_path: absolute_path(output_path)?,
            output_dir: absolute_path(output_path.parent().unwrap_or_else(|| Path::new(".")))?,
            job_name: job_name.to_owned(),
        })
    }
}

fn run_hook(
    config_dir: &Path,
    hook: &HookConfig,
    phase: HookPhase,
    job_name: &str,
    env: &HookEnvironment,
    outcome: Option<WriteOutcome>,
) -> Result<HookReport, GenerateError> {
    let (program, args) = resolve_hook_command(config_dir, &hook.command);
    let output = Command::new(&program)
        .args(args)
        .current_dir(config_dir)
        .env("NUMI_HOOK_PHASE", phase.as_str())
        .env("NUMI_HOOK_JOB_NAME", &env.job_name)
        .env("NUMI_JOB_NAME", &env.job_name)
        .env("NUMI_HOOK_CONFIG_PATH", &env.config_path)
        .env("NUMI_CONFIG_PATH", &env.config_path)
        .env("NUMI_HOOK_OUTPUT_PATH", &env.output_path)
        .env("NUMI_OUTPUT_PATH", &env.output_path)
        .env("NUMI_HOOK_OUTPUT_DIR", &env.output_dir)
        .env("NUMI_OUTPUT_DIR", &env.output_dir)
        .env_remove("NUMI_HOOK_WRITE_OUTCOME")
        .env_remove("NUMI_WRITE_OUTCOME")
        .env_remove("NUMI_HOOK_WORKSPACE_CONFIG_PATH")
        .env_remove("NUMI_WORKSPACE_MANIFEST_PATH")
        .envs(
            outcome
                .map(|value| {
                    let outcome = write_outcome_name(value).to_string();
                    [
                        ("NUMI_HOOK_WRITE_OUTCOME", outcome.clone()),
                        ("NUMI_WRITE_OUTCOME", outcome),
                    ]
                })
                .into_iter()
                .flatten(),
        )
        .envs(
            env.workspace_manifest_path
                .as_ref()
                .map(|path| {
                    let path = path.display().to_string();
                    [
                        ("NUMI_HOOK_WORKSPACE_CONFIG_PATH", path.clone()),
                        ("NUMI_WORKSPACE_MANIFEST_PATH", path),
                    ]
                })
                .into_iter()
                .flatten(),
        )
        .output()
        .map_err(|source| GenerateError::HookSpawn {
            job: job_name.to_owned(),
            phase,
            command: hook.command.clone(),
            source,
        })?;

    if !output.status.success() {
        return Err(GenerateError::HookExit {
            job: job_name.to_owned(),
            phase,
            command: hook.command.clone(),
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    Ok(HookReport {
        phase,
        command: hook.command.clone(),
    })
}

fn resolve_hook_command<'a>(config_dir: &Path, command: &'a [String]) -> (PathBuf, &'a [String]) {
    let program = command
        .first()
        .map(|value| {
            if command_looks_like_path(value) {
                config_dir.join(value)
            } else {
                PathBuf::from(value)
            }
        })
        .unwrap_or_default();
    (program, &command[1..])
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

fn absolute_path(path: &Path) -> Result<PathBuf, GenerateError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir().map_err(|error| GenerateError::UnsupportedJob {
        job: "hooks".to_string(),
        detail: format!("failed to read cwd while resolving hook paths: {error}"),
    })?;
    Ok(cwd.join(path))
}

fn write_outcome_name(outcome: WriteOutcome) -> &'static str {
    match outcome {
        WriteOutcome::Created => "created",
        WriteOutcome::Updated => "updated",
        WriteOutcome::Unchanged => "unchanged",
        WriteOutcome::Skipped => "skipped",
    }
}

fn render_hook_command(command: &[String]) -> String {
    command.join(" ")
}

fn to_utf8_path(path: &Path) -> Result<Utf8PathBuf, GenerateError> {
    Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| GenerateError::InvalidOutputPath { path })
}

#[cfg(test)]
mod tests;
