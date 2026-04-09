use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum ParseXcassetsError {
    ReadDirectory {
        path: PathBuf,
        source: io::Error,
    },
    ReadFile {
        path: PathBuf,
        source: io::Error,
    },
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidCatalogPath {
        path: PathBuf,
    },
}

impl std::fmt::Display for ParseXcassetsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDirectory { path, source } => {
                write!(
                    f,
                    "failed to read asset catalog directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadFile { path, source } => {
                write!(
                    f,
                    "failed to read asset contents {}: {source}",
                    path.display()
                )
            }
            Self::ParseJson { path, source } => {
                write!(
                    f,
                    "failed to parse asset contents {}: {source}",
                    path.display()
                )
            }
            Self::InvalidCatalogPath { path } => write!(
                f,
                "asset catalog path {} is not valid UTF-8 and cannot be represented in the IR",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ParseXcassetsError {}

#[derive(Debug, Deserialize)]
struct CatalogContents {
    info: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ImageSetContents {
    images: serde_json::Value,
    info: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ColorSetContents {
    colors: serde_json::Value,
    info: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug)]
struct ParseCatalogReport {
    entries: Vec<RawEntry>,
    warnings: Vec<numi_diagnostics::Diagnostic>,
}

pub fn parse_catalog(catalog_path: &Path) -> Result<Vec<RawEntry>, ParseXcassetsError> {
    parse_catalog_entries(catalog_path)
}

#[cfg(test)]
fn parse_catalog_with_warnings(
    catalog_path: &Path,
) -> Result<ParseCatalogReport, ParseXcassetsError> {
    let raw_entries = parse_catalog_entries(catalog_path)?;

    Ok(ParseCatalogReport {
        entries: raw_entries,
        warnings: Vec::new(),
    })
}

fn parse_catalog_entries(
    catalog_path: &Path,
) -> Result<Vec<RawEntry>, ParseXcassetsError> {
    validate_root_contents(catalog_path)?;

    let mut raw_entries = Vec::new();
    collect_entries(catalog_path, catalog_path, &mut raw_entries)?;
    raw_entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(raw_entries)
}

fn validate_root_contents(catalog_path: &Path) -> Result<(), ParseXcassetsError> {
    let contents_path = catalog_path.join("Contents.json");
    let contents = read_json_file::<CatalogContents>(&contents_path)?;
    let _ = contents.info;
    Ok(())
}

fn collect_entries(
    root: &Path,
    current: &Path,
    raw_entries: &mut Vec<RawEntry>,
) -> Result<(), ParseXcassetsError> {
    let read_dir = fs::read_dir(current).map_err(|source| ParseXcassetsError::ReadDirectory {
        path: current.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseXcassetsError::ReadDirectory {
            path: current.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| ParseXcassetsError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if !file_type.is_dir() {
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".imageset"))
        {
            raw_entries.push(parse_imageset(root, &path)?);
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".colorset"))
        {
            raw_entries.push(parse_colorset(root, &path)?);
            continue;
        }

        collect_entries(root, &path, raw_entries)?;
    }

    Ok(())
}

fn parse_imageset(root: &Path, imageset_path: &Path) -> Result<RawEntry, ParseXcassetsError> {
    let contents_path = imageset_path.join("Contents.json");
    let contents = read_json_file::<ImageSetContents>(&contents_path)?;
    let _ = (contents.images, contents.info);
    let asset_name = asset_path(root, imageset_path, ".imageset")?;

    Ok(RawEntry {
        path: asset_name.clone(),
        source_path: utf8_path(imageset_path)?,
        kind: EntryKind::Image,
        properties: asset_properties(&asset_name),
    })
}

fn parse_colorset(root: &Path, colorset_path: &Path) -> Result<RawEntry, ParseXcassetsError> {
    let contents_path = colorset_path.join("Contents.json");
    let contents = read_json_file::<ColorSetContents>(&contents_path)?;
    let _ = (contents.colors, contents.info);
    let asset_name = asset_path(root, colorset_path, ".colorset")?;

    Ok(RawEntry {
        path: asset_name.clone(),
        source_path: utf8_path(colorset_path)?,
        kind: EntryKind::Color,
        properties: asset_properties(&asset_name),
    })
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ParseXcassetsError> {
    let contents = fs::read_to_string(path).map_err(|source| ParseXcassetsError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&contents).map_err(|source| ParseXcassetsError::ParseJson {
        path: path.to_path_buf(),
        source,
    })
}

fn asset_path(root: &Path, asset_dir: &Path, suffix: &str) -> Result<String, ParseXcassetsError> {
    let relative = asset_dir
        .strip_prefix(root)
        .expect("asset directories should be under the catalog root");
    let mut components = relative
        .iter()
        .map(|component| component.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if let Some(last) = components.last_mut() {
        if let Some(stripped) = last.strip_suffix(suffix) {
            *last = stripped.to_owned();
        }
    }

    Ok(components.join("/"))
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
    use std::time::{SystemTime, UNIX_EPOCH};

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

        let report = parse_catalog_with_warnings(&catalog_dir).expect("catalog should parse");
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
}
