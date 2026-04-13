use crate::{ManifestKindSniff, sniff_manifest_kind_str};

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
[workspace]
members = [{ config = "AppUI/numi.toml" }]
"#
        ),
        ManifestKindSniff::WorkspaceLike
    );
}
