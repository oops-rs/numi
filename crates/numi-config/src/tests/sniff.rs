use crate::{
    DiscoveryError, ManifestKindSniff, discover_config, discover_workspace_ancestor,
    load_workspace_from_path, sniff_manifest_kind_str,
};
use std::fs;

use super::{create_temp_dir, write_file};

#[test]
fn sniffs_broken_workspace_like_manifests_without_classifying_them_as_unknown() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
[workspace]
members = [
"#
        ),
        ManifestKindSniff::BrokenWorkspaceLike
    );
}

#[test]
fn sniffs_broken_mixed_manifests_as_mixed() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
jobs = {}
members = [
"#
        ),
        ManifestKindSniff::Mixed
    );
}

#[test]
fn sniffs_legacy_top_level_members_manifest_as_workspace_like() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
members = [{ config = "AppUI/numi.toml" }]
"#
        ),
        ManifestKindSniff::WorkspaceLike
    );
}

#[test]
fn sniffs_mixed_manifests_without_fully_loading_them() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
jobs = {}
members = [{ config = "AppUI/numi.toml" }]
"#
        ),
        ManifestKindSniff::Mixed
    );
}

#[test]
fn sniffs_workspaceish_unparsable_manifests_as_broken_workspace_like() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
members = [
"#
        ),
        ManifestKindSniff::BrokenWorkspaceLike
    );
}

#[test]
fn discovers_workspace_manifest_in_ancestors_only() {
    let ancestor_root = create_temp_dir("workspace-discovery-ancestor");
    let ancestor_manifest = ancestor_root.join("numi.toml");
    write_file(&ancestor_manifest, "version = 1\n[workspace]\nmembers = [\"App\"]\n");

    let nested = ancestor_root.join("apps/ios/App");
    fs::create_dir_all(&nested).expect("nested directory should exist");

    let discovered = discover_workspace_ancestor(&nested, None)
        .expect("ancestor workspace manifest should be discovered");
    assert_eq!(
        discovered,
        ancestor_manifest
            .canonicalize()
            .expect("manifest path should canonicalize")
    );

    let descendant_root = create_temp_dir("workspace-discovery-descendant");
    write_file(
        &descendant_root.join("apps/App/numi.toml"),
        "version = 1\n[workspace]\nmembers = [\"App\"]\n",
    );

    let error = discover_workspace_ancestor(&descendant_root, None)
        .expect_err("descendant workspace manifests should not be discovered");
    match error {
        DiscoveryError::NotFound { start_dir } => assert_eq!(
            start_dir,
            descendant_root
                .canonicalize()
                .expect("path should canonicalize")
        ),
        other => panic!("expected not found discovery error, got {other:?}"),
    }
}

#[test]
fn workspace_load_errors_use_workspace_manifest_language() {
    let missing = create_temp_dir("workspace-load-error").join("missing-workspace.toml");
    let error = load_workspace_from_path(&missing)
        .expect_err("missing workspace manifest should return a read error");
    let message = error.to_string();
    assert!(message.contains("workspace numi.toml"));
    assert!(!message.contains("failed to read config"));

    let temp_dir = create_temp_dir("workspace-parse-error");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(&manifest_path, "not = [valid");

    let error = load_workspace_from_path(&manifest_path)
        .expect_err("invalid workspace manifest should return a parse error");
    let message = error.to_string();
    assert!(message.contains("workspace numi.toml TOML"));
    assert!(!message.contains("config TOML"));
}

#[test]
fn workspace_discovery_errors_use_workspace_manifest_language() {
    let temp_dir = create_temp_dir("workspace-discovery-not-found");
    let error = discover_workspace_ancestor(&temp_dir, None)
        .expect_err("missing workspace manifest should be reported");
    let message = error.to_string();
    assert!(message.contains("No configuration file found from"));
    assert!(message.contains("numi config locate --config <path>"));

    let explicit = temp_dir.join("missing-workspace.toml");
    let error = discover_workspace_ancestor(&temp_dir, Some(&explicit))
        .expect_err("missing explicit workspace manifest should be reported");
    assert!(error.to_string().contains("config file not found"));
}

#[test]
fn discovers_config_manifest_in_ancestors_only() {
    let ancestor_root = create_temp_dir("config-discovery-ancestor");
    let ancestor_manifest = ancestor_root.join("numi.toml");
    write_file(&ancestor_manifest, "version = 1\njobs = []\n");

    let nested = ancestor_root.join("apps/ios/App");
    fs::create_dir_all(&nested).expect("nested directory should exist");

    let discovered =
        discover_config(&nested, None).expect("ancestor config manifest should be discovered");
    assert_eq!(
        discovered,
        ancestor_manifest
            .canonicalize()
            .expect("manifest path should canonicalize")
    );

    let descendant_root = create_temp_dir("config-discovery-descendant");
    write_file(
        &descendant_root.join("apps/App/numi.toml"),
        "version = 1\njobs = []\n",
    );

    let error = discover_config(&descendant_root, None)
        .expect_err("descendant config manifests should not be discovered");
    match error {
        DiscoveryError::NotFound { start_dir } => assert_eq!(
            start_dir,
            descendant_root
                .canonicalize()
                .expect("path should canonicalize")
        ),
        other => panic!("expected not found discovery error, got {other:?}"),
    }
}
