use std::{fs, fs::OpenOptions, path::PathBuf, thread, time::Duration};

#[test]
fn package_metadata_uses_numi_name_and_docs_url() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = manifest_dir.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("failed to read Cargo.toml");
    let parsed: toml::Value = toml::from_str(&manifest).expect("Cargo.toml should parse");

    assert_eq!(parsed["package"]["name"].as_str(), Some("numi"));
    assert_eq!(
        parsed["package"]["documentation"].as_str(),
        Some("https://docs.rs/numi")
    );
}

#[test]
fn starter_config_is_embedded_from_within_the_cli_crate() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lib_rs = std::fs::read_to_string(manifest_dir.join("src/lib.rs"))
        .expect("failed to read src/lib.rs");

    assert!(
        lib_rs.contains("include_str!(\"../assets/starter-numi.toml\")"),
        "expected src/lib.rs to embed the crate-local starter config"
    );
    assert!(
        !lib_rs.contains("../../../docs/examples/starter-numi.toml"),
        "src/lib.rs should not reference starter config outside the crate root"
    );
}

fn starter_config_lock_path() -> PathBuf {
    std::env::temp_dir().join("numi-cli-starter-config.lock")
}

struct StarterConfigLock {
    path: PathBuf,
}

impl Drop for StarterConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_starter_config_lock() -> StarterConfigLock {
    let path = starter_config_lock_path();

    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return StarterConfigLock { path },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed to acquire starter config lock: {error}"),
        }
    }
}

#[test]
fn starter_config_asset_and_docs_copy_stay_in_sync() {
    let _lock = acquire_starter_config_lock();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let asset = fs::read_to_string(manifest_dir.join("assets/starter-numi.toml"))
        .expect("failed to read crate-local starter config");
    let docs = fs::read_to_string(manifest_dir.join("../../docs/examples/starter-numi.toml"))
        .expect("failed to read docs starter config");

    assert_eq!(
        asset, docs,
        "starter config asset and docs copy should match"
    );
}
