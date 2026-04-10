use minijinja::{Environment, Error, ErrorKind};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::context::AssetTemplateContext;

const SWIFTUI_ASSETS_TEMPLATE: &str = include_str!("../../../templates/swift/swiftui-assets.jinja");
const L10N_TEMPLATE: &str = include_str!("../../../templates/swift/l10n.jinja");
const FILES_TEMPLATE: &str = include_str!("../../../templates/swift/files.jinja");
const ENTRY_TEMPLATE_NAME: &str = "__numi_entry__";
const FILE_TEMPLATE_PREFIX: &str = "file:";
const INCLUDE_REQUEST_PREFIX: &str = "include:";

#[derive(Debug)]
pub enum RenderError {
    UnknownBuiltin(String),
    ReadTemplate {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    RegisterTemplate(minijinja::Error),
    Render(minijinja::Error),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownBuiltin(name) => write!(f, "unknown built-in template `{name}`"),
            Self::ReadTemplate { path, source } => {
                write!(f, "failed to read template {}: {source}", path.display())
            }
            Self::RegisterTemplate(error) => write!(f, "failed to register template: {error}"),
            Self::Render(error) => write!(f, "template rendering failed: {error}"),
        }
    }
}

impl std::error::Error for RenderError {}

pub fn render_builtin(
    builtin_name: &str,
    context: &AssetTemplateContext,
) -> Result<String, RenderError> {
    let template_source = builtin_template_source(builtin_name)?;

    render_template_source(builtin_name, template_source, context)
}

pub fn builtin_template_source(builtin_name: &str) -> Result<&'static str, RenderError> {
    match builtin_name {
        "swiftui-assets" => Ok(SWIFTUI_ASSETS_TEMPLATE),
        "l10n" => Ok(L10N_TEMPLATE),
        "files" => Ok(FILES_TEMPLATE),
        other => Err(RenderError::UnknownBuiltin(other.to_owned())),
    }
}

pub fn resolve_template_entry_path(
    config_root: &Path,
    configured_path: &str,
) -> Result<PathBuf, RenderError> {
    let direct = config_root.join(configured_path);
    let with_jinja = config_root.join(format!("{configured_path}.jinja"));

    match (direct.is_file(), with_jinja.is_file()) {
        (true, false) => Ok(direct),
        (false, true) => Ok(with_jinja),
        (false, false) => Err(RenderError::ReadTemplate {
            path: direct,
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "template file was not found",
            ),
        }),
        (true, true) => Err(RenderError::Render(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "ambiguous template path `{configured_path}` matched both extensionless and `.jinja` files"
            ),
        ))),
    }
}

pub fn render_path(
    path: &Path,
    config_root: &Path,
    context: &AssetTemplateContext,
) -> Result<String, RenderError> {
    let template_source = fs::read_to_string(path).map_err(|source| RenderError::ReadTemplate {
        path: path.to_path_buf(),
        source,
    })?;

    let mut environment = build_custom_environment(path, config_root);
    environment
        .add_template_owned(ENTRY_TEMPLATE_NAME.to_string(), template_source)
        .map_err(RenderError::RegisterTemplate)?;

    let rendered = environment
        .get_template(ENTRY_TEMPLATE_NAME)
        .map_err(RenderError::Render)?
        .render(context)
        .map_err(RenderError::Render)?;

    Ok(normalize_blank_lines(&rendered))
}

pub fn collect_custom_template_dependencies(
    path: &Path,
    config_root: &Path,
) -> Result<Option<Vec<PathBuf>>, RenderError> {
    let mut visited = BTreeSet::new();
    if collect_template_dependencies_recursive(path, config_root, &mut visited)? {
        Ok(Some(visited.into_iter().collect()))
    } else {
        Ok(None)
    }
}

fn render_template_source(
    template_name: &str,
    template_source: &str,
    context: &AssetTemplateContext,
) -> Result<String, RenderError> {
    let mut environment = build_environment();
    environment
        .add_template(template_name, template_source)
        .map_err(RenderError::RegisterTemplate)?;

    let rendered = environment
        .get_template(template_name)
        .expect("template should exist after registration")
        .render(context)
        .map_err(RenderError::Render)?;

    Ok(normalize_blank_lines(&rendered))
}

fn collect_template_dependencies_recursive(
    path: &Path,
    config_root: &Path,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<bool, RenderError> {
    if !visited.insert(path.to_path_buf()) {
        return Ok(true);
    }

    let template_source = fs::read_to_string(path).map_err(|source| RenderError::ReadTemplate {
        path: path.to_path_buf(),
        source,
    })?;
    let Some(references) = extract_literal_template_references(&template_source) else {
        return Ok(false);
    };
    let local_root = path.parent().unwrap_or_else(|| Path::new("."));

    for include_name in references {
        let resolved =
            resolve_include(&include_name, local_root, config_root).map_err(RenderError::Render)?;
        if !collect_template_dependencies_recursive(&resolved, config_root, visited)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn build_environment() -> Environment<'static> {
    let mut environment = Environment::new();
    environment.set_keep_trailing_newline(true);
    environment.add_filter("lower_first", lower_first);
    environment.add_filter("string_literal", string_literal);
    environment
}

fn build_custom_environment(entry_path: &Path, config_root: &Path) -> Environment<'static> {
    let mut environment = build_environment();
    let entry_dir = entry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let config_root = config_root.to_path_buf();
    let join_entry_dir = entry_dir.clone();
    let join_config_root = config_root.clone();

    environment.set_path_join_callback(move |name, parent| {
        Cow::Owned(resolve_include_name(
            name,
            parent,
            &join_entry_dir,
            &join_config_root,
        ))
    });

    let load_entry_dir = entry_dir;
    let load_config_root = config_root;
    environment
        .set_loader(move |name| load_custom_template(name, &load_entry_dir, &load_config_root));

    environment
}

fn resolve_include_name(
    include_name: &str,
    parent_name: &str,
    entry_dir: &Path,
    config_root: &Path,
) -> String {
    let local_root = parent_local_root(parent_name, entry_dir);

    resolve_include(include_name, &local_root, config_root)
        .map(|path| encode_loaded_template_path(&path))
        .unwrap_or_else(|_| encode_include_request(parent_name, include_name))
}

fn load_custom_template(
    name: &str,
    entry_dir: &Path,
    config_root: &Path,
) -> Result<Option<String>, minijinja::Error> {
    if let Some(path) = decode_loaded_template_path(name) {
        return fs::read_to_string(&path).map(Some).map_err(|source| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("failed to read included template {}", path.display()),
            )
            .with_source(source)
        });
    }

    let Some((parent_name, include_name)) = decode_include_request(name) else {
        return Ok(None);
    };
    let local_root = parent_local_root(parent_name, entry_dir);
    let path = resolve_include(include_name, &local_root, config_root)?;

    fs::read_to_string(&path).map(Some).map_err(|source| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("failed to read included template {}", path.display()),
        )
        .with_source(source)
    })
}

fn resolve_include(
    include_name: &str,
    local_root: &Path,
    config_root: &Path,
) -> Result<PathBuf, Error> {
    let local_candidate = safe_template_join(local_root, include_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("invalid include path `{include_name}`"),
        )
    })?;
    let shared_candidate = safe_template_join(config_root, include_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("invalid include path `{include_name}`"),
        )
    })?;

    let local_exists = local_candidate.exists();
    let shared_exists = shared_candidate.exists();

    match (local_exists, shared_exists) {
        (true, false) => Ok(local_candidate),
        (false, true) => Ok(shared_candidate),
        (false, false) => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "missing included template `{include_name}`; searched local root {} and shared root {}",
                local_root.display(),
                config_root.display()
            ),
        )),
        (true, true) if local_candidate == shared_candidate => Ok(local_candidate),
        (true, true) => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "ambiguous included template `{include_name}`; matched {} and {}",
                local_candidate.display(),
                shared_candidate.display()
            ),
        )),
    }
}

fn parent_local_root(parent_name: &str, entry_dir: &Path) -> PathBuf {
    if parent_name == ENTRY_TEMPLATE_NAME {
        return entry_dir.to_path_buf();
    }

    decode_loaded_template_path(parent_name)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| entry_dir.to_path_buf())
}

fn encode_loaded_template_path(path: &Path) -> String {
    format!("{FILE_TEMPLATE_PREFIX}{}", path.display())
}

fn encode_include_request(parent_name: &str, include_name: &str) -> String {
    format!("{INCLUDE_REQUEST_PREFIX}{parent_name}|{include_name}")
}

fn decode_include_request(name: &str) -> Option<(&str, &str)> {
    let payload = name.strip_prefix(INCLUDE_REQUEST_PREFIX)?;
    payload.split_once('|')
}

fn decode_loaded_template_path(name: &str) -> Option<PathBuf> {
    name.strip_prefix(FILE_TEMPLATE_PREFIX).map(PathBuf::from)
}

fn safe_template_join(base: &Path, include_name: &str) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    for segment in include_name.split('/') {
        if segment.starts_with('.') || segment.contains('\\') {
            return None;
        }
        path.push(segment);
    }
    Some(path)
}

fn extract_literal_template_references(template_source: &str) -> Option<Vec<String>> {
    let mut references = Vec::new();
    let mut rest = template_source;

    while let Some(tag_start) = rest.find("{%") {
        rest = &rest[tag_start + 2..];
        let Some(tag_end) = rest.find("%}") else {
            break;
        };

        let tag_body = rest[..tag_end].trim();
        rest = &rest[tag_end + 2..];

        let mut parts = tag_body.splitn(2, char::is_whitespace);
        let Some(keyword) = parts.next() else {
            continue;
        };
        let Some(remainder) = parts.next() else {
            continue;
        };

        if !matches!(keyword, "include" | "extends" | "import" | "from") {
            continue;
        }

        let (literal, _tail) = parse_quoted_literal(remainder.trim_start())?;
        references.push(literal.to_owned());
    }

    Some(references)
}

fn parse_quoted_literal(input: &str) -> Option<(&str, &str)> {
    let mut chars = input.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == quote {
            return Some((&input[1..index], &input[index + quote.len_utf8()..]));
        }
    }

    None
}

fn lower_first(value: String) -> String {
    if let Some(inner) = value
        .strip_prefix('`')
        .and_then(|trimmed| trimmed.strip_suffix('`'))
    {
        return format!("`{}`", lower_first(inner.to_owned()));
    }

    let chars = value.chars().collect::<Vec<_>>();
    if chars.is_empty() || !chars[0].is_ascii_uppercase() {
        return value;
    }

    let mut prefix_len = 1;
    while prefix_len < chars.len() && chars[prefix_len].is_ascii_uppercase() {
        prefix_len += 1;
    }

    let lower_count = if prefix_len == chars.len() {
        prefix_len
    } else if prefix_len == 1 {
        1
    } else {
        prefix_len - 1
    };

    let mut lowered = String::with_capacity(value.len());
    for ch in &chars[..lower_count] {
        lowered.push(ch.to_ascii_lowercase());
    }
    for ch in &chars[lower_count..] {
        lowered.push(*ch);
    }
    lowered
}

fn string_literal(value: String) -> String {
    serde_json::to_string(&value).expect("string literal should serialize")
}

fn normalize_blank_lines(rendered: &str) -> String {
    let mut normalized = rendered.to_owned();
    while normalized.contains("\n\n\n") {
        normalized = normalized.replace("\n\n\n", "\n\n");
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::AssetTemplateContext;
    use camino::Utf8PathBuf;
    use numi_ir::{EntryKind, Metadata, ModuleKind, ResourceEntry, ResourceModule};
    use serde_json::json;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn l10n_context() -> AssetTemplateContext {
        AssetTemplateContext::new(
            "l10n",
            "Generated/L10n.swift",
            "internal",
            "module",
            None,
            &[ResourceModule {
                id: "Localizable".to_string(),
                kind: ModuleKind::Strings,
                name: "Localizable".to_string(),
                entries: vec![ResourceEntry {
                    id: "profile.title".to_string(),
                    name: "profile.title".to_string(),
                    source_path: Utf8PathBuf::from("fixture"),
                    swift_identifier: "ProfileTitle".to_string(),
                    kind: EntryKind::StringKey,
                    children: Vec::new(),
                    properties: Metadata::from([
                        ("key".to_string(), json!("profile.title")),
                        ("translation".to_string(), json!("Profile")),
                    ]),
                    metadata: Metadata::new(),
                }],
                metadata: Metadata::from([("tableName".to_string(), json!("Localizable"))]),
            }],
        )
        .expect("context should build")
    }

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

    #[test]
    fn renders_builtin_l10n_template() {
        let rendered = render_builtin("l10n", &l10n_context()).expect("template should render");

        assert_eq!(
            rendered,
            r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let profileTitle = tr("Localizable", "profile.title")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
        );
    }

    #[test]
    fn rejects_unknown_builtin_template_name() {
        let error = render_builtin("not-a-real-template", &l10n_context())
            .expect_err("unknown built-ins should be rejected");

        assert!(
            matches!(error, RenderError::UnknownBuiltin(name) if name == "not-a-real-template")
        );
    }

    #[test]
    fn renders_builtin_files_template() {
        let context = AssetTemplateContext::new(
            "files",
            "Generated/Files.swift",
            "internal",
            "module",
            None,
            &[ResourceModule {
                id: "Fixtures".to_string(),
                kind: ModuleKind::Files,
                name: "Fixtures".to_string(),
                entries: vec![
                    ResourceEntry {
                        id: "Onboarding".to_string(),
                        name: "Onboarding".to_string(),
                        source_path: Utf8PathBuf::from("virtual"),
                        swift_identifier: "Onboarding".to_string(),
                        kind: EntryKind::Namespace,
                        children: vec![ResourceEntry {
                            id: "Onboarding/welcome-video.mp4".to_string(),
                            name: "welcome-video.mp4".to_string(),
                            source_path: Utf8PathBuf::from("fixture"),
                            swift_identifier: "WelcomeVideoMp4".to_string(),
                            kind: EntryKind::Data,
                            children: Vec::new(),
                            properties: Metadata::from([(
                                "relativePath".to_string(),
                                json!("Onboarding/welcome-video.mp4"),
                            )]),
                            metadata: Metadata::new(),
                        }],
                        properties: Metadata::new(),
                        metadata: Metadata::new(),
                    },
                    ResourceEntry {
                        id: "faq.pdf".to_string(),
                        name: "faq.pdf".to_string(),
                        source_path: Utf8PathBuf::from("fixture"),
                        swift_identifier: "FaqPdf".to_string(),
                        kind: EntryKind::Data,
                        children: Vec::new(),
                        properties: Metadata::from([(
                            "relativePath".to_string(),
                            json!("faq.pdf"),
                        )]),
                        metadata: Metadata::new(),
                    },
                ],
                metadata: Metadata::new(),
            }],
        )
        .expect("context should build");

        let rendered = render_builtin("files", &context).expect("template should render");

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
    }

    #[test]
    fn renders_custom_template_from_disk() {
        let temp_dir = make_temp_dir("render-custom-template");
        let template_path = temp_dir.join("custom.jinja");
        fs::write(
            &template_path,
            "{{ job.swiftIdentifier }}|{{ modules[0].properties.tableName }}|{{ modules[0].entries[0].properties.translation }}\n",
        )
        .expect("template should be written");

        let rendered = render_path(&template_path, &temp_dir, &l10n_context())
            .expect("template should render");

        assert_eq!(rendered, "L10n|Localizable|Profile\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn renders_local_include_from_template_directory() {
        let temp_dir = make_temp_dir("render-local-include");
        let config_root = temp_dir.join("Config");
        let templates_dir = config_root.join("Templates");
        fs::create_dir_all(templates_dir.join("partials")).expect("templates dir should exist");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}\n",
        )
        .expect("main template should be written");
        fs::write(templates_dir.join("partials/header.jinja"), "LOCAL")
            .expect("local partial should be written");

        let rendered = render_path(
            &templates_dir.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect("template should render");

        assert_eq!(rendered, "LOCAL|L10n\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn renders_include_from_shared_config_root() {
        let temp_dir = make_temp_dir("render-shared-include");
        let config_root = temp_dir.join("Config");
        let templates_dir = config_root.join("Templates");
        fs::create_dir_all(&templates_dir).expect("templates dir should exist");
        fs::create_dir_all(config_root.join("partials")).expect("shared partial dir should exist");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/header.jinja\" %}|{{ modules[0].name }}\n",
        )
        .expect("main template should be written");
        fs::write(config_root.join("partials/header.jinja"), "SHARED")
            .expect("shared partial should be written");

        let rendered = render_path(
            &templates_dir.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect("template should render");

        assert_eq!(rendered, "SHARED|Localizable\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn renders_nested_includes_from_mixed_roots() {
        let temp_dir = make_temp_dir("render-nested-includes");
        let config_root = temp_dir.join("Config");
        let templates_dir = config_root.join("Templates");
        fs::create_dir_all(templates_dir.join("partials")).expect("templates dir should exist");
        fs::create_dir_all(config_root.join("shared")).expect("shared include dir should exist");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/outer.jinja\" %}\n",
        )
        .expect("main template should be written");
        fs::write(
            templates_dir.join("partials/outer.jinja"),
            "OUTER[{% include \"shared/inner.jinja\" %}]",
        )
        .expect("outer partial should be written");
        fs::write(
            config_root.join("shared/inner.jinja"),
            "{{ job.swiftIdentifier }}",
        )
        .expect("shared nested partial should be written");

        let rendered = render_path(
            &templates_dir.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect("template should render");

        assert_eq!(rendered, "OUTER[L10n]\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn missing_include_reports_local_and_shared_roots() {
        let temp_dir = make_temp_dir("render-missing-include");
        let config_root = temp_dir.join("Config");
        let templates_dir = config_root.join("Templates");
        fs::create_dir_all(&templates_dir).expect("templates dir should exist");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/missing.jinja\" %}\n",
        )
        .expect("main template should be written");

        let error = render_path(
            &templates_dir.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect_err("missing include should fail");

        let message = error.to_string();
        assert!(message.contains("missing included template `partials/missing.jinja`"));
        assert!(message.contains("Templates"));
        assert!(message.contains("Config"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn ambiguous_include_reports_both_candidate_paths() {
        let temp_dir = make_temp_dir("render-ambiguous-include");
        let config_root = temp_dir.join("Config");
        let templates_dir = config_root.join("Templates");
        fs::create_dir_all(templates_dir.join("partials")).expect("local partial dir should exist");
        fs::create_dir_all(config_root.join("partials")).expect("shared partial dir should exist");
        fs::write(
            templates_dir.join("main.jinja"),
            "{% include \"partials/header.jinja\" %}\n",
        )
        .expect("main template should be written");
        fs::write(templates_dir.join("partials/header.jinja"), "LOCAL")
            .expect("local partial should exist");
        fs::write(config_root.join("partials/header.jinja"), "SHARED")
            .expect("shared partial should exist");

        let error = render_path(
            &templates_dir.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect_err("ambiguous include should fail");

        let message = error.to_string();
        assert!(message.contains("ambiguous included template `partials/header.jinja`"));
        assert!(message.contains("Templates/partials/header.jinja"));
        assert!(message.contains("Config/partials/header.jinja"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn renders_include_from_same_config_root_without_false_ambiguity() {
        let temp_dir = make_temp_dir("render-same-config-root-include");
        let config_root = temp_dir.join("Config");
        fs::create_dir_all(config_root.join("partials")).expect("partials dir should exist");
        fs::write(
            config_root.join("main.jinja"),
            "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}\n",
        )
        .expect("main template should be written");
        fs::write(config_root.join("partials/header.jinja"), "ROOT")
            .expect("root partial should be written");

        let rendered = render_path(
            &config_root.join("main.jinja"),
            &config_root,
            &l10n_context(),
        )
        .expect("template should render");

        assert_eq!(rendered, "ROOT|L10n\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn resolves_template_entry_path_when_configured_path_exists_directly() {
        let temp_dir = make_temp_dir("resolve-template-entry-direct");
        let config_root = temp_dir.join("Config");
        let template_path = config_root.join("Templates/l10n");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template directory should exist");
        fs::write(&template_path, "{{ job.swiftIdentifier }}\n")
            .expect("template should be written");

        let resolved = resolve_template_entry_path(&config_root, "Templates/l10n")
            .expect("direct template path should resolve");

        assert_eq!(resolved, template_path);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn resolves_template_entry_path_via_jinja_suffix() {
        let temp_dir = make_temp_dir("resolve-template-entry-jinja");
        let config_root = temp_dir.join("Config");
        let template_path = config_root.join("Templates/l10n.jinja");
        fs::create_dir_all(
            template_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template directory should exist");
        fs::write(&template_path, "{{ job.swiftIdentifier }}\n")
            .expect("template should be written");

        let resolved = resolve_template_entry_path(&config_root, "Templates/l10n")
            .expect("jinja template path should resolve");

        assert_eq!(resolved, template_path);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn resolve_template_entry_path_reports_missing_path() {
        let temp_dir = make_temp_dir("resolve-template-entry-missing");
        let config_root = temp_dir.join("Config");
        fs::create_dir_all(&config_root).expect("config root should exist");

        let error = resolve_template_entry_path(&config_root, "Templates/l10n")
            .expect_err("missing template path should fail");

        match error {
            RenderError::ReadTemplate { path, source } => {
                assert_eq!(path, config_root.join("Templates/l10n"));
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected read template error, got {other:?}"),
        }

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn resolve_template_entry_path_rejects_ambiguous_matches() {
        let temp_dir = make_temp_dir("resolve-template-entry-ambiguous");
        let config_root = temp_dir.join("Config");
        let direct_path = config_root.join("Templates/l10n");
        let jinja_path = config_root.join("Templates/l10n.jinja");
        fs::create_dir_all(
            direct_path
                .parent()
                .expect("template path should have parent"),
        )
        .expect("template directory should exist");
        fs::write(&direct_path, "DIRECT").expect("direct template should be written");
        fs::write(&jinja_path, "JINJA").expect("jinja template should be written");

        let error = resolve_template_entry_path(&config_root, "Templates/l10n")
            .expect_err("ambiguous template path should fail");

        match error {
            RenderError::Render(source) => {
                let message = source.to_string();
                assert!(message.contains("ambiguous template path `Templates/l10n`"));
                assert!(message.contains("extensionless"));
                assert!(message.contains(".jinja"));
            }
            other => panic!("expected render error, got {other:?}"),
        }

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
