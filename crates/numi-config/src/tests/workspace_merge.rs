use crate::{Manifest, load_workspace_from_path, parse_manifest_str};

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
fn parses_workspace_manifest() {
    let temp_dir = create_temp_dir("parse-workspace-manifest");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(
        &manifest_path,
        r#"
version = 1

[workspace]
members = ["App", "Core"]

[workspace.member_overrides.App]
jobs = ["assets", "l10n"]
"#,
    );

    let loaded = load_workspace_from_path(&manifest_path).expect("workspace manifest should parse");

    assert_eq!(loaded.config.version, 1);
    assert_eq!(loaded.config.workspace.members, vec!["App", "Core"]);
    assert_eq!(
        loaded.config.workspace.member_overrides["App"].jobs,
        Some(vec!["assets".to_string(), "l10n".to_string()])
    );
}
#[test]
fn unified_manifest_entrypoint_accepts_legacy_workspace_shape() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets"]
"#,
    )
    .expect("legacy workspace shape should parse through manifest entrypoint");

    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    assert_eq!(workspace.workspace.members, vec!["App"]);
    assert_eq!(workspace.members()[0].config, "App/numi.toml");
    assert_eq!(workspace.members()[0].jobs, vec!["assets"]);
}
#[test]
fn deserializes_legacy_workspace_manifest_into_workspace_config() {
    let workspace = toml::from_str::<crate::WorkspaceConfig>(
        r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets", "l10n"]

[[members]]
config = "Core/numi.toml"
"#,
    )
    .expect("legacy workspace manifest should deserialize into WorkspaceConfig");

    assert_eq!(workspace.workspace.members, vec!["App", "Core"]);
    assert_eq!(
        workspace
            .members()
            .iter()
            .map(|member| member.config.as_str())
            .collect::<Vec<_>>(),
        vec!["App/numi.toml", "Core/numi.toml"]
    );
}

#[test]
fn parses_legacy_workspace_manifest_for_compatibility() {
    let temp_dir = create_temp_dir("parse-legacy-workspace-manifest");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(
        &manifest_path,
        r#"
version = 1

[[members]]
config = "App/numi.toml"
jobs = ["assets", "l10n"]

[[members]]
config = "Core/numi.toml"
"#,
    );

    let loaded = load_workspace_from_path(&manifest_path).expect("legacy workspace should parse");

    assert_eq!(loaded.config.workspace.members, vec!["App", "Core"]);
    assert_eq!(
        loaded
            .config
            .members()
            .iter()
            .map(|member| member.config.as_str())
            .collect::<Vec<_>>(),
        vec!["App/numi.toml", "Core/numi.toml"]
    );
    assert_eq!(loaded.config.members()[0].jobs, vec!["assets", "l10n"]);
}

#[test]
fn rejects_workspace_root_members_under_unified_manifest_model() {
    for member in [".", "./"] {
        let error = parse_manifest_str(&format!(
            r#"
version = 1

[workspace]
members = ["{member}"]
"#
        ))
        .expect_err("workspace root member should be rejected");

        let message = error.to_string();
        assert!(
            message.contains("workspace.members entries must not point at the workspace root"),
            "message was: {message}"
        );
        assert!(
            message.contains(
                "declare member directories like `AppUI` or `Core`; the workspace root numi.toml carries `[workspace]`, not a member config path"
            ),
            "message was: {message}"
        );
    }
}

#[test]
fn workspace_members_are_derived_from_current_workspace_state() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["App"]
"#,
    )
    .expect("workspace manifest should parse");

    let Manifest::Workspace(mut workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    assert_eq!(workspace.members()[0].jobs, Vec::<String>::new());

    workspace.workspace.member_overrides.insert(
        "App".to_string(),
        crate::WorkspaceMemberOverride {
            jobs: Some(vec!["assets".to_string(), "l10n".to_string()]),
        },
    );

    assert_eq!(workspace.members()[0].jobs, vec!["assets", "l10n"]);
}

#[test]
fn rejects_duplicate_workspace_members() {
    let temp_dir = create_temp_dir("duplicate-workspace-members");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(
        &manifest_path,
        r#"
version = 1

[workspace]
members = ["App", "App"]
"#,
    );

    let error = load_workspace_from_path(&manifest_path)
        .expect_err("duplicate workspace members should fail validation");

    assert!(
        error
            .to_string()
            .contains("workspace.members entries must be unique")
    );
}

#[test]
fn rejects_empty_workspace_members() {
    let temp_dir = create_temp_dir("empty-workspace-members");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(&manifest_path, "version = 1\n[workspace]\n");

    let error = load_workspace_from_path(&manifest_path)
        .expect_err("workspace manifest requires at least one member");

    assert!(
        error
            .to_string()
            .contains("workspace must declare at least one member")
    );
}

#[test]
fn rejects_unsupported_workspace_version() {
    let temp_dir = create_temp_dir("unsupported-workspace-version");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(
        &manifest_path,
        r#"
version = 2

[workspace]
members = ["App"]
"#,
    );

    let error = load_workspace_from_path(&manifest_path)
        .expect_err("workspace manifest should reject unsupported versions");

    let message = error.to_string();
    assert!(message.contains("workspace version must be 1"));
    assert!(message.contains("set `version = 1` in numi.toml"));
    assert!(!message.contains("numi-workspace.toml"));
}

#[test]
fn rejects_empty_and_duplicate_workspace_jobs() {
    let temp_dir = create_temp_dir("invalid-workspace-jobs");
    let manifest_path = temp_dir.join("numi.toml");
    write_file(
        &manifest_path,
        r#"
version = 1

[workspace]
members = ["App", "Core"]

[workspace.member_overrides.App]
jobs = []

[workspace.member_overrides.Core]
jobs = ["assets", "assets"]
"#,
    );

    let error = load_workspace_from_path(&manifest_path)
        .expect_err("workspace jobs should reject empty and duplicate selections");

    let message = error.to_string();
    assert!(message.contains("workspace member override jobs must not be empty"));
    assert!(message.contains("workspace member override jobs must be unique"));
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
fn parses_workspace_defaults_job_hook_shell_shape() {
    let manifest = parse_manifest_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.hooks.post_generate]
shell = "swift format -i \"$NUMI_HOOK_OUTPUT_PATH\""
"#,
    )
    .expect("workspace defaults shell hook should parse");

    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };

    let hook = workspace.workspace.defaults.jobs["assets"]
        .hooks
        .post_generate
        .as_ref()
        .expect("post hook should exist");
    assert!(hook.command.is_empty());
    assert_eq!(
        hook.shell.as_deref(),
        Some("swift format -i \"$NUMI_HOOK_OUTPUT_PATH\"")
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
                (
                    "App".to_string(),
                    crate::WorkspaceMemberOverride { jobs: None },
                ),
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
