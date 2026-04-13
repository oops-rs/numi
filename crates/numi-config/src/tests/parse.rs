use crate::{
    BuiltinTemplateConfig, BundleConfig, Config, DefaultsConfig, HooksConfig, InputConfig,
    JobConfig, Manifest, ManifestKindSniff, TemplateConfig, parse_manifest_str, parse_str,
    sniff_manifest_kind_str,
};
#[test]
fn parses_defaults_and_jobs_from_toml() {
    let config = parse_str(
        r#"
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/l10n.stencil"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.version, 1);
    assert_eq!(config.defaults.access_level.as_deref(), Some("internal"));
    assert_eq!(config.defaults.bundle.mode.as_deref(), Some("module"));
    assert_eq!(config.jobs.len(), 2);
    assert_eq!(config.jobs[0].name, "assets");
    assert_eq!(config.jobs[0].inputs.len(), 1);
    assert_eq!(
        config.jobs[0]
            .template
            .builtin
            .as_ref()
            .and_then(|builtin| builtin.name.as_deref()),
        Some("swiftui-assets")
    );
    assert_eq!(
        config.jobs[1].template.path.as_deref(),
        Some("Templates/l10n.stencil")
    );
}

#[test]
fn parses_namespaced_builtin_template_config() {
    let config = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.jobs[0]
            .template
            .builtin
            .as_ref()
            .and_then(|builtin| builtin.name.as_deref()),
        Some("swiftui-assets")
    );
}

#[test]
fn parses_builtin_template_language_and_name() {
    let config = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
    )
    .expect("config should parse");

    let builtin = config.jobs[0]
        .template
        .builtin
        .as_ref()
        .expect("builtin should exist");
    assert_eq!(builtin.language.as_deref(), Some("objc"));
    assert_eq!(builtin.name.as_deref(), Some("assets"));
}

#[test]
fn parses_incremental_generation_settings_from_defaults_and_job() {
    let config = parse_str(
        r#"
version = 1

[defaults]
incremental = false

[jobs.assets]
output = "Generated/Assets.swift"
incremental = true

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.defaults.incremental, Some(false));
    assert_eq!(config.jobs[0].incremental, Some(true));
}

#[test]
fn rejects_empty_job_hook_command() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[jobs.assets.hooks.post_generate]
command = []
"#,
    )
    .expect_err("empty hook commands should fail validation");

    assert!(
        error
            .to_string()
            .contains("job hook command must not be empty")
    );
}

#[test]
fn rejects_builtin_template_language_without_name() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
"#,
    )
    .expect_err("partial builtin language should fail validation");

    let message = error.to_string();
    assert!(message.contains("job template builtin must set both language and name"));
    assert!(
        message
            .contains("set `[jobs.assets.template.builtin] language = \"...\" name = \"...\"`")
    );
}

#[test]
fn rejects_builtin_template_name_without_language() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
name = "swiftui-assets"
"#,
    )
    .expect_err("partial builtin name should fail validation");

    let message = error.to_string();
    assert!(message.contains("job template builtin must set both language and name"));
    assert!(
        message
            .contains("set `[jobs.assets.template.builtin] language = \"...\" name = \"...\"`")
    );
}

#[test]
fn rejects_unknown_builtin_language() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "kotlin"
name = "assets"
"#,
    )
    .expect_err("unknown builtin language should fail");

    let message = error.to_string();
    assert!(message.contains("jobs.assets.template.builtin.language must be one of"));
}

#[test]
fn rejects_legacy_swift_builtin_namespace_shape() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"
"#,
    )
    .expect_err("legacy builtin namespace shape should fail");

    let message = error.to_string();
    assert!(message.contains("unknown field `swift`"));
}

#[test]
fn rejects_template_configs_that_set_both_builtin_and_path() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
path = "Templates/assets.stencil"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect_err("config with both template sources should fail validation");

    let message = error.to_string();
    assert!(message.contains("job template must set exactly one source"));
    assert!(message.contains("set either `[jobs.assets.template.builtin] language = \"...\" name = \"...\"` or `[jobs.assets.template] path = \"...\"`"));
}

#[test]
fn rejects_empty_builtin_template_namespace() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
"#,
    )
    .expect_err("empty built-in namespace should fail validation");

    let message = error.to_string();
    assert!(message.contains("job template must set exactly one source"));
    assert!(message.contains("set either `[jobs.assets.template.builtin] language = \"...\" name = \"...\"` or `[jobs.assets.template] path = \"...\"`"));
}

#[test]
fn rejects_legacy_jobs_array_syntax_with_migration_hint() {
    let error = parse_str(
        r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect_err("legacy jobs array syntax should fail with a migration diagnostic");

    let message = error.to_string();
    assert!(message.contains("legacy `[[jobs]]` syntax is no longer supported"));
    assert!(message.contains("[jobs.assets]"));
    assert!(message.contains("[[jobs.assets.inputs]]"));
}

#[test]
fn rejects_legacy_flat_builtin_template_shape_with_migration_hint() {
    let error = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
builtin = "swiftui-assets"
"#,
    )
    .expect_err("legacy flat builtin syntax should fail with a migration diagnostic");

    let message = error.to_string();
    assert!(message.contains("legacy flat built-in template syntax is no longer supported"));
    assert!(message.contains("[jobs.assets.template.builtin] language = \"...\" name = \"...\""));
    assert!(!message.contains("invalid type: string"));
}

#[test]
fn accepts_path_template_with_empty_builtin_table() {
    let config = parse_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
path = "Templates/assets.jinja"

[jobs.assets.template.builtin]
"#,
    )
    .expect("path template should remain valid when an empty builtin table is present");

    assert_eq!(
        config.jobs[0].template.path.as_deref(),
        Some("Templates/assets.jinja")
    );
    assert!(
        config.jobs[0]
            .template
            .builtin
            .as_ref()
            .is_some_and(|builtin| builtin.is_empty())
    );
}

#[test]
fn serializing_empty_builtin_namespace_omits_builtin_table() {
    let config = Config {
        version: 1,
        defaults: DefaultsConfig::default(),
        jobs: vec![JobConfig {
            name: "assets".to_string(),
            output: "Generated/Assets.swift".to_string(),
            access_level: None,
            incremental: None,
            bundle: BundleConfig::default(),
            inputs: vec![InputConfig {
                kind: "xcassets".to_string(),
                path: "Resources/Assets.xcassets".to_string(),
            }],
            template: TemplateConfig {
                builtin: Some(BuiltinTemplateConfig {
                    language: None,
                    name: None,
                }),
                path: None,
            },
            hooks: HooksConfig::default(),
        }],
    };

    let serialized = toml::to_string(&config).expect("config should serialize");

    assert!(!serialized.contains("[jobs.assets.template]"));
    assert!(!serialized.contains("[jobs.assets.template.builtin]"));
    assert!(!serialized.contains("swift ="));
}

#[test]
fn rejects_invalid_v1_enum_values() {
    let error = parse_str(
        r#"
version = 1

[defaults]
access_level = "private"

[defaults.bundle]
mode = "feature"

[jobs.assets]
output = "Generated/Assets.swift"
access_level = "open"

[[jobs.assets.inputs]]
type = "images"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect_err("invalid v1 enum values should fail validation");

    let message = error.to_string();
    assert!(message.contains("defaults.access_level"));
    assert!(message.contains("defaults.bundle.mode"));
    assert!(message.contains("[job: assets]"));
    assert!(message.contains("jobs.inputs[].type"));
}

#[test]
fn accepts_files_as_valid_input_kind() {
    let config = parse_str(
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources"

[jobs.files.template]
path = "Templates/files.stencil"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.jobs.len(), 1);
    assert_eq!(config.jobs[0].inputs[0].kind, "files");
}

#[test]
fn accepts_fonts_as_valid_input_kind() {
    let config = parse_str(
        r#"
version = 1

[jobs.fonts]
output = "Generated/Fonts.swift"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/Fonts"

[jobs.fonts.template]
path = "Templates/fonts.jinja"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.jobs.len(), 1);
    assert_eq!(config.jobs[0].inputs[0].kind, "fonts");
}

#[test]
fn rejects_unknown_keys_during_parsing() {
    let error = parse_str(
        r#"
version = 1
verison = 2

[defaults]
access_level = "internal"
accessLevel = "public"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
pth = "Resources/Typo.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect_err("unknown keys should fail validation");

    let message = error.to_string();
    assert!(message.contains("unknown field"));
}

#[test]
fn parses_unified_single_config_manifest() {
    let manifest = parse_manifest_str(
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
"#,
    )
    .expect("single-config manifest should parse");

    match manifest {
        Manifest::Config(config) => {
            assert_eq!(config.version, 1);
            assert_eq!(config.jobs.len(), 1);
            assert_eq!(config.jobs[0].name, "assets");
        }
        other => panic!("expected config manifest, got {other:?}"),
    }
}

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
