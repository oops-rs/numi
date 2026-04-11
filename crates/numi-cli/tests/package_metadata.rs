use std::path::PathBuf;

#[test]
fn package_metadata_uses_numi_name_and_docs_url() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = manifest_dir.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("failed to read Cargo.toml");
    let parsed: toml::Value = manifest.parse().expect("Cargo.toml should parse");

    assert_eq!(parsed["package"]["name"].as_str(), Some("numi"));
    assert_eq!(
        parsed["package"]["documentation"].as_str(),
        Some("https://docs.rs/numi")
    );
}
