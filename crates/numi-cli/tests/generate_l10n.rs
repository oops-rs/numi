use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should exist");

    for entry in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("file type should be readable");

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination_path);
        } else {
            fs::copy(entry.path(), destination_path).expect("fixture file should copy");
        }
    }
}

#[test]
fn generate_writes_l10n_accessors_from_strings() {
    let temp_root = make_temp_dir("generate-l10n");
    let fixture_root = repo_root().join("fixtures/l10n-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/L10n.swift"))
        .expect("generated l10n file should exist");

    assert_eq!(
        generated,
        r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let profileTitle = tr("Localizable", "profile.title")
        internal static let settingsLogout = tr("Localizable", "settings.logout")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_writes_l10n_accessors_from_xcstrings() {
    let temp_root = make_temp_dir("generate-xcstrings");
    let fixture_root = repo_root().join("fixtures/xcstrings-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/L10n.swift"))
        .expect("generated l10n file should exist");

    assert_eq!(
        generated,
        r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let greetingMessage = tr("Localizable", "greeting.message")
        internal static let profileTitle = tr("Localizable", "profile.title")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn repeated_l10n_generate_is_byte_stable() {
    let temp_root = make_temp_dir("generate-l10n-stable");
    let fixture_root = repo_root().join("fixtures/l10n-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let first = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("first numi generate should run");

    assert!(
        first.status.success(),
        "first command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let generated_path = working_root.join("Generated/L10n.swift");
    let first_bytes = fs::read(&generated_path).expect("first generated l10n file should exist");
    let first_modified = fs::metadata(&generated_path)
        .expect("first generated l10n metadata should exist")
        .modified()
        .expect("first generated l10n mtime should exist");

    thread::sleep(Duration::from_millis(20));

    let second = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("second numi generate should run");

    assert!(
        second.status.success(),
        "second command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let second_bytes = fs::read(&generated_path).expect("second generated l10n file should exist");
    let second_modified = fs::metadata(&generated_path)
        .expect("second generated l10n metadata should exist")
        .modified()
        .expect("second generated l10n mtime should exist");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first_modified, second_modified);

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_warns_and_succeeds_for_skipped_xcstrings_variations() {
    let temp_root = make_temp_dir("generate-xcstrings-warning");
    let working_root = temp_root.join("fixture");
    let localization_root = working_root.join("Resources/Localization");
    fs::create_dir_all(&localization_root).expect("localization directory should exist");
    fs::write(
        working_root.join("swiftgen.toml"),
        r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.template]
builtin = "l10n"
"#,
    )
    .expect("config should be written");
    fs::write(
        localization_root.join("Localizable.xcstrings"),
        r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "things.label": {
      "localizations": {
        "en": {
          "variations": {
            "plural": {
              "one": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%lld thing"
                }
              },
              "other": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%lld things"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
    )
    .expect("xcstrings file should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning: skipping xcstrings key `things.label`"));
    assert!(stderr.contains("unsupported plural variations"));

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_emits_json_for_selected_job() {
    let fixture_root = repo_root().join("fixtures/l10n-basic");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&fixture_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(json["job"]["name"], "l10n");
    assert_eq!(json["job"]["swiftIdentifier"], "L10n");
    assert_eq!(json["modules"][0]["kind"], "strings");
    assert_eq!(json["modules"][0]["name"], "Localizable");
    assert_eq!(json["modules"][0]["properties"]["tableName"], "Localizable");
    assert_eq!(json["modules"][0]["entries"][0]["kind"], "string");
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["key"],
        "profile.title"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["translation"],
        "Profile"
    );
}

#[test]
fn dump_context_emits_xcstrings_module_kind_and_placeholders() {
    let fixture_root = repo_root().join("fixtures/xcstrings-basic");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&fixture_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(json["job"]["name"], "l10n");
    assert_eq!(json["job"]["swiftIdentifier"], "L10n");
    assert_eq!(json["modules"][0]["kind"], "xcstrings");
    assert_eq!(json["modules"][0]["name"], "Localizable");
    assert_eq!(json["modules"][0]["properties"]["tableName"], "Localizable");
    assert_eq!(json["modules"][0]["entries"][0]["kind"], "string");
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["key"],
        "greeting.message"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["translation"],
        "Hello %#@name@, you have %#@count@ messages"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["placeholders"],
        serde_json::json!([
            {"name": "count", "format": "lld", "swiftType": "Int"},
            {"name": "name", "format": "@", "swiftType": "String"}
        ])
    );
    let second_entry_properties = json["modules"][0]["entries"][1]["properties"]
        .as_object()
        .expect("second entry properties should be an object");
    assert!(!second_entry_properties.contains_key("placeholders"));
}
