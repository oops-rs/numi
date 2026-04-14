use crate::{
    Config, Manifest, parse_manifest_str, parse_str, resolve_workspace_member_config,
    workspace_member_config_path,
};
use std::{path::Path, path::PathBuf};

fn workspace_fixture(toml: &str) -> crate::WorkspaceConfig {
    let manifest = parse_manifest_str(toml).expect("workspace should parse");
    let Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };
    workspace
}

fn member_fixture(toml: &str) -> Config {
    toml::from_str::<Config>(toml).expect("member config should deserialize")
}

#[test]
fn workspace_member_config_path_joins_member_root_with_numi_toml() {
    assert_eq!(
        workspace_member_config_path(Path::new("/tmp/workspace"), "AppUI"),
        PathBuf::from("/tmp/workspace/AppUI/numi.toml")
    );
}

#[test]
fn workspace_defaults_can_supply_builtin_language() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
name = "assets"
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    let builtin = resolved.jobs[0]
        .template
        .builtin
        .as_ref()
        .expect("builtin should exist");
    assert_eq!(builtin.language.as_deref(), Some("objc"));
    assert_eq!(builtin.name.as_deref(), Some("assets"));
}

#[test]
fn workspace_defaults_can_disable_auto_lookup() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.strings.template]
auto_lookup = false
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.strings]
output = "Generated/Strings.swift"

[[jobs.strings.inputs]]
type = "strings"
path = "Resources/Localization"
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(resolved.jobs[0].template.auto_lookup, Some(false));
    assert!(resolved.jobs[0].template.path.is_none());
    assert!(resolved.jobs[0].template.builtin.is_none());
}

#[test]
fn member_auto_lookup_flag_blocks_workspace_template_path_inheritance() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.strings.template]
path = "Templates/strings"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.strings]
output = "Generated/Strings.swift"

[[jobs.strings.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.strings.template]
auto_lookup = false
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(resolved.jobs[0].template.auto_lookup, Some(false));
    assert!(resolved.jobs[0].template.path.is_none());
}

#[test]
fn workspace_defaults_hooks_inherit_when_job_hooks_are_missing() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.hooks.post_generate]
command = ["swiftformat"]
"#,
    );

    let member_config = member_fixture(
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
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(
        resolved.jobs[0]
            .hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.clone()),
        Some(vec!["swiftformat".to_string()])
    );
}

#[test]
fn workspace_defaults_shell_hooks_inherit_when_job_hooks_are_missing() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.hooks.post_generate]
shell = "swift format -i \"$NUMI_HOOK_OUTPUT_PATH\""
"#,
    );

    let member_config = member_fixture(
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
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    let hook = resolved.jobs[0]
        .hooks
        .post_generate
        .as_ref()
        .expect("post hook should be inherited");
    assert!(hook.command.is_empty());
    assert_eq!(
        hook.shell.as_deref(),
        Some("swift format -i \"$NUMI_HOOK_OUTPUT_PATH\"")
    );
}

#[test]
fn workspace_global_hooks_inherit_when_job_hooks_are_missing() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.hooks.post_generate]
command = ["swiftformat"]
"#,
    );

    let member_config = member_fixture(
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
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(
        resolved.jobs[0]
            .hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.clone()),
        Some(vec!["swiftformat".to_string()])
    );
}

#[test]
fn workspace_job_hooks_override_workspace_global_hooks() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.hooks.post_generate]
command = ["swiftformat"]

[workspace.defaults.jobs.assets.hooks.post_generate]
command = ["swiftlint", "format"]
"#,
    );

    let member_config = member_fixture(
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
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(
        resolved.jobs[0]
            .hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.clone()),
        Some(vec!["swiftlint".to_string(), "format".to_string()])
    );
}

#[test]
fn job_level_hooks_replace_workspace_default_hooks_for_same_phase() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.hooks.post_generate]
command = ["swiftformat"]
"#,
    );

    let member_config = member_fixture(
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

[jobs.assets.hooks.post_generate]
command = ["swiftlint", "format"]
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    assert_eq!(
        resolved.jobs[0]
            .hooks
            .post_generate
            .as_ref()
            .map(|hook| hook.command.clone()),
        Some(vec!["swiftlint".to_string(), "format".to_string()])
    );
}

#[test]
fn workspace_defaults_path_inherit_for_empty_member_template() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");
    let expected_path = PathBuf::from("..")
        .join("Templates")
        .join("assets.stencil")
        .display()
        .to_string();

    assert_eq!(
        resolved.jobs[0].template.path.as_deref(),
        Some(expected_path.as_str())
    );
    assert!(resolved.jobs[0].template.builtin.is_none());
}

#[test]
fn workspace_defaults_path_inherit_handles_nested_member_roots_with_native_separators() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["apps/AppUI"]

[workspace.defaults.jobs.assets.template]
path = "Templates/assets.stencil"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "apps/AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");
    let expected_path = PathBuf::from("..")
        .join("..")
        .join("Templates")
        .join("assets.stencil")
        .display()
        .to_string();

    assert_eq!(
        resolved.jobs[0].template.path.as_deref(),
        Some(expected_path.as_str())
    );
}

#[test]
fn workspace_defaults_missing_member_builtin_name_remains_invalid() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
"#,
    );

    let error = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect_err("missing builtin name should remain invalid after resolution");

    let message = error
        .into_iter()
        .map(|diagnostic| diagnostic.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(message.contains("job template builtin must set both language and name"));
    assert!(message.contains("job template must set exactly one source"));
}

#[test]
fn job_level_builtin_language_overrides_workspace_default_language() {
    let workspace = workspace_fixture(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
    );

    let member_config = member_fixture(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    );

    let resolved = resolve_workspace_member_config(
        Path::new("/tmp/workspace"),
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace defaults should resolve");

    let builtin = resolved.jobs[0]
        .template
        .builtin
        .as_ref()
        .expect("builtin should exist");
    assert_eq!(builtin.language.as_deref(), Some("swift"));
    assert_eq!(builtin.name.as_deref(), Some("swiftui-assets"));
}

#[test]
fn resolve_config_materializes_v1_default_values() {
    let config = parse_str(
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
    .expect("config should parse");

    let resolved = crate::resolve_config(&config);

    assert_eq!(resolved.defaults.access_level.as_deref(), Some("internal"));
    assert_eq!(resolved.defaults.bundle.mode.as_deref(), Some("module"));
    assert_eq!(resolved.defaults.incremental, Some(true));
    assert!(resolved.jobs[0].bundle.is_empty());
}
