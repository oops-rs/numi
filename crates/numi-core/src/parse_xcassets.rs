use camino::Utf8PathBuf;
use numi_diagnostics::{Diagnostic, Severity};
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ParseXcassetsError {
    ParseCatalog { source: xcassets::ParseError },
    InvalidCatalogPath { path: PathBuf },
}

impl std::fmt::Display for ParseXcassetsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseCatalog { source } => write!(f, "{source}"),
            Self::InvalidCatalogPath { path } => write!(
                f,
                "asset catalog path {} is not valid UTF-8 and cannot be represented in the IR",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ParseXcassetsError {}

impl From<xcassets::ParseError> for ParseXcassetsError {
    fn from(source: xcassets::ParseError) -> Self {
        Self::ParseCatalog { source }
    }
}

#[derive(Debug)]
pub struct XcassetsReport {
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}

pub fn parse_catalog(catalog_path: &Path) -> Result<XcassetsReport, ParseXcassetsError> {
    let report = xcassets::parse_catalog(catalog_path).map_err(ParseXcassetsError::from)?;
    let mut entries = Vec::new();
    let mut warnings = map_xcassets_diagnostics(&report.diagnostics, catalog_path);

    walk_nodes(
        &report.catalog.children,
        catalog_path,
        &mut entries,
        &mut warnings,
    )?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(XcassetsReport { entries, warnings })
}

fn walk_nodes(
    nodes: &[xcassets::Node],
    catalog_root: &Path,
    entries: &mut Vec<RawEntry>,
    warnings: &mut Vec<Diagnostic>,
) -> Result<(), ParseXcassetsError> {
    for node in nodes {
        match node {
            xcassets::Node::Group(group) => {
                walk_nodes(&group.children, catalog_root, entries, warnings)?;
            }
            xcassets::Node::ImageSet(node) => {
                if node.contents.is_some() {
                    entries.push(image_entry(node, catalog_root)?);
                }
            }
            xcassets::Node::ColorSet(node) => {
                if node.contents.is_some() {
                    entries.push(color_entry(node, catalog_root)?);
                }
            }
            xcassets::Node::AppIconSet(node) => warnings.push(unsupported_node_warning(
                catalog_root,
                &node.relative_path,
                "appiconset",
            )),
            xcassets::Node::Opaque(node) => {
                walk_nodes(&node.children, catalog_root, entries, warnings)?;
            }
        }
    }
    Ok(())
}

fn image_entry(
    node: &xcassets::ImageSetNode,
    catalog_root: &Path,
) -> Result<RawEntry, ParseXcassetsError> {
    let asset_name = asset_name_from_relative(&node.relative_path, ".imageset");
    let source_path = utf8_path(&catalog_root.join(&node.relative_path))?;

    Ok(RawEntry {
        path: asset_name.clone(),
        source_path,
        kind: EntryKind::Image,
        properties: asset_properties(&asset_name),
    })
}

fn color_entry(
    node: &xcassets::ColorSetNode,
    catalog_root: &Path,
) -> Result<RawEntry, ParseXcassetsError> {
    let asset_name = asset_name_from_relative(&node.relative_path, ".colorset");
    let source_path = utf8_path(&catalog_root.join(&node.relative_path))?;

    Ok(RawEntry {
        path: asset_name.clone(),
        source_path,
        kind: EntryKind::Color,
        properties: asset_properties(&asset_name),
    })
}

fn asset_name_from_relative(relative: &Path, suffix: &str) -> String {
    let mut components = relative
        .iter()
        .map(|component| component.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if let Some(last) = components.last_mut() {
        if let Some(stripped) = last.strip_suffix(suffix) {
            *last = stripped.to_owned();
        }
    }

    components.join("/")
}

fn unsupported_node_warning(catalog_root: &Path, relative_path: &Path, kind: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        message: format!("unsupported asset node kind `{kind}` was skipped"),
        hint: None,
        job: None,
        path: Some(catalog_root.join(relative_path)),
    }
}

fn map_xcassets_diagnostics(
    diagnostics: &[xcassets::Diagnostic],
    catalog_path: &Path,
) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            let resolved_path = if diagnostic.path.as_os_str().is_empty() {
                catalog_path.to_path_buf()
            } else {
                catalog_path.join(&diagnostic.path)
            };

            Diagnostic {
                severity: Severity::Warning,
                message: diagnostic.message.clone(),
                hint: None,
                job: None,
                path: Some(resolved_path),
            }
        })
        .collect()
}

fn utf8_path(path: &Path) -> Result<Utf8PathBuf, ParseXcassetsError> {
    Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseXcassetsError::InvalidCatalogPath { path })
}

fn asset_properties(asset_name: &str) -> Metadata {
    Metadata::from([(
        "assetName".to_string(),
        Value::String(asset_name.to_owned()),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
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

    #[test]
    fn unsupported_asset_nodes_do_not_emit_supported_entries_without_warning() {
        let temp_dir = make_temp_dir("parse-xcassets-unsupported-asset-node");
        let catalog_dir = temp_dir.join("Assets.xcassets");
        let imageset_dir = catalog_dir.join("Supported.imageset");
        let appiconset_dir = catalog_dir.join("AppIcon.appiconset");

        fs::create_dir_all(&imageset_dir).expect("imageset dir should exist");
        fs::create_dir_all(&appiconset_dir).expect("unsupported appiconset dir should exist");

        fs::write(
            catalog_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("catalog contents should be written");

        fs::write(
            imageset_dir.join("Contents.json"),
            r#"{"images": [], "info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("imageset contents should be written");

        fs::write(
            appiconset_dir.join("Contents.json"),
            r#"{"images": [], "info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("unsupported asset contents should be written");

        let report = parse_catalog(&catalog_dir).expect("catalog should parse");
        let warning = report
            .warnings
            .first()
            .expect("unsupported asset node warning should be present");

        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].kind, EntryKind::Image);
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(warning.severity, numi_diagnostics::Severity::Warning);
        assert!(warning.message.contains("unsupported asset node kind"));
        let warning_path = warning
            .path
            .as_ref()
            .expect("unsupported node warning should contain a path");
        assert!(warning_path.ends_with("AppIcon.appiconset"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn malformed_imageset_is_skipped_from_entries() {
        let temp_dir = make_temp_dir("parse-xcassets-malformed-imageset");
        let catalog_dir = temp_dir.join("Assets.xcassets");
        let valid_imageset_dir = catalog_dir.join("Valid.imageset");
        let broken_imageset_dir = catalog_dir.join("Broken.imageset");

        fs::create_dir_all(&valid_imageset_dir).expect("valid imageset dir should exist");
        fs::create_dir_all(&broken_imageset_dir).expect("broken imageset dir should exist");

        fs::write(
            catalog_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("catalog contents should be written");

        fs::write(
            valid_imageset_dir.join("Contents.json"),
            r#"{"images": [], "info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("valid imageset contents should be written");

        fs::write(broken_imageset_dir.join("Contents.json"), r#"{"images": "#)
            .expect("broken imageset contents should be written");

        let report = parse_catalog(&catalog_dir).expect("catalog should parse");

        assert_eq!(report.entries.len(), 1);
        assert_eq!(report.entries[0].path, "Valid");
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.message.contains("invalid Contents.json")),
            "malformed imageset should emit a parser warning"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn unsupported_opaque_folder_emits_single_warning() {
        let temp_dir = make_temp_dir("parse-xcassets-opaque-warning");
        let catalog_dir = temp_dir.join("Assets.xcassets");
        let opaque_dir = catalog_dir.join("Widget.imagestack");

        fs::create_dir_all(&opaque_dir).expect("opaque folder should exist");

        fs::write(
            catalog_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("catalog contents should be written");

        fs::write(
            opaque_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("opaque folder contents should be written");

        let report = parse_catalog(&catalog_dir).expect("catalog should parse");
        let opaque_warnings = report
            .warnings
            .iter()
            .filter(|warning| {
                warning
                    .path
                    .as_ref()
                    .is_some_and(|path| path.ends_with("Widget.imagestack"))
            })
            .collect::<Vec<_>>();

        assert!(report.entries.is_empty());
        assert_eq!(opaque_warnings.len(), 1);
        assert!(
            opaque_warnings[0]
                .message
                .contains("unsupported folder type")
                || opaque_warnings[0]
                    .message
                    .contains("unsupported asset node kind"),
            "opaque warning should be present once"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn supported_assets_nested_under_opaque_node_are_discovered() {
        let temp_dir = make_temp_dir("parse-xcassets-opaque-nested-supported");
        let catalog_dir = temp_dir.join("Assets.xcassets");
        let opaque_dir = catalog_dir.join("Atlas.spriteatlas");
        let nested_imageset_dir = opaque_dir.join("Nested.imageset");

        fs::create_dir_all(&nested_imageset_dir).expect("nested imageset dir should exist");

        fs::write(
            catalog_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("catalog contents should be written");

        fs::write(
            opaque_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("opaque contents should be written");

        fs::write(
            nested_imageset_dir.join("Contents.json"),
            r#"{"images": [], "info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("nested imageset contents should be written");

        let report = parse_catalog(&catalog_dir).expect("catalog should parse");

        assert!(
            report.entries.iter().any(|entry| {
                entry.kind == EntryKind::Image && entry.path == "Atlas.spriteatlas/Nested"
            }),
            "nested supported imageset should produce an entry"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
