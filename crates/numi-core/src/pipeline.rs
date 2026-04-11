use blake3::Hasher;
use camino::Utf8PathBuf;
use numi_config::{BundleConfig, DefaultsConfig, JobConfig};
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GenerateOptions {
    pub incremental: Option<bool>,
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
    let loaded = numi_config::load_from_path(config_path).map_err(GenerateError::LoadConfig)?;
    generate_loaded_config(&loaded.path, &loaded.config, selected_jobs, options)
}

pub fn generate_loaded_config(
    config_path: &Path,
    config: &numi_config::Config,
    selected_jobs: Option<&[String]>,
    options: GenerateOptions,
) -> Result<GenerateReport, GenerateError> {
    let config_dir = config_dir(config_path);
    let jobs = numi_config::resolve_selected_jobs(config, selected_jobs)
        .map_err(GenerateError::Diagnostics)?;

    let mut reports = Vec::with_capacity(jobs.len());
    let mut warnings = Vec::new();

    for job in jobs {
        let job_report = generate_job(config_path, config_dir, &config.defaults, job, &options)?;
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
    let incremental = resolve_incremental(defaults, job, options);
    let should_check_generation_cache = incremental
        && generation_cache::cache_record_exists(config_path, &job.name)
            .ok()
            .unwrap_or(false);
    let mut generation_plan = None;

    if should_check_generation_cache {
        generation_plan = compute_generation_fingerprint(config_dir, defaults, job);
    }

    if incremental
        && let Some(plan) = generation_plan.as_ref()
        && generation_cache::is_fresh(config_path, &job.name, &plan.fingerprint, &output_path)
            .ok()
            .unwrap_or(false)
    {
        return Ok(JobExecution {
            job_name: job.name.clone(),
            output_path: to_utf8_path(&output_path)?,
            outcome: WriteOutcome::Skipped,
            warnings: Vec::new(),
        });
    }

    if generation_plan.is_none() {
        generation_plan = compute_generation_fingerprint(config_dir, defaults, job);
    }

    let (context, warnings) = build_context(
        config_path,
        config_dir,
        defaults,
        job,
        generation_plan.as_ref(),
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
                modules.push(ResourceModule {
                    id: module_id.clone(),
                    kind: ModuleKind::Files,
                    name: swift_identifier(&module_id),
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
                modules.push(ResourceModule {
                    id: module_id.clone(),
                    kind: ModuleKind::Fonts,
                    name: swift_identifier(&module_id),
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
    use blake3::Hasher;
    use camino::Utf8PathBuf;
    use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry, ResourceEntry, swift_identifier};
    use serde_json::json;
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

    fn entry(name: &str, kind: EntryKind) -> ResourceEntry {
        ResourceEntry {
            id: name.to_string(),
            name: name.to_string(),
            source_path: Utf8PathBuf::from("fixture"),
            swift_identifier: swift_identifier(name),
            kind,
            children: Vec::new(),
            properties: Metadata::new(),
            metadata: Metadata::new(),
        }
    }

    #[test]
    fn file_sort_keys_match_case_insensitive_name_ordering() {
        let sibling_names = [
            "YouTubePlayer.html",
            "youtube_embed.html",
            "backgroundMusic.mp3",
            "backHome.mp3",
            "miniSlot",
            "Spy",
            "greedy_drawing.mp3",
            "greedy_drawing_end.MP3",
            "jackpot_select.mp3",
            "jackpot_select_luxury.mp3",
            "play_center_list_item_new_tag.svga",
            "play_center_list_item_new_tag_ar.svga",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

        let mut ordered = sibling_names
            .iter()
            .map(|name| (name.as_str(), swiftgen_file_sort_key(name, &sibling_names)))
            .collect::<Vec<_>>();
        ordered.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(right.0)));

        assert_eq!(
            ordered
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>(),
            vec![
                "backgroundMusic.mp3",
                "backHome.mp3",
                "greedy_drawing.mp3",
                "greedy_drawing_end.MP3",
                "jackpot_select.mp3",
                "jackpot_select_luxury.mp3",
                "miniSlot",
                "play_center_list_item_new_tag.svga",
                "play_center_list_item_new_tag_ar.svga",
                "Spy",
                "youtube_embed.html",
                "YouTubePlayer.html",
            ]
        );
    }

    #[test]
    fn assets_sort_only_moves_nine_patch_before_base() {
        let mut entries = vec![
            entry("bet_bubble tips_up", EntryKind::Image),
            entry("bet_bubble_tips", EntryKind::Image),
            entry("bet_bubble_tips_down", EntryKind::Image),
            entry("room_task_list_bg", EntryKind::Image),
            entry("room_task_list_bg.9", EntryKind::Image),
        ];

        sort_entries_for_assets(&mut entries);

        let ids = entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "bet_bubble tips_up",
                "bet_bubble_tips",
                "bet_bubble_tips_down",
                "room_task_list_bg.9",
                "room_task_list_bg",
            ]
        );
    }

    #[test]
    fn builtin_template_fingerprint_record_includes_language_and_name() {
        let record = GenerationFingerprintRecord {
            schema_version: GENERATION_FINGERPRINT_SCHEMA_VERSION,
            job_name: "assets".to_string(),
            output: "Generated/Assets.swift".to_string(),
            access_level: "internal".to_string(),
            bundle_mode: "module".to_string(),
            bundle_identifier: None,
            inputs: Vec::new(),
            template: GenerationTemplateFingerprintRecord::Builtin {
                language: "objc".to_string(),
                name: "assets".to_string(),
                fingerprint: "fingerprint".to_string(),
            },
        };

        let serialized = serde_json::to_value(&record).expect("record should serialize");

        assert_eq!(serialized["template"]["kind"], "Builtin");
        assert_eq!(serialized["template"]["language"], "objc");
        assert_eq!(serialized["template"]["name"], "assets");
        assert_eq!(serialized["template"]["fingerprint"], "fingerprint");
    }

    fn write_strings_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
        )
        .expect("config should be written");
    }

    fn write_extensionless_l10n_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/l10n"
"#,
        )
        .expect("config should be written");
    }

    fn write_custom_files_job_config(config_path: &Path, incremental: Option<bool>) {
        let incremental_line = incremental
            .map(|value| format!("incremental = {value}\n"))
            .unwrap_or_default();
        fs::write(
            config_path,
            format!(
                r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"
{incremental_line}
[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
path = "Templates/files.jinja"
"#
            ),
        )
        .expect("config should be written");
    }

    fn write_xcstrings_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
        )
        .expect("config should be written");
    }

    fn write_files_job_config(config_path: &Path) {
        fs::write(
            config_path,
            r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
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
        let mut hasher = Hasher::new();
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
            .join(format!("{}.json", hasher.finalize().to_hex()))
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

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
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

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
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

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
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
    fn generate_resolves_extensionless_template_path_to_jinja_file() {
        let temp_dir = make_temp_dir("pipeline-extensionless-template-path");
        let config_path = temp_dir.join("numi.toml");
        let localization_root = temp_dir.join("Resources/Localization/en.lproj");
        let template_path = temp_dir.join("Templates/l10n.jinja");
        let generated_path = temp_dir.join("Generated/L10n.swift");

        fs::create_dir_all(&localization_root).expect("localization dir should exist");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template dir should exist");
        fs::write(
            localization_root.join("Localizable.strings"),
            "\"profile.title\" = \"Profile\";\n",
        )
        .expect("strings file should be written");
        fs::write(
            &template_path,
            "{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
        )
        .expect("template should be written");
        write_extensionless_l10n_job_config(&config_path);

        let report = generate(&config_path, None).expect("generation should succeed");
        let rendered = fs::read_to_string(&generated_path).expect("output should be written");

        assert_eq!(report.jobs.len(), 1);
        assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
        assert_eq!(rendered, "L10n|Localizable\n");

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

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
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
    fn generate_writes_objc_builtin_files_accessors() {
        let temp_dir = make_temp_dir("pipeline-objc-files-generate");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let generated_path = temp_dir.join("Generated/Files.h");

        fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
            .expect("video file should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[jobs.files]
output = "Generated/Files.h"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
        )
        .expect("config should be written");

        let report = generate(&config_path, None).expect("generation should succeed");
        let rendered = fs::read_to_string(&generated_path).expect("output should be written");

        assert_eq!(report.jobs.len(), 1);
        assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
        assert!(!rendered.contains("@implementation"));
        assert!(rendered.contains("NS_INLINE NSURL *FilesFixturesOnboardingWelcomeVideoMp4(void)"));
        assert!(rendered.contains("SWIFTPM_MODULE_BUNDLE"));
        assert!(!rendered.contains("bundleForClass:"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generation_fingerprint_changes_when_builtin_language_changes() {
        let temp_dir = make_temp_dir("pipeline-fingerprint-builtin-language");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");

        fs::create_dir_all(&files_root).expect("files directory should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(
            &config_path,
            r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
        )
        .expect("config should be written");

        let loaded = numi_config::load_from_path(&config_path).expect("config should load");
        let config_dir = config_path.parent().expect("config should have parent");
        let selected_jobs = vec!["files".to_string()];
        let swift_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
            .expect("files job should resolve");
        let swift_job = swift_jobs
            .into_iter()
            .next()
            .expect("files job should exist");
        let swift_fingerprint =
            compute_generation_fingerprint(config_dir, &loaded.config.defaults, swift_job)
                .expect("swift builtin fingerprint should compute");

        fs::write(
            &config_path,
            r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
        )
        .expect("objc config should be written");

        let loaded = numi_config::load_from_path(&config_path).expect("objc config should load");
        let objc_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
            .expect("files job should resolve");
        let objc_job = objc_jobs
            .into_iter()
            .next()
            .expect("files job should exist");
        let objc_fingerprint =
            compute_generation_fingerprint(config_dir, &loaded.config.defaults, objc_job)
                .expect("objc builtin fingerprint should compute");

        assert_ne!(swift_fingerprint.fingerprint, objc_fingerprint.fingerprint);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_skips_when_generation_contract_is_unchanged_by_default() {
        let temp_dir = make_temp_dir("pipeline-generate-skip-default");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let template_path = temp_dir.join("Templates/files.jinja");
        let generated_path = temp_dir.join("Generated/Files.swift");

        fs::create_dir_all(&files_root).expect("files directory should exist");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template dir should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(
            &template_path,
            "{{ modules[0].entries[0].properties.fileName }}\n",
        )
        .expect("template should be written");
        write_custom_files_job_config(&config_path, None);

        let first = generate(&config_path, None).expect("initial generation should succeed");
        assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);
        assert_eq!(
            fs::read_to_string(&generated_path).expect("generated file should exist"),
            "faq.pdf\n"
        );

        let second = generate(&config_path, None).expect("second generation should succeed");
        assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
        assert_eq!(
            fs::read_to_string(&generated_path).expect("generated file should remain"),
            "faq.pdf\n"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_respects_job_incremental_opt_out_and_rerenders() {
        let temp_dir = make_temp_dir("pipeline-generate-opt-out");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let template_path = temp_dir.join("Templates/files.jinja");
        let generated_path = temp_dir.join("Generated/Files.swift");

        fs::create_dir_all(&files_root).expect("files directory should exist");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template dir should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(
            &template_path,
            "{{ modules[0].entries[0].properties.fileName }}\n",
        )
        .expect("template should be written");
        write_custom_files_job_config(&config_path, Some(false));

        let first = generate(&config_path, None).expect("initial generation should succeed");
        assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

        let second = generate(&config_path, None).expect("second generation should rerender");
        assert_eq!(second.jobs[0].outcome, WriteOutcome::Unchanged);
        assert_eq!(
            fs::read_to_string(&generated_path).expect("generated file should remain"),
            "faq.pdf\n"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn generate_options_override_job_incremental_setting() {
        let temp_dir = make_temp_dir("pipeline-generate-options-override");
        let config_path = temp_dir.join("numi.toml");
        let files_root = temp_dir.join("Resources/Fixtures");
        let template_path = temp_dir.join("Templates/files.jinja");
        let generated_path = temp_dir.join("Generated/Files.swift");

        fs::create_dir_all(&files_root).expect("files directory should exist");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template dir should exist");
        fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
        fs::write(
            &template_path,
            "{{ modules[0].entries[0].properties.fileName }}\n",
        )
        .expect("template should be written");
        write_custom_files_job_config(&config_path, Some(false));

        let first = generate(&config_path, None).expect("initial generation should succeed");
        assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

        let second = generate_with_options(
            &config_path,
            None,
            GenerateOptions {
                incremental: Some(true),
            },
        )
        .expect("second generation should honor the explicit override");
        assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
        assert_eq!(
            fs::read_to_string(&generated_path).expect("generated file should remain"),
            "faq.pdf\n"
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

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
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
                None,
                None,
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
                None,
                None,
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
