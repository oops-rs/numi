use crate::{
    DiscoveryError, Manifest, discover_workspace_ancestor, load_workspace_from_path,
    parse_manifest_str,
};
use std::fs;

use super::{create_temp_dir, write_file};

#[test]
fn parses_unified_workspace_manifest() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"

[workspace.member_overrides.AppUI]
jobs = ["assets", "l10n"]
"#,
    )
    .expect("workspace manifest should parse");

    match manifest {
        Manifest::Workspace(workspace) => {
            assert_eq!(workspace.version, 1);
            assert_eq!(workspace.workspace.members, vec!["AppUI", "Core"]);
            assert_eq!(
                workspace
                    .members()
                    .iter()
                    .map(|member| member.config.as_str())
                    .collect::<Vec<_>>(),
                vec!["AppUI/numi.toml", "Core/numi.toml"]
            );
            assert_eq!(
                workspace.workspace.defaults.jobs["l10n"]
                    .template
                    .builtin
                    .as_ref()
                    .and_then(|builtin| builtin.language.as_deref()),
                Some("swift")
            );
            assert!(
                workspace.workspace.defaults.jobs["l10n"]
                    .template
                    .builtin
                    .as_ref()
                    .and_then(|builtin| builtin.name.as_deref())
                    .is_none()
            );
            assert_eq!(
                workspace.workspace.member_overrides["AppUI"].jobs,
                Some(vec!["assets".to_string(), "l10n".to_string()])
            );
        }
        other => panic!("expected workspace manifest, got {other:?}"),
    }
}

#[test]
fn rejects_legacy_workspace_default_builtin_shape_with_migration_hint() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
builtin = "l10n"
"#,
    )
    .expect_err("legacy workspace default builtin shape should fail");

    let message = error.to_string();
    assert!(message.contains("legacy flat built-in template syntax is no longer supported"));
    assert!(message.contains("[workspace.defaults.jobs.l10n.template.builtin] language = \"...\""));
}

#[test]
fn rejects_manifest_that_mixes_jobs_and_workspace() {
    let error = parse_manifest_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[workspace]
members = ["AppUI"]
"#,
    )
    .expect_err("mixed manifest should be rejected");

    assert!(
        error
            .to_string()
            .contains("must not define both `jobs` and `workspace`")
    );
}

#[test]
fn rejects_workspace_members_that_look_like_config_paths() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI/numi.toml"]
"#,
    )
    .expect_err("workspace members that look like config paths should be rejected");

    assert!(
        error
            .to_string()
            .contains("workspace.members entries must be relative member roots")
    );
}

#[test]
fn accepts_workspace_members_whose_names_end_with_toml() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["App.toml"]
"#,
    )
    .expect("non-config .toml member root should parse");

    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    assert_eq!(workspace.workspace.members, vec!["App.toml"]);
}

#[test]
fn rejects_workspace_members_that_normalize_to_the_same_root() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["App", "App/"]
"#,
    )
    .expect_err("equivalent workspace member roots should be rejected");

    assert!(
        error
            .to_string()
            .contains("workspace.members entries must be unique")
    );
}

#[test]
fn parses_workspace_defaults_job_template_shape() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.l10n.template]
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"
"#,
    )
    .expect("workspace defaults template should parse");

    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    assert_eq!(
        workspace.workspace.defaults.jobs["l10n"]
            .template
            .builtin
            .as_ref()
            .and_then(|builtin| builtin.language.as_deref()),
        Some("swift")
    );
    assert!(
        workspace.workspace.defaults.jobs["l10n"]
            .template
            .builtin
            .as_ref()
            .and_then(|builtin| builtin.name.as_deref())
            .is_none()
    );
}

#[test]
fn parses_workspace_defaults_job_hooks_shape() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.hooks.post_generate]
command = ["swiftformat"]
"#,
    )
    .expect("workspace defaults hooks should parse");

    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    assert_eq!(
        workspace.workspace.defaults.jobs["assets"]
            .hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.clone()),
        Some(vec!["swiftformat".to_string()])
    );
}

#[test]
fn rejects_workspace_member_overrides_for_undeclared_members() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.member_overrides.Core]
jobs = ["assets"]
"#,
    )
    .expect_err("undeclared member override should fail validation");

    assert!(
        error
            .to_string()
            .contains("workspace.member_overrides keys must match declared members")
    );
}

#[test]
fn rejects_normalized_duplicate_workspace_member_overrides() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["App"]

[workspace.member_overrides.App]
jobs = ["assets"]

[workspace.member_overrides."App/"]
jobs = ["l10n"]
"#,
    )
    .expect_err("normalized duplicate override keys should fail validation");

    assert!(
        error
            .to_string()
            .contains("workspace.member_overrides keys must be unique")
    );
}

#[test]
fn rejects_invalid_workspace_default_job_template_shape() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n.stencil"
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect_err("invalid workspace default template should fail validation");

    assert!(
        error
            .to_string()
            .contains("workspace default job template builtin must not set `name`")
    );
}

#[test]
fn rejects_mixed_workspace_default_path_and_builtin_language() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
    )
    .expect_err("mixed workspace default template sources should fail");

    let message = error.to_string();
    assert!(message.contains("workspace default job template must set exactly one source"));
    assert!(message.contains("remove either `path` or `builtin.language`"));
}

#[test]
fn serializing_workspace_member_without_jobs_omits_jobs_field() {
    let workspace = crate::WorkspaceConfig {
        version: 1,
        workspace: crate::WorkspaceSettings {
            members: vec!["App".to_string(), "Core".to_string()],
            defaults: crate::WorkspaceDefaults::default(),
            member_overrides: std::collections::BTreeMap::from([
                ("App".to_string(), crate::WorkspaceMemberOverride { jobs: None }),
                (
                    "Core".to_string(),
                    crate::WorkspaceMemberOverride {
                        jobs: Some(vec!["assets".to_string()]),
                    },
                ),
            ]),
        },
    };

    let serialized = toml::to_string(&workspace).expect("workspace should serialize");

    assert!(!serialized.contains("jobs = []"));
    assert!(serialized.contains("[workspace.member_overrides.Core]"));
    assert!(serialized.contains("jobs = [\"assets\"]"));

    let reparsed = toml::from_str::<crate::WorkspaceConfig>(&serialized)
        .expect("serialized workspace should parse back");
    assert_eq!(reparsed, workspace);
}

#[test]
fn rejects_workspace_default_builtin_name() {
    let error = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
    )
    .expect_err("workspace default builtin name should fail validation");

    assert!(
        error
            .to_string()
            .contains("workspace default job template builtin must not set `name`")
    );
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
fn discovers_workspace_manifest_in_ancestors_only() {
    let ancestor_root = create_temp_dir("workspace-discovery-ancestor");
    let ancestor_manifest = ancestor_root.join("numi.toml");
    write_file(&ancestor_manifest, "version = 1\n[workspace]\nmembers = [\"App\"]\n");

    let nested = ancestor_root.join("apps/ios/App");
    fs::create_dir_all(&nested).expect("nested directory should exist");

    let discovered = crate::discover_workspace_ancestor(&nested, None)
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

    let error = crate::discover_workspace_ancestor(&descendant_root, None)
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
fn rejects_unified_manifest_missing_jobs_or_workspace() {
    let error = parse_manifest_str(
        r#"
version = 1
"#,
    )
    .expect_err("manifest without jobs or workspace should fail");

    assert!(error.to_string().contains("manifest must define either `jobs` or `workspace`"));
}
