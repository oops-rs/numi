use super::super::super::{WriteOutcome, compute_generation_fingerprint, generate};
use super::super::make_temp_dir;
use std::fs;

fn write_extensionless_l10n_job_config(config_path: &std::path::Path) {
    std::fs::write(
        config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/l10n"
"#,
    )
    .expect("config should be written");
}

fn write_implicit_l10n_job_config(config_path: &std::path::Path) {
    std::fs::write(
        config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"
"#,
    )
    .expect("config should be written");
}

fn write_l10n_job_config_with_auto_lookup(config_path: &std::path::Path, auto_lookup: bool) {
    std::fs::write(
        config_path,
        format!(
            r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
auto_lookup = {auto_lookup}
"#
        ),
    )
    .expect("config should be written");
}

#[test]
fn generate_accepts_strings_with_escaped_apostrophes_via_langcodec() {
    let temp_dir = make_temp_dir("pipeline-strings-apostrophe");
    let config_path = temp_dir.join("swiftgen.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"invite.accept\" = \"Can\\'t accept the invitation\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let generated_path = temp_dir.join("Generated/L10n.swift");
    let generated = fs::read_to_string(&generated_path).expect("generated output should exist");

    assert!(report.warnings.is_empty());
    assert_eq!(
        generated,
        r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let inviteAccept = tr("Localizable", "invite.accept")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_renders_custom_template_includes_from_config_root() {
    let temp_dir = make_temp_dir("custom-template-shared-include");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization");
    let templates_dir = temp_dir.join("Templates");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(localization_root.join("en.lproj")).expect("localization dir should exist");
    fs::create_dir_all(&templates_dir).expect("templates dir should exist");
    fs::create_dir_all(temp_dir.join("partials")).expect("shared partial dir should exist");

    fs::write(
        localization_root.join("en.lproj/Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    fs::write(temp_dir.join("partials/header.jinja"), "SHARED")
        .expect("shared include should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/main.jinja"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "SHARED|L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_resolves_extensionless_template_path_to_jinja_file() {
    let temp_dir = make_temp_dir("pipeline-extensionless-template-path");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let template_path = temp_dir.join("Templates/l10n.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        &template_path,
        "{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    write_extensionless_l10n_job_config(&config_path);

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_auto_discovers_template_from_templates_directory() {
    let temp_dir = make_temp_dir("pipeline-auto-template-templates");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let template_path = temp_dir.join("Templates/l10n.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        &template_path,
        "AUTO|{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    write_implicit_l10n_job_config(&config_path);

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "AUTO|L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_auto_discovers_template_from_lowercase_templates_directory() {
    let temp_dir = make_temp_dir("pipeline-auto-template-lowercase");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let template_path = temp_dir.join("templates/l10n.template.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        &template_path,
        "LOWER|{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    write_implicit_l10n_job_config(&config_path);

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "LOWER|L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_rejects_ambiguous_implicit_template_matches() {
    let temp_dir = make_temp_dir("pipeline-auto-template-ambiguous");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let upper_template_path = temp_dir.join("Templates/l10n.jinja");
    let lower_template_path = temp_dir.join("templates/l10n.template.jinja");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        upper_template_path
            .parent()
            .expect("upper template path should have parent"),
    )
    .expect("upper template dir should exist");
    fs::create_dir_all(
        lower_template_path
            .parent()
            .expect("lower template path should have parent"),
    )
    .expect("lower template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(&upper_template_path, "UPPER\n").expect("upper template should be written");
    fs::write(&lower_template_path, "LOWER\n").expect("lower template should be written");
    write_implicit_l10n_job_config(&config_path);

    let error = generate(&config_path, None).expect_err("ambiguous implicit templates should fail");
    let message = error.to_string();

    assert!(message.contains("ambiguous implicit template lookup for job `l10n`"));
    assert!(message.contains("Templates/l10n.jinja"));
    assert!(message.contains("l10n.template.jinja"));
    assert!(message.contains("template.path"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_prefers_explicit_template_path_over_implicit_lookup() {
    let temp_dir = make_temp_dir("pipeline-auto-template-explicit-path-wins");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let implicit_template_path = temp_dir.join("Templates/l10n.jinja");
    let explicit_template_path = temp_dir.join("Templates/main.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        implicit_template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(&implicit_template_path, "IMPLICIT\n").expect("implicit template should be written");
    fs::write(&explicit_template_path, "EXPLICIT\n").expect("explicit template should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/main.jinja"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "EXPLICIT\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_prefers_builtin_template_over_implicit_lookup() {
    let temp_dir = make_temp_dir("pipeline-auto-template-builtin-wins");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let implicit_template_path = temp_dir.join("Templates/l10n.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        implicit_template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(&implicit_template_path, "IMPLICIT\n").expect("implicit template should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(rendered.contains("internal enum L10n"));
    assert!(rendered.contains("private func tr(_ table: String, _ key: String) -> String"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_respects_auto_lookup_false_without_explicit_template() {
    let temp_dir = make_temp_dir("pipeline-auto-template-disabled");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let template_path = temp_dir.join("Templates/l10n.jinja");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(&template_path, "AUTO\n").expect("template should be written");
    write_l10n_job_config_with_auto_lookup(&config_path, false);

    let error = generate(&config_path, None).expect_err("disabled auto lookup should fail");
    let message = error.to_string();

    assert!(message.contains("implicit template lookup is disabled"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_writes_builtin_files_accessors() {
    let temp_dir = make_temp_dir("pipeline-files-generate");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let generated_path = temp_dir.join("Generated/Files.swift");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(
        rendered,
        r#"import Foundation

internal enum Files {
    internal enum Onboarding {
        internal static let welcomeVideoMp4 = file("Onboarding/welcome-video.mp4")
    }
    internal static let faqPdf = file("faq.pdf")
}

private func resourceBundle() -> Bundle {
    Bundle.module
}

private func file(_ path: String) -> URL {
    guard let url = resourceBundle().url(forResource: path, withExtension: nil) else {
        fatalError("Missing file resource: \(path)")
    }
    return url
}
"#
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_skips_hidden_only_files_inputs() {
    let temp_dir = make_temp_dir("pipeline-files-hidden-only-input");
    let config_path = temp_dir.join("numi.toml");
    let empty_root = temp_dir.join("Resources/Empty");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");

    fs::create_dir_all(empty_root.join(".Snapshots")).expect("hidden-only directory should exist");
    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(empty_root.join(".DS_Store"), "ignored").expect("dotfile should be written");
    fs::write(empty_root.join(".Snapshots/preview.txt"), "hidden")
        .expect("hidden file should be written");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(
        &template_path,
        "{{ modules | length }}|{{ modules[0].name }}|{{ modules[0].entries[0].properties.fileName }}\n",
    )
    .expect("template should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Empty"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
path = "Templates/files.jinja"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "1|Fixtures|faq.pdf\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_writes_objc_builtin_files_accessors() {
    let temp_dir = make_temp_dir("pipeline-objc-files-generate");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let generated_path = temp_dir.join("Generated/Files.h");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.h"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(!rendered.contains("@implementation"));
    assert!(
        rendered.contains("NS_INLINE NSURL *Files__Fixtures__Onboarding__WelcomeVideoMp4(void)")
    );
    assert!(rendered.contains("SWIFTPM_MODULE_BUNDLE"));
    assert!(!rendered.contains("bundleForClass:"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generation_fingerprint_changes_when_builtin_language_changes() {
    let temp_dir = make_temp_dir("pipeline-fingerprint-builtin-language");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");

    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    )
    .expect("config should be written");

    let loaded = numi_config::load_from_path(&config_path).expect("config should load");
    let config_dir = config_path.parent().expect("config should have parent");
    let selected_jobs = vec!["files".to_string()];
    let swift_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
        .expect("files job should resolve");
    let swift_job = swift_jobs
        .into_iter()
        .next()
        .expect("files job should exist");
    let swift_fingerprint =
        compute_generation_fingerprint(config_dir, config_dir, &loaded.config.defaults, swift_job)
            .expect("swift builtin fingerprint should compute");

    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
    )
    .expect("objc config should be written");

    let loaded = numi_config::load_from_path(&config_path).expect("objc config should load");
    let objc_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
        .expect("files job should resolve");
    let objc_job = objc_jobs
        .into_iter()
        .next()
        .expect("files job should exist");
    let objc_fingerprint =
        compute_generation_fingerprint(config_dir, config_dir, &loaded.config.defaults, objc_job)
            .expect("objc builtin fingerprint should compute");

    assert_ne!(swift_fingerprint.fingerprint, objc_fingerprint.fingerprint);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
