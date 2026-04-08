use minijinja::Environment;
use std::{fs, path::Path};

use crate::context::AssetTemplateContext;

const SWIFTUI_ASSETS_TEMPLATE: &str =
    include_str!("../../../templates/builtin/swiftui-assets.jinja");
const L10N_TEMPLATE: &str = include_str!("../../../templates/builtin/l10n.jinja");

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
    let template_source = match builtin_name {
        "swiftui-assets" => SWIFTUI_ASSETS_TEMPLATE,
        "l10n" => L10N_TEMPLATE,
        other => return Err(RenderError::UnknownBuiltin(other.to_owned())),
    };

    render_template_source(builtin_name, template_source, context)
}

pub fn render_path(path: &Path, context: &AssetTemplateContext) -> Result<String, RenderError> {
    let template_source = fs::read_to_string(path).map_err(|source| RenderError::ReadTemplate {
        path: path.to_path_buf(),
        source,
    })?;

    render_template_source("custom", &template_source, context)
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

fn build_environment() -> Environment<'static> {
    let mut environment = Environment::new();
    environment.set_keep_trailing_newline(true);
    environment.add_filter("lower_first", lower_first);
    environment.add_filter("string_literal", string_literal);
    environment
}

fn lower_first(value: String) -> String {
    if let Some(inner) = value
        .strip_prefix('`')
        .and_then(|trimmed| trimmed.strip_suffix('`'))
    {
        return format!("`{}`", lower_first(inner.to_owned()));
    }

    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {
            let mut lowered = String::new();
            lowered.push(first.to_ascii_lowercase());
            lowered.push_str(chars.as_str());
            lowered
        }
        Some(_) | None => value,
    }
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
    fn renders_custom_template_from_disk() {
        let temp_dir = make_temp_dir("render-custom-template");
        let template_path = temp_dir.join("custom.jinja");
        fs::write(
            &template_path,
            "{{ job.swiftIdentifier }}|{{ modules[0].properties.tableName }}|{{ modules[0].entries[0].properties.translation }}\n",
        )
        .expect("template should be written");

        let rendered =
            render_path(&template_path, &l10n_context()).expect("template should render");

        assert_eq!(rendered, "L10n|Localizable|Profile\n");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
