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
fn generate_writes_files_accessors_from_fixture() {
    let temp_root = make_temp_dir("generate-files");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/Files.swift"))
        .expect("generated files output should exist");

    assert_eq!(
        generated,
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

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn repeated_files_generate_is_byte_stable() {
    let temp_root = make_temp_dir("generate-files-stable");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let first = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("first numi generate should run");

    assert!(
        first.status.success(),
        "first command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let generated_path = working_root.join("Generated/Files.swift");
    let first_bytes = fs::read(&generated_path).expect("first generated files output should exist");
    let first_modified = fs::metadata(&generated_path)
        .expect("first generated output metadata should exist")
        .modified()
        .expect("first generated output mtime should exist");

    thread::sleep(Duration::from_millis(20));

    let second = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("second numi generate should run");

    assert!(
        second.status.success(),
        "second command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let second_bytes =
        fs::read(&generated_path).expect("second generated files output should still exist");
    let second_modified = fs::metadata(&generated_path)
        .expect("second generated output metadata should exist")
        .modified()
        .expect("second generated output mtime should exist");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first_modified, second_modified);

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_emits_files_module_kind_and_properties() {
    let temp_root = make_temp_dir("dump-context-files");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "numi.toml", "--job", "files"])
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

    assert_eq!(json["modules"][0]["kind"], "files");
    assert_eq!(json["modules"][0]["name"], "Fixtures");
    assert_eq!(json["modules"][0]["entries"][0]["kind"], "namespace");
    assert_eq!(json["modules"][0]["entries"][0]["name"], "Onboarding");
    assert_eq!(
        json["modules"][0]["entries"][0]["children"][0]["kind"],
        "data"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["children"][0]["properties"]["relativePath"],
        "Onboarding/welcome-video.mp4"
    );
    assert_eq!(json["modules"][0]["entries"][1]["kind"], "data");
    assert_eq!(
        json["modules"][0]["entries"][1]["properties"]["fileName"],
        "faq.pdf"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
