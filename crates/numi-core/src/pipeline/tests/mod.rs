use crate::{
    generation_cache,
    parse_cache::{self, CacheKind, CachedParseData},
};
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, ResourceEntry, swift_identifier};
use std::{
    fs,
    path::Path,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn make_temp_dir(test_name: &str) -> PathBuf {
    let unique = format!(
        "numi-{test_name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    );
    // Some tests intentionally override TMPDIR to a file path. Use a stable
    // scratch root so unrelated tests never inherit that override.
    let temp_root = if cfg!(unix) {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    assert!(temp_root.is_dir(), "temp dir root should exist");
    let path = temp_root.join(unique);
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

pub(super) fn entry(name: &str, kind: EntryKind) -> ResourceEntry {
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

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn utf16be(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

pub(super) fn make_test_font_bytes(family: &str, style: &str, post_script_name: &str) -> Vec<u8> {
    let full_name = if style == "Regular" {
        family.to_string()
    } else {
        format!("{family} {style}")
    };
    let name_records = [
        (1_u16, utf16be(family)),
        (2_u16, utf16be(style)),
        (4_u16, utf16be(&full_name)),
        (6_u16, utf16be(post_script_name)),
    ];

    let string_offset = 6 + (name_records.len() as u16 * 12);
    let mut name_table = Vec::new();
    push_u16(&mut name_table, 0);
    push_u16(&mut name_table, name_records.len() as u16);
    push_u16(&mut name_table, string_offset);

    let mut storage = Vec::new();
    for (name_id, encoded) in &name_records {
        push_u16(&mut name_table, 3);
        push_u16(&mut name_table, 1);
        push_u16(&mut name_table, 0x0409);
        push_u16(&mut name_table, *name_id);
        push_u16(&mut name_table, encoded.len() as u16);
        push_u16(&mut name_table, storage.len() as u16);
        storage.extend_from_slice(encoded);
    }
    name_table.extend_from_slice(&storage);

    let table_offset = 12 + 16;
    let mut font = Vec::new();
    push_u32(&mut font, 0x0001_0000);
    push_u16(&mut font, 1);
    push_u16(&mut font, 16);
    push_u16(&mut font, 0);
    push_u16(&mut font, 0);
    font.extend_from_slice(b"name");
    push_u32(&mut font, 0);
    push_u32(&mut font, table_offset as u32);
    push_u32(&mut font, name_table.len() as u32);
    font.extend_from_slice(&name_table);
    while font.len() % 4 != 0 {
        font.push(0);
    }
    font
}


pub(super) fn write_strings_job_config(config_path: &Path) {
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


pub(super) fn write_custom_files_job_config(config_path: &Path, incremental: Option<bool>) {
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

pub(super) fn write_xcstrings_job_config(config_path: &Path) {
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

pub(super) fn seed_cached_parse(
    kind: CacheKind,
    input_path: &Path,
    data: CachedParseData,
) -> Result<(), parse_cache::CacheError> {
    let fingerprint = parse_cache::fingerprint_input(kind, input_path)?;
    parse_cache::store(kind, input_path, &fingerprint, &data)
}

pub(super) fn with_temp_dir_override<T>(temp_dir: &Path, f: impl FnOnce() -> T) -> T {
    generation_cache::with_test_cache_root_override(temp_dir, || {
        parse_cache::with_test_cache_root_override(temp_dir, f)
    })
}

mod check;
mod context;
mod generate;
mod helpers;
