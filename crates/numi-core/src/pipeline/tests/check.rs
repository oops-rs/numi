use super::super::{check, generate};
use super::{make_temp_dir, seed_cached_parse, with_temp_dir_override};
use crate::parse_cache::{CacheKind, CachedParseData};
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde_json::json;
use std::{fs, path::Path};

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

#[test]
fn check_uses_cached_files_parse_and_still_reports_stale_outputs() {
    let temp_dir = make_temp_dir("pipeline-files-check-cache-hit");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    write_files_job_config(&config_path);

    generate(&config_path, None).expect("initial generation should succeed");
    let generated_path = temp_dir.join("Generated/Files.swift");
    let baseline = fs::read_to_string(&generated_path).expect("generated output should exist");
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
