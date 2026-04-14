use crate::input_filters::should_ignore_directory_entry;
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub enum ParseFilesError {
    ReadDirectory { path: PathBuf, source: io::Error },
    InvalidPath { path: PathBuf },
    InvalidUtf8Path { path: PathBuf },
}

impl std::fmt::Display for ParseFilesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDirectory { path, source } => {
                write!(
                    f,
                    "failed to read files input directory {}: {source}",
                    path.display()
                )
            }
            Self::InvalidPath { path } => {
                write!(
                    f,
                    "files input path {} is not a file or directory",
                    path.display()
                )
            }
            Self::InvalidUtf8Path { path } => {
                write!(
                    f,
                    "files input path {} is not valid UTF-8 and cannot be represented in the IR",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ParseFilesError {}

pub fn parse_files(path: &Path) -> Result<Vec<RawEntry>, ParseFilesError> {
    if path.is_file() {
        return Ok(vec![parse_single_file_entry(path)?]);
    }

    if path.is_dir() {
        let mut entries = Vec::new();
        collect_files(path, path, &mut entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        return Ok(entries);
    }

    Err(ParseFilesError::InvalidPath {
        path: path.to_path_buf(),
    })
}

fn collect_files(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<RawEntry>,
) -> Result<(), ParseFilesError> {
    let read_dir = fs::read_dir(directory).map_err(|source| ParseFilesError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseFilesError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if should_ignore_directory_entry(&path) {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|source| ParseFilesError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            collect_files(root, &path, entries)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        entries.push(parse_file_entry(root, &path)?);
    }

    Ok(())
}

fn parse_file_entry(root: &Path, file_path: &Path) -> Result<RawEntry, ParseFilesError> {
    let relative = file_path
        .strip_prefix(root)
        .expect("files should be discovered under input root");
    let relative_path = relative
        .iter()
        .map(|part| {
            part.to_str()
                .ok_or_else(|| ParseFilesError::InvalidUtf8Path {
                    path: file_path.to_path_buf(),
                })
                .map(ToOwned::to_owned)
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ParseFilesError::InvalidUtf8Path {
            path: file_path.to_path_buf(),
        })?
        .to_owned();
    let file_stem = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseFilesError::InvalidUtf8Path {
            path: file_path.to_path_buf(),
        })?
        .to_owned();
    let path_extension = file_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_owned();

    Ok(RawEntry {
        path: relative_path.clone(),
        source_path: Utf8PathBuf::from_path_buf(file_path.to_path_buf())
            .map_err(|path| ParseFilesError::InvalidUtf8Path { path })?,
        kind: EntryKind::Data,
        properties: Metadata::from([
            ("relativePath".to_string(), Value::String(relative_path)),
            ("fileName".to_string(), Value::String(file_name)),
            ("fileStem".to_string(), Value::String(file_stem)),
            ("pathExtension".to_string(), Value::String(path_extension)),
        ]),
    })
}

fn parse_single_file_entry(file_path: &Path) -> Result<RawEntry, ParseFilesError> {
    let relative_path = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ParseFilesError::InvalidUtf8Path {
            path: file_path.to_path_buf(),
        })?
        .to_owned();
    let file_name = relative_path.clone();
    let file_stem = file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseFilesError::InvalidUtf8Path {
            path: file_path.to_path_buf(),
        })?
        .to_owned();
    let path_extension = file_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_owned();

    Ok(RawEntry {
        path: relative_path.clone(),
        source_path: Utf8PathBuf::from_path_buf(file_path.to_path_buf())
            .map_err(|path| ParseFilesError::InvalidUtf8Path { path })?,
        kind: EntryKind::Data,
        properties: Metadata::from([
            ("relativePath".to_string(), Value::String(relative_path)),
            ("fileName".to_string(), Value::String(file_name)),
            ("fileStem".to_string(), Value::String(file_stem)),
            ("pathExtension".to_string(), Value::String(path_extension)),
        ]),
    })
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
    fn parses_single_file_input() {
        let temp_dir = make_temp_dir("parse-files-single");
        let file_path = temp_dir.join("Single.txt");
        fs::write(&file_path, "binary").expect("file should be written");

        let entries = parse_files(&file_path).expect("single file input should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "Single.txt");
        assert_eq!(entries[0].kind, EntryKind::Data);
        assert_eq!(
            entries[0].source_path,
            Utf8PathBuf::from_path_buf(file_path.clone()).expect("utf8 path")
        );
        assert_eq!(
            entries[0].properties["relativePath"],
            Value::String("Single.txt".to_string())
        );
        assert_eq!(
            entries[0].properties["fileName"],
            Value::String("Single.txt".to_string())
        );
        assert_eq!(
            entries[0].properties["fileStem"],
            Value::String("Single".to_string())
        );
        assert_eq!(
            entries[0].properties["pathExtension"],
            Value::String("txt".to_string())
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_recursive_directory_input() {
        let temp_dir = make_temp_dir("parse-files-recursive");
        let assets_dir = temp_dir.join("Resources").join("Assets");
        fs::create_dir_all(assets_dir.join("Nested")).expect("directories should be created");
        let first_file = assets_dir.join("zeta").with_file_name("zeta.txt");
        let second_file = assets_dir.join("Nested").join("alpha.json");
        let ignored = assets_dir.join(".DS_Store");
        let hidden_dir = assets_dir.join(".Snapshots");
        fs::write(&first_file, "one").expect("first file should be written");
        fs::write(&second_file, "two").expect("second file should be written");
        fs::create_dir_all(&hidden_dir).expect("hidden directory should be created");
        fs::write(hidden_dir.join("preview.txt"), "hidden").expect("hidden file should be written");
        fs::write(&ignored, "ignored").expect("noise file should be written");

        let entries = parse_files(&assets_dir).expect("directory input should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "Nested/alpha.json");
        assert_eq!(
            entries[0].properties["relativePath"],
            Value::String("Nested/alpha.json".to_string())
        );
        assert_eq!(
            entries[0].properties["fileName"],
            Value::String("alpha.json".to_string())
        );
        assert_eq!(
            entries[0].properties["fileStem"],
            Value::String("alpha".to_string())
        );
        assert_eq!(
            entries[0].properties["pathExtension"],
            Value::String("json".to_string())
        );
        assert_eq!(entries[1].path, "zeta.txt");
        assert_eq!(
            entries[1].properties["fileName"],
            Value::String("zeta.txt".to_string())
        );
        assert_eq!(
            entries[1].properties["pathExtension"],
            Value::String("txt".to_string())
        );
        assert!(entries.iter().all(|entry| entry.path != ".DS_Store"));
        assert!(
            entries
                .iter()
                .all(|entry| !entry.path.starts_with(".Snapshots/"))
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn hidden_only_directory_is_treated_as_empty() {
        let temp_dir = make_temp_dir("parse-files-hidden-only");
        let files_dir = temp_dir.join("Resources").join("Assets");
        let hidden_dir = files_dir.join(".Snapshots");
        fs::create_dir_all(&hidden_dir).expect("hidden directory should be created");
        fs::write(files_dir.join(".DS_Store"), "ignored").expect("dotfile should be written");
        fs::write(hidden_dir.join("preview.txt"), "hidden").expect("hidden file should be written");

        let entries = parse_files(&files_dir).expect("hidden-only directory should parse");

        assert!(
            entries.is_empty(),
            "hidden-only folders should not emit entries"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
