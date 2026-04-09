use std::{
    fs,
    path::PathBuf,
    process::Command,
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
fn dump_context_emits_strings_placeholder_metadata() {
    let temp_root = make_temp_dir("dump-context-strings-placeholders");
    let working_root = temp_root.join("fixture");
    let resources_root = working_root.join("Resources");
    let templates_root = working_root.join("Templates");
    fs::create_dir_all(&resources_root).expect("resources directory should exist");
    fs::create_dir_all(&templates_root).expect("templates directory should exist");

    fs::write(
        resources_root.join("Localizable.strings"),
        "\"welcome.message\" = \"Hello %@, you have %d coins\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        working_root.join("numi.toml"),
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources"

[jobs.l10n.template]
path = "Templates/strings.jinja"
"#,
    )
    .expect("config should be written");
    fs::write(templates_root.join("strings.jinja"), "{{ job.name }}\n")
        .expect("template should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "numi.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");

    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["placeholders"],
        serde_json::json!([
            {"format": "@", "swiftType": "String"},
            {"format": "d", "swiftType": "Int"}
        ])
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_sorts_strings_by_raw_key_and_keeps_last_duplicate_value() {
    let temp_root = make_temp_dir("dump-context-strings-order");
    let working_root = temp_root.join("fixture");
    let resources_root = working_root.join("Resources");
    let templates_root = working_root.join("Templates");
    fs::create_dir_all(&resources_root).expect("resources directory should exist");
    fs::create_dir_all(&templates_root).expect("templates directory should exist");

    fs::write(
        resources_root.join("Localizable.strings"),
        concat!(
            "\"rank_title\" = \"Rankings\";\n",
            "\"account_area_code\" = \"+%@ old\";\n",
            "\"_50000\" = \"50000\";\n",
            "\"AD\" = \"Andorra\";\n",
            "\"account_area_code\" = \"+%@ new\";\n",
        ),
    )
    .expect("strings file should be written");
    fs::write(
        working_root.join("numi.toml"),
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources"

[jobs.l10n.template]
path = "Templates/strings.jinja"
"#,
    )
    .expect("config should be written");
    fs::write(templates_root.join("strings.jinja"), "{{ job.name }}\n")
        .expect("template should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "numi.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let entries = json["modules"][0]["entries"]
        .as_array()
        .expect("entries should be an array");

    let ordered_keys = entries
        .iter()
        .map(|entry| {
            entry["properties"]["key"]
                .as_str()
                .expect("key should be a string")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ordered_keys,
        vec!["_50000", "account_area_code", "AD", "rank_title"]
    );

    assert_eq!(
        entries[1]["properties"]["translation"],
        serde_json::Value::String("+%@ new".to_string())
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
