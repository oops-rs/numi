use std::path::PathBuf;

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
