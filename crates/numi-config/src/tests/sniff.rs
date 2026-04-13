use crate::{
    DiscoveryError, ManifestKindSniff, discover_config, sniff_manifest_kind_str,
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
fn sniffs_inline_table_workspace_manifest_as_workspace_like() {
    assert_eq!(
        sniff_manifest_kind_str(
            r#"
version = 1
workspace={members=["AppUI"]}
"#
        ),
        ManifestKindSniff::WorkspaceLike
    );
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
