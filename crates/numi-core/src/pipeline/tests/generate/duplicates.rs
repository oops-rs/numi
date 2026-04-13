use super::super::{make_temp_dir, make_test_font_bytes};
use super::super::super::generate;
use std::fs;

#[test]
fn generate_rejects_duplicate_strings_table_names_from_directory_inputs() {
let temp_dir = make_temp_dir("duplicate-strings-table");
let config_path = temp_dir.join("numi.toml");
let localization_root = temp_dir.join("Resources/Localization");
let en_dir = localization_root.join("en.lproj");
let fr_dir = localization_root.join("fr.lproj");
fs::create_dir_all(&en_dir).expect("en dir should exist");
fs::create_dir_all(&fr_dir).expect("fr dir should exist");
fs::write(
    en_dir.join("Localizable.strings"),
    "\"profile.title\" = \"Profile\";\n",
)
.expect("en strings should be written");
fs::write(
    fr_dir.join("Localizable.strings"),
    "\"profile.title\" = \"Profil\";\n",
)
.expect("fr strings should be written");
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

let error = generate(&config_path, None).expect_err("duplicate tables should fail");
let message = error.to_string();

assert!(message.contains("duplicate localization table `Localizable`"));
assert!(message.contains("en.lproj/Localizable.strings"));
assert!(message.contains("fr.lproj/Localizable.strings"));

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_rejects_duplicate_files_module_names_from_distinct_inputs() {
let temp_dir = make_temp_dir("duplicate-files-module");
let config_path = temp_dir.join("numi.toml");
let first_root = temp_dir.join("Resources/A/Fixtures");
let second_root = temp_dir.join("Resources/B/Fixtures");
fs::create_dir_all(&first_root).expect("first files directory should exist");
fs::create_dir_all(&second_root).expect("second files directory should exist");
fs::write(first_root.join("faq.pdf"), "faq").expect("first file should be written");
fs::write(second_root.join("faq.pdf"), "faq").expect("second file should be written");
fs::write(
    &config_path,
    r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/A/Fixtures"

[[jobs.files.inputs]]
type = "files"
path = "Resources/B/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
)
.expect("config should be written");

let error = generate(&config_path, None).expect_err("duplicate modules should fail");
let message = error.to_string();

assert!(message.contains("duplicate files module `Fixtures`"));
assert!(message.contains("Resources/A/Fixtures"));
assert!(message.contains("Resources/B/Fixtures"));

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_rejects_duplicate_fonts_module_names_from_distinct_inputs() {
let temp_dir = make_temp_dir("duplicate-fonts-module");
let config_path = temp_dir.join("numi.toml");
let first_root = temp_dir.join("Resources/A/Fonts");
let second_root = temp_dir.join("Resources/B/Fonts");
fs::create_dir_all(&first_root).expect("first fonts directory should exist");
fs::create_dir_all(&second_root).expect("second fonts directory should exist");
fs::write(
    first_root.join("Baloo2-Bold.ttf"),
    make_test_font_bytes("Baloo 2", "Bold", "Baloo2-Bold"),
)
.expect("first font should be written");
fs::write(
    second_root.join("Baloo2-Regular.ttf"),
    make_test_font_bytes("Baloo 2", "Regular", "Baloo2-Regular"),
)
.expect("second font should be written");
fs::write(
    &config_path,
    r#"
version = 1

[jobs.fonts]
output = "Generated/Fonts.swift"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/A/Fonts"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/B/Fonts"

[jobs.fonts.template]
path = "Templates/fonts.jinja"
"#,
)
.expect("config should be written");
fs::create_dir_all(temp_dir.join("Templates")).expect("templates dir should exist");
fs::write(
    temp_dir.join("Templates/fonts.jinja"),
    "{{ modules | length }}\n",
)
.expect("template should be written");

let error = generate(&config_path, None).expect_err("duplicate modules should fail");
let message = error.to_string();

assert!(message.contains("duplicate fonts module `Fonts`"));
assert!(message.contains("Resources/A/Fonts"));
assert!(message.contains("Resources/B/Fonts"));

fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
