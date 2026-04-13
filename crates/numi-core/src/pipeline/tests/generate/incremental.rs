use super::super::{make_temp_dir, write_custom_files_job_config};
use super::super::super::{generate, generate_with_options, GenerateOptions, WriteOutcome};
use std::fs;

#[test]
fn generate_skips_when_generation_contract_is_unchanged_by_default() {
let temp_dir = make_temp_dir("pipeline-generate-skip-default");
let config_path = temp_dir.join("numi.toml");
let files_root = temp_dir.join("Resources/Fixtures");
let template_path = temp_dir.join("Templates/files.jinja");
let generated_path = temp_dir.join("Generated/Files.swift");

fs::create_dir_all(&files_root).expect("files directory should exist");
fs::create_dir_all(
    template_path
        .parent()
        .expect("template path should have parent"),
)
.expect("template dir should exist");
fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
fs::write(
    &template_path,
    "{{ modules[0].entries[0].properties.fileName }}\n",
)
.expect("template should be written");
write_custom_files_job_config(&config_path, None);

let first = generate(&config_path, None).expect("initial generation should succeed");
assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);
assert_eq!(
    fs::read_to_string(&generated_path).expect("generated file should exist"),
    "faq.pdf\n"
);

let second = generate(&config_path, None).expect("second generation should succeed");
assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
assert_eq!(
    fs::read_to_string(&generated_path).expect("generated file should remain"),
    "faq.pdf\n"
);

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_respects_job_incremental_opt_out_and_rerenders() {
let temp_dir = make_temp_dir("pipeline-generate-opt-out");
let config_path = temp_dir.join("numi.toml");
let files_root = temp_dir.join("Resources/Fixtures");
let template_path = temp_dir.join("Templates/files.jinja");
let generated_path = temp_dir.join("Generated/Files.swift");

fs::create_dir_all(&files_root).expect("files directory should exist");
fs::create_dir_all(
    template_path
        .parent()
        .expect("template path should have parent"),
)
.expect("template dir should exist");
fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
fs::write(
    &template_path,
    "{{ modules[0].entries[0].properties.fileName }}\n",
)
.expect("template should be written");
write_custom_files_job_config(&config_path, Some(false));

let first = generate(&config_path, None).expect("initial generation should succeed");
assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

let second = generate(&config_path, None).expect("second generation should rerender");
assert_eq!(second.jobs[0].outcome, WriteOutcome::Unchanged);
assert_eq!(
    fs::read_to_string(&generated_path).expect("generated file should remain"),
    "faq.pdf\n"
);

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_options_override_job_incremental_setting() {
let temp_dir = make_temp_dir("pipeline-generate-options-override");
let config_path = temp_dir.join("numi.toml");
let files_root = temp_dir.join("Resources/Fixtures");
let template_path = temp_dir.join("Templates/files.jinja");
let generated_path = temp_dir.join("Generated/Files.swift");

fs::create_dir_all(&files_root).expect("files directory should exist");
fs::create_dir_all(
    template_path
        .parent()
        .expect("template path should have parent"),
)
.expect("template dir should exist");
fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
fs::write(
    &template_path,
    "{{ modules[0].entries[0].properties.fileName }}\n",
)
.expect("template should be written");
write_custom_files_job_config(&config_path, Some(false));

let first = generate(&config_path, None).expect("initial generation should succeed");
assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

let second = generate_with_options(
    &config_path,
    None,
    GenerateOptions {
        incremental: Some(true),
        parse_cache: None,
        force_regenerate: false,
        workspace_manifest_path: None,
    },
)
.expect("second generation should honor the explicit override");
assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
assert_eq!(
    fs::read_to_string(&generated_path).expect("generated file should remain"),
    "faq.pdf\n"
);

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_refresh_bypasses_generation_skip() {
let temp_dir = make_temp_dir("pipeline-generate-refresh");
let config_path = temp_dir.join("numi.toml");
let files_root = temp_dir.join("Resources/Fixtures");
let template_path = temp_dir.join("Templates/files.jinja");
let generated_path = temp_dir.join("Generated/Files.swift");

fs::create_dir_all(&files_root).expect("files directory should exist");
fs::create_dir_all(
    template_path
        .parent()
        .expect("template path should have parent"),
)
.expect("template dir should exist");
fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
fs::write(
    &template_path,
    "{{ modules[0].entries[0].properties.fileName }}\n",
)
.expect("template should be written");
write_custom_files_job_config(&config_path, Some(true));

let first = generate(&config_path, None).expect("initial generation should succeed");
assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

let second = generate_with_options(
    &config_path,
    None,
    GenerateOptions {
        incremental: Some(true),
        parse_cache: None,
        force_regenerate: false,
        workspace_manifest_path: None,
    },
)
.expect("second generation should skip");
assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);

let third = generate_with_options(
    &config_path,
    None,
    GenerateOptions {
        incremental: Some(true),
        parse_cache: Some(true),
        force_regenerate: true,
        workspace_manifest_path: None,
    },
)
.expect("force regenerate should rerender");
assert_eq!(third.jobs[0].outcome, WriteOutcome::Unchanged);
assert_eq!(
    fs::read_to_string(&generated_path).expect("generated file should remain"),
    "faq.pdf\n"
);
}
