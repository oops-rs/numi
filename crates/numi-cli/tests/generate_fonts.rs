use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn make_temp_dir(test_name: &str) -> PathBuf {
    let unique = format!(
        "numi-{test_name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn push_u16(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_be_bytes());
}

fn utf16be(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

fn make_test_font_bytes(family: &str, style: &str, post_script_name: &str) -> Vec<u8> {
    let full_name = if style == "Regular" {
        family.to_string()
    } else {
        format!("{family} {style}")
    };
    let name_records = [
        (1_u16, utf16be(family)),
        (2_u16, utf16be(style)),
        (4_u16, utf16be(&full_name)),
        (6_u16, utf16be(post_script_name)),
    ];

    let string_offset = 6 + (name_records.len() as u16 * 12);
    let mut name_table = Vec::new();
    push_u16(&mut name_table, 0);
    push_u16(&mut name_table, name_records.len() as u16);
    push_u16(&mut name_table, string_offset);

    let mut storage = Vec::new();
    for (name_id, encoded) in &name_records {
        push_u16(&mut name_table, 3);
        push_u16(&mut name_table, 1);
        push_u16(&mut name_table, 0x0409);
        push_u16(&mut name_table, *name_id);
        push_u16(&mut name_table, encoded.len() as u16);
        push_u16(&mut name_table, storage.len() as u16);
        storage.extend_from_slice(encoded);
    }
    name_table.extend_from_slice(&storage);

    let table_offset = 12 + 16;
    let mut font = Vec::new();
    push_u32(&mut font, 0x0001_0000);
    push_u16(&mut font, 1);
    push_u16(&mut font, 16);
    push_u16(&mut font, 0);
    push_u16(&mut font, 0);
    font.extend_from_slice(b"name");
    push_u32(&mut font, 0);
    push_u32(&mut font, table_offset as u32);
    push_u32(&mut font, name_table.len() as u32);
    font.extend_from_slice(&name_table);
    while font.len() % 4 != 0 {
        font.push(0);
    }
    font
}

#[test]
fn generate_renders_custom_template_from_fonts_input() {
    let temp_root = make_temp_dir("generate-fonts");
    let working_root = temp_root.join("fixture");
    let resources_root = working_root.join("Resources").join("Fonts");
    let templates_root = working_root.join("Templates");
    fs::create_dir_all(&resources_root).expect("resources directory should exist");
    fs::create_dir_all(&templates_root).expect("templates directory should exist");

    fs::write(
        resources_root.join("Baloo2-Bold.ttf"),
        make_test_font_bytes("Baloo 2", "Bold", "Baloo2-Bold"),
    )
    .expect("first font should be written");
    fs::write(
        resources_root.join("Baloo2-Regular.ttf"),
        make_test_font_bytes("Baloo 2", "Regular", "Baloo2-Regular"),
    )
    .expect("second font should be written");
    fs::write(
        resources_root.join("Montserrat-Regular.otf"),
        make_test_font_bytes("Montserrat", "Regular", "Montserrat-Regular"),
    )
    .expect("third font should be written");
    fs::write(
        working_root.join("numi.toml"),
        r#"
version = 1

[defaults]
access_level = "public"

[jobs.fonts]
output = "Generated/Fonts.swift"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/Fonts"

[jobs.fonts.template]
path = "Templates/fonts.jinja"
"#,
    )
    .expect("config should be written");
    fs::write(
        templates_root.join("fonts.jinja"),
        r#"{% for family in modules[0].properties.families -%}
{{ family.name }}|{{ family.swiftIdentifier }}
{% for font in family.fonts -%}
{{ font.postScriptName }}|{{ font.styleName }}|{{ font.fileName }}
{% endfor -%}
{% endfor -%}
"#,
    )
    .expect("template should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "fonts"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/Fonts.swift"))
        .expect("generated fonts output should exist");

    assert_eq!(
        generated,
        "\
Baloo 2|Baloo2
Baloo2-Bold|Bold|Baloo2-Bold.ttf
Baloo2-Regular|Regular|Baloo2-Regular.ttf
Montserrat|Montserrat
Montserrat-Regular|Regular|Montserrat-Regular.otf
"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_emits_fonts_module_kind_and_metadata() {
    let temp_root = make_temp_dir("dump-context-fonts");
    let working_root = temp_root.join("fixture");
    let resources_root = working_root.join("Resources").join("Fonts");
    let templates_root = working_root.join("Templates");
    fs::create_dir_all(&resources_root).expect("resources directory should exist");
    fs::create_dir_all(&templates_root).expect("templates directory should exist");

    fs::write(
        resources_root.join("rank.otf"),
        make_test_font_bytes("Lettown Hills", "Italic", "LettownHills-Italic"),
    )
    .expect("font should be written");
    fs::write(
        working_root.join("numi.toml"),
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
    .expect("config should be written");
    fs::write(templates_root.join("fonts.jinja"), "{{ job.name }}\n")
        .expect("template should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "numi.toml", "--job", "fonts"])
        .current_dir(&working_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");

    assert_eq!(json["modules"][0]["kind"], "fonts");
    assert_eq!(json["modules"][0]["entries"][0]["kind"], "font");
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["familyName"],
        "Lettown Hills"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["styleName"],
        "Italic"
    );
    assert_eq!(
        json["modules"][0]["entries"][0]["properties"]["postScriptName"],
        "LettownHills-Italic"
    );
    assert_eq!(
        json["modules"][0]["properties"]["families"][0]["name"],
        "Lettown Hills Italic"
    );
    assert_eq!(
        json["modules"][0]["properties"]["families"][0]["fonts"][0]["fileName"],
        "rank.otf"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
