use camino::Utf8PathBuf;
use numi_diagnostics::{Diagnostic, Severity};
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XcassetsReport {
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}

pub fn parse_catalog(catalog_path: &Path) -> Result<XcassetsReport, ParseXcassetsError> {
    let index = xcassets::index_asset_references(catalog_path).map_err(ParseXcassetsError::from)?;
    let mut entries = Vec::new();
    let mut warnings = map_xcassets_diagnostics(&index.diagnostics, catalog_path);

    for reference in &index.references {
        match reference.kind {
            xcassets::AssetReferenceKind::Image | xcassets::AssetReferenceKind::Color => {
                entries.push(entry_from_reference(reference, catalog_path)?);
            }
            xcassets::AssetReferenceKind::AppIcon => warnings.push(unsupported_node_warning(
                catalog_path,
                &reference.relative_path,
                "appiconset",
            )),
        }
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(XcassetsReport { entries, warnings })
}

fn entry_from_reference(
    reference: &xcassets::AssetReference,
    catalog_root: &Path,
) -> Result<RawEntry, ParseXcassetsError> {
    let (kind, suffix) = match reference.kind {
        xcassets::AssetReferenceKind::Image => (EntryKind::Image, ".imageset"),
        xcassets::AssetReferenceKind::Color => (EntryKind::Color, ".colorset"),
        xcassets::AssetReferenceKind::AppIcon => unreachable!("app icon references are warnings"),
    };
    let asset_name = asset_name_from_relative(&reference.relative_path, suffix);
    let source_path = utf8_path(&catalog_root.join(&reference.relative_path))?;

    Ok(RawEntry {
        path: asset_name.clone(),
        source_path,
        kind,
        properties: asset_properties(&asset_name),
    })
}

fn asset_name_from_relative(relative: &Path, suffix: &str) -> String {
    let mut components = relative
        .iter()
        .map(|component| component.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if let Some(last) = components.last_mut()
        && let Some(stripped) = last.strip_suffix(suffix)
    {
        *last = stripped.to_owned();
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
            } else if diagnostic.path.is_absolute() {
                diagnostic.path.clone()
            } else {
                catalog_path.join(&diagnostic.path)
            };
            let severity = match diagnostic.severity {
                xcassets::Severity::Warning => Severity::Warning,
                xcassets::Severity::Error => Severity::Error,
            };

            Diagnostic {
                severity,
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
    fn malformed_imageset_still_emits_entry_reference() {
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

        assert_eq!(report.entries.len(), 2);
        assert!(
            report
                .entries
                .iter()
                .any(|entry| entry.kind == EntryKind::Image && entry.path == "Broken")
        );
        assert!(
            report
                .entries
                .iter()
                .any(|entry| entry.kind == EntryKind::Image && entry.path == "Valid")
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn imageset_without_leaf_contents_json_still_emits_entry() {
        let temp_dir = make_temp_dir("parse-xcassets-missing-leaf-contents");
        let catalog_dir = temp_dir.join("Assets.xcassets");
        let imageset_dir = catalog_dir.join("Loose.imageset");

        fs::create_dir_all(&imageset_dir).expect("imageset dir should exist");

        fs::write(
            catalog_dir.join("Contents.json"),
            r#"{"info": {"author": "xcode", "version": 1}}"#,
        )
        .expect("catalog contents should be written");

        let report = parse_catalog(&catalog_dir).expect("catalog should parse");

        assert!(
            report
                .entries
                .iter()
                .any(|entry| entry.kind == EntryKind::Image && entry.path == "Loose"),
            "imageset without leaf Contents.json should still produce an image entry"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn unsupported_opaque_folder_is_ignored_without_warning() {
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

        assert!(report.entries.is_empty());
        assert!(report.warnings.is_empty());

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
