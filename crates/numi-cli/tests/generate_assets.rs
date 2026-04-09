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
fn generate_writes_swiftui_assets_from_xcassets_fixture() {
    let temp_root = make_temp_dir("generate-assets");
    let fixture_root = repo_root().join("fixtures/xcassets-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/Assets.swift"))
        .expect("generated assets file should exist");

    assert!(generated.contains("ImageAsset(name: \"Icons/add\")"));
    assert!(generated.contains("ColorAsset(name: \"Brand\")"));

    assert_eq!(
        generated,
        r#"import SwiftUI

internal enum Assets {
    internal static let brand = ColorAsset(name: "Brand")
    internal enum Icons {
        internal static let add = ImageAsset(name: "Icons/add")
    }
}

internal struct ColorAsset {
    internal let name: String
}

internal struct ImageAsset {
    internal let name: String
}
"#
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn repeated_generate_is_byte_stable() {
    let temp_root = make_temp_dir("generate-assets-stable");
    let fixture_root = repo_root().join("fixtures/xcassets-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let first = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml"])
        .current_dir(&working_root)
        .output()
        .expect("first numi generate should run");

    assert!(
        first.status.success(),
        "first command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );

    let generated_path = working_root.join("Generated/Assets.swift");
    let first_bytes = fs::read(&generated_path).expect("first generated assets file should exist");
    let first_modified = fs::metadata(&generated_path)
        .expect("first generated assets metadata should exist")
        .modified()
        .expect("first generated assets mtime should exist");

    thread::sleep(Duration::from_millis(20));

    let second = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml"])
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
        fs::read(&generated_path).expect("second generated assets file should exist");
    let second_modified = fs::metadata(&generated_path)
        .expect("second generated assets metadata should exist")
        .modified()
        .expect("second generated assets mtime should exist");

    assert_eq!(first_bytes, second_bytes);
    assert_eq!(first_modified, second_modified);

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
