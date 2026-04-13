use super::{entry, make_temp_dir, seed_cached_parse, write_xcstrings_job_config};
use super::super::{
    dump_context, sort_entries_for_assets, swiftgen_file_sort_key,
    GenerationFingerprintRecord, GenerationTemplateFingerprintRecord,
    GENERATION_FINGERPRINT_SCHEMA_VERSION,
};
use blake3::Hasher;
use camino::Utf8PathBuf;
use crate::{
    parse_cache::{CacheKind, CachedParseData},
    parse_l10n::LocalizationTable,
};
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

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
fn dump_context_uses_cached_xcstrings_parse_and_keeps_json_stable() {
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
}


#[test]
fn dump_context_degrades_when_cached_record_is_invalid() {
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
}
