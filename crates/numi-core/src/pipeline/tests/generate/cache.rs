use super::super::{
    make_temp_dir,
    seed_cached_parse,
    write_strings_job_config,
    with_temp_dir_override,
};
use super::super::super::{generate, load_or_parse_cached, GenerateError};
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
use serde_json::json;
use crate::{
    parse_cache::{CacheKind, CachedParseData},
    parse_l10n::LocalizationTable,
    parse_xcassets::XcassetsReport,
    WriteOutcome,
};
use std::fs;

#[test]
fn generate_uses_cached_xcassets_parse_payload_on_cache_hit() {
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
            properties: Metadata::from([("assetName".to_string(), json!("CachedPalette"))]),
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
}


#[test]
fn generate_uses_cached_strings_parse_payload_on_cache_hit() {
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
}


#[test]
fn cache_store_skips_entries_when_inputs_change_during_parse() {
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
        fs::write(&input_file, "after").expect("fixture file should mutate during parse");
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
