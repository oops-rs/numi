use super::*;
use crate::{
    generation_cache,
    parse_cache::{self, CacheKind, CachedParseData},
    parse_l10n::LocalizationTable,
    parse_xcassets::XcassetsReport,
};
use blake3::Hasher;
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry, ResourceEntry, swift_identifier};
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::MAIN_SEPARATOR,
    path::Path,
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

fn make_test_font_bytes(family: &str, style: &str, post_script_name: &str) -> Vec<u8> {
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

fn write_custom_files_job_config_with_hooks(
    config_path: &Path,
    incremental: Option<bool>,
    pre_generate: Option<&[String]>,
    post_generate: Option<&[String]>,
) {
    let incremental_line = incremental
        .map(|value| format!("incremental = {value}\n"))
        .unwrap_or_default();
    let mut manifest = format!(
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
    );

    if let Some(command) = pre_generate {
        manifest.push_str("\n[jobs.files.hooks.pre_generate]\n");
        manifest.push_str(&format!("command = {}\n", toml_array(command)));
    }

    if let Some(command) = post_generate {
        manifest.push_str("\n[jobs.files.hooks.post_generate]\n");
        manifest.push_str(&format!("command = {}\n", toml_array(command)));
    }

    fs::write(config_path, manifest).expect("config should be written");
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

fn toml_array(values: &[String]) -> String {
    let parts = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>();
    format!("[{}]", parts.join(", "))
}

fn write_hook_probe_script(root: &Path, name: &str, exit_code: i32) -> String {
    let scripts_root = root.join("Scripts");
    fs::create_dir_all(&scripts_root).expect("scripts dir should exist");
    let file_name = if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        format!("{name}.sh")
    };
    let script_path = scripts_root.join(&file_name);
    let script_body = if cfg!(windows) {
        format!(
            "@echo off\r\nsetlocal\r\n>> \"%~1\" echo %NUMI_HOOK_PHASE%^|%NUMI_HOOK_JOB_NAME%^|%NUMI_HOOK_OUTPUT_PATH%^|%NUMI_HOOK_OUTPUT_DIR%^|%NUMI_HOOK_CONFIG_PATH%^|%NUMI_HOOK_WRITE_OUTCOME%^|%NUMI_HOOK_WORKSPACE_CONFIG_PATH%\r\nexit /b {exit_code}\r\n"
        )
    } else {
        format!(
            "#!/bin/sh\nlog_path=\"$1\"\nprintf '%s|%s|%s|%s|%s|%s|%s\\n' \"$NUMI_HOOK_PHASE\" \"$NUMI_HOOK_JOB_NAME\" \"$NUMI_HOOK_OUTPUT_PATH\" \"$NUMI_HOOK_OUTPUT_DIR\" \"$NUMI_HOOK_CONFIG_PATH\" \"${{NUMI_HOOK_WRITE_OUTCOME-}}\" \"${{NUMI_HOOK_WORKSPACE_CONFIG_PATH-}}\" >> \"$log_path\"\nexit {exit_code}\n"
        )
    };
    fs::write(&script_path, script_body).expect("hook script should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .expect("script permissions should be updated");
    }

    PathBuf::from("Scripts")
        .join(file_name)
        .display()
        .to_string()
        .replace(MAIN_SEPARATOR, "/")
}

fn seed_cached_parse(
    kind: CacheKind,
    input_path: &Path,
    data: CachedParseData,
) -> Result<(), parse_cache::CacheError> {
    let fingerprint = parse_cache::fingerprint_input(kind, input_path)?;
    parse_cache::store(kind, input_path, &fingerprint, &data)
}

fn with_temp_dir_override<T>(temp_dir: &Path, f: impl FnOnce() -> T) -> T {
    generation_cache::with_test_cache_root_override(temp_dir, || {
        parse_cache::with_test_cache_root_override(temp_dir, f)
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



mod check;
mod context;
mod generate;
