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

pub fn parse_catalog(catalog_path: &Path) -> Result<Vec<RawEntry>, ParseXcassetsError> {
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
