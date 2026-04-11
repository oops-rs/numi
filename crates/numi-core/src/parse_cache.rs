use crate::{parse_l10n::LocalizationTable, parse_xcassets::XcassetsReport};
use atomic_write_file::AtomicWriteFile;
use blake3::Hasher;
use numi_ir::RawEntry;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[cfg(test)]
use std::cell::RefCell;

const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CachedParseData {
    Xcassets(XcassetsReport),
    Strings(Vec<LocalizationTable>),
    Xcstrings(Vec<LocalizationTable>),
    Files(Vec<RawEntry>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CacheRecord {
    schema_version: u32,
    fingerprint: String,
    data: CachedParseData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CacheKind {
    Xcassets,
    Strings,
    Xcstrings,
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputSnapshot {
    root: PathBuf,
    is_file: bool,
    entries: Vec<InputSnapshotEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputSnapshotEntry {
    relative_path: PathBuf,
    kind: InputSnapshotEntryKind,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSnapshotEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FingerprintedInput {
    pub(crate) fingerprint: String,
    pub(crate) snapshot: InputSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RelevantInputEntry {
    absolute_path: PathBuf,
    relative_path: PathBuf,
    kind: InputSnapshotEntryKind,
}

#[derive(Debug)]
pub enum CacheError {
    CanonicalizePath {
        path: PathBuf,
        source: io::Error,
    },
    ReadDirectory {
        path: PathBuf,
        source: io::Error,
    },
    ReadFile {
        path: PathBuf,
        source: io::Error,
    },
    CreateDirectory {
        path: PathBuf,
        source: io::Error,
    },
    CreateTemp {
        path: PathBuf,
        source: io::Error,
    },
    WriteTemp {
        path: PathBuf,
        source: io::Error,
    },
    Commit {
        path: PathBuf,
        source: io::Error,
    },
    Serialize {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CanonicalizePath { path, source } => {
                write!(
                    f,
                    "failed to canonicalize cache input {}: {source}",
                    path.display()
                )
            }
            Self::ReadDirectory { path, source } => {
                write!(
                    f,
                    "failed to read cache input directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadFile { path, source } => {
                write!(
                    f,
                    "failed to read cache input file {}: {source}",
                    path.display()
                )
            }
            Self::CreateDirectory { path, source } => {
                write!(
                    f,
                    "failed to create cache directory {}: {source}",
                    path.display()
                )
            }
            Self::CreateTemp { path, source } => {
                write!(
                    f,
                    "failed to create cache temp file {}: {source}",
                    path.display()
                )
            }
            Self::WriteTemp { path, source } => {
                write!(
                    f,
                    "failed to write cache temp file {}: {source}",
                    path.display()
                )
            }
            Self::Commit { path, source } => {
                write!(
                    f,
                    "failed to commit cache file {}: {source}",
                    path.display()
                )
            }
            Self::Serialize { path, source } => {
                write!(
                    f,
                    "failed to serialize cache record {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for CacheError {}

#[cfg(test)]
thread_local! {
    static TEST_CACHE_ROOT_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_cache_root_override<T>(temp_root: &Path, f: impl FnOnce() -> T) -> T {
    TEST_CACHE_ROOT_OVERRIDE.with(|cell| {
        let previous = cell.replace(Some(temp_root.to_path_buf()));
        let result = f();
        cell.replace(previous);
        result
    })
}

pub fn fingerprint_input(kind: CacheKind, path: &Path) -> Result<String, CacheError> {
    Ok(fingerprint_input_with_snapshot(kind, path)?.fingerprint)
}

pub(crate) fn fingerprint_input_with_snapshot(
    kind: CacheKind,
    path: &Path,
) -> Result<FingerprintedInput, CacheError> {
    let root = canonicalize(path)?;
    let entries_to_track = relevant_entries(kind, &root)?;
    let mut hasher = Hasher::new();
    let mut entries = Vec::with_capacity(entries_to_track.len());

    hasher.update(kind.cache_key().as_bytes());
    hasher.update(b"\0");
    hasher.update(root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");

    for entry in entries_to_track {
        let metadata =
            fs::metadata(&entry.absolute_path).map_err(|source| CacheError::ReadFile {
                path: entry.absolute_path.clone(),
                source,
            })?;

        hasher.update(match entry.kind {
            InputSnapshotEntryKind::File => b"file".as_slice(),
            InputSnapshotEntryKind::Directory => b"dir".as_slice(),
        });
        hasher.update(b"\0");
        hasher.update(entry.relative_path.as_os_str().as_encoded_bytes());
        hasher.update(b"\0");
        if entry.kind == InputSnapshotEntryKind::File {
            let contents =
                fs::read(&entry.absolute_path).map_err(|source| CacheError::ReadFile {
                    path: entry.absolute_path.clone(),
                    source,
                })?;
            hasher.update(&contents);
            hasher.update(b"\0");
        }
        entries.push(InputSnapshotEntry {
            relative_path: entry.relative_path,
            kind: entry.kind,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }

    Ok(FingerprintedInput {
        fingerprint: hasher.finalize().to_hex().to_string(),
        snapshot: InputSnapshot {
            root,
            is_file: path.is_file(),
            entries,
        },
    })
}

pub fn load(kind: CacheKind, path: &Path) -> Result<Option<CachedParseData>, CacheError> {
    let cache_path = cache_file_path(kind, path)?;
    let Some(record) = read_cache_record(&cache_path) else {
        return Ok(None);
    };
    let fingerprint = fingerprint_input(kind, path)?;
    Ok(validate_record(kind, record, &fingerprint))
}

pub fn load_with_fingerprint(
    kind: CacheKind,
    path: &Path,
    fingerprint: &str,
) -> Result<Option<CachedParseData>, CacheError> {
    let cache_path = cache_file_path(kind, path)?;
    let Some(record) = read_cache_record(&cache_path) else {
        return Ok(None);
    };

    Ok(validate_record(kind, record, fingerprint))
}

#[cfg(test)]
fn cache_record_exists(kind: CacheKind, path: &Path) -> Result<bool, CacheError> {
    Ok(cache_file_path(kind, path)?.is_file())
}

#[cfg(test)]
pub(crate) fn snapshot_input(kind: CacheKind, path: &Path) -> Result<InputSnapshot, CacheError> {
    let root = canonicalize(path)?;
    build_input_snapshot(kind, &root)
}

pub(crate) fn input_matches_snapshot(
    kind: CacheKind,
    path: &Path,
    snapshot: &InputSnapshot,
) -> Result<bool, CacheError> {
    let root = canonicalize(path)?;
    if root != snapshot.root {
        return Ok(false);
    }

    Ok(build_input_snapshot(kind, &root)? == *snapshot)
}

pub fn store(
    kind: CacheKind,
    path: &Path,
    fingerprint: &str,
    data: &CachedParseData,
) -> Result<(), CacheError> {
    if !kind.matches(data) {
        return Ok(());
    }

    let cache_path = cache_file_path(kind, path)?;
    let parent = cache_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| CacheError::CreateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;

    let payload = serde_json::to_vec(&CacheRecord {
        schema_version: CACHE_SCHEMA_VERSION,
        fingerprint: fingerprint.to_owned(),
        data: data.clone(),
    })
    .map_err(|source| CacheError::Serialize {
        path: cache_path.clone(),
        source,
    })?;

    let mut atomic_file =
        AtomicWriteFile::open(&cache_path).map_err(|source| CacheError::CreateTemp {
            path: cache_path.clone(),
            source,
        })?;
    atomic_file
        .write_all(&payload)
        .and_then(|_| atomic_file.sync_all())
        .map_err(|source| CacheError::WriteTemp {
            path: cache_path.clone(),
            source,
        })?;
    atomic_file.commit().map_err(|source| CacheError::Commit {
        path: cache_path,
        source,
    })?;

    Ok(())
}

fn cache_root() -> PathBuf {
    #[cfg(test)]
    if let Some(temp_root) = TEST_CACHE_ROOT_OVERRIDE.with(|cell| cell.borrow().clone()) {
        return temp_root
            .join("numi-cache")
            .join(format!("parsed-v{}", CACHE_SCHEMA_VERSION));
    }

    std::env::temp_dir()
        .join("numi-cache")
        .join(format!("parsed-v{}", CACHE_SCHEMA_VERSION))
}

fn cache_file_path(kind: CacheKind, path: &Path) -> Result<PathBuf, CacheError> {
    let canonical = canonicalize(path)?;
    let mut hasher = Hasher::new();
    hasher.update(kind.cache_key().as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical.as_os_str().as_encoded_bytes());

    Ok(cache_root().join(format!("{}.json", hasher.finalize().to_hex())))
}

fn read_cache_record(cache_path: &Path) -> Option<CacheRecord> {
    let bytes = match fs::read(cache_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(_) => return None,
    };

    serde_json::from_slice(&bytes).ok()
}

fn validate_record(
    kind: CacheKind,
    record: CacheRecord,
    fingerprint: &str,
) -> Option<CachedParseData> {
    if record.schema_version != CACHE_SCHEMA_VERSION {
        return None;
    }

    if record.fingerprint != fingerprint {
        return None;
    }

    if !kind.matches(&record.data) {
        return None;
    }

    Some(record.data)
}

fn build_input_snapshot(kind: CacheKind, root: &Path) -> Result<InputSnapshot, CacheError> {
    let entries_to_track = relevant_entries(kind, root)?;
    let mut entries = Vec::with_capacity(entries_to_track.len());

    for entry in entries_to_track {
        let metadata =
            fs::metadata(&entry.absolute_path).map_err(|source| CacheError::ReadFile {
                path: entry.absolute_path.clone(),
                source,
            })?;
        entries.push(InputSnapshotEntry {
            relative_path: entry.relative_path,
            kind: entry.kind,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }

    Ok(InputSnapshot {
        root: root.to_path_buf(),
        is_file: root.is_file(),
        entries,
    })
}

fn relevant_entries(kind: CacheKind, path: &Path) -> Result<Vec<RelevantInputEntry>, CacheError> {
    if kind == CacheKind::Xcassets {
        return relevant_xcassets_entries(path);
    }

    if path.is_file() {
        return Ok(if is_relevant_file(kind, path) {
            vec![RelevantInputEntry {
                absolute_path: path.to_path_buf(),
                relative_path: path.file_name().map(PathBuf::from).unwrap_or_default(),
                kind: InputSnapshotEntryKind::File,
            }]
        } else {
            Vec::new()
        });
    }

    if !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_relevant_files(kind, path, &mut files)?;
    let mut entries = files
        .into_iter()
        .map(|absolute_path| RelevantInputEntry {
            relative_path: absolute_path
                .strip_prefix(path)
                .unwrap_or(&absolute_path)
                .to_path_buf(),
            absolute_path,
            kind: InputSnapshotEntryKind::File,
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(entries)
}

fn collect_relevant_files(
    kind: CacheKind,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), CacheError> {
    let read_dir = fs::read_dir(directory).map_err(|source| CacheError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| CacheError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| CacheError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            collect_relevant_files(kind, &path, files)?;
            continue;
        }

        if file_type.is_file() && is_relevant_file(kind, &path) {
            files.push(path);
        }
    }

    Ok(())
}

fn relevant_xcassets_entries(path: &Path) -> Result<Vec<RelevantInputEntry>, CacheError> {
    if path.is_file() || !path.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    collect_relevant_xcassets_entries(path, path, &mut entries)?;
    entries.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| match (left.kind, right.kind) {
                (InputSnapshotEntryKind::Directory, InputSnapshotEntryKind::File) => {
                    std::cmp::Ordering::Less
                }
                (InputSnapshotEntryKind::File, InputSnapshotEntryKind::Directory) => {
                    std::cmp::Ordering::Greater
                }
                _ => std::cmp::Ordering::Equal,
            })
    });
    Ok(entries)
}

fn collect_relevant_xcassets_entries(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<RelevantInputEntry>,
) -> Result<(), CacheError> {
    let read_dir = fs::read_dir(directory).map_err(|source| CacheError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| CacheError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| CacheError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            entries.push(RelevantInputEntry {
                absolute_path: path.clone(),
                relative_path,
                kind: InputSnapshotEntryKind::Directory,
            });
            collect_relevant_xcassets_entries(root, &path, entries)?;
            continue;
        }

        if file_type.is_file() && is_relevant_xcassets_file(root, &path) {
            entries.push(RelevantInputEntry {
                relative_path: path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                absolute_path: path,
                kind: InputSnapshotEntryKind::File,
            });
        }
    }

    Ok(())
}

fn is_relevant_xcassets_file(root: &Path, path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()) != Some("Contents.json") {
        return false;
    }

    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(relative_parent) = parent.strip_prefix(root) else {
        return false;
    };
    if relative_parent.as_os_str().is_empty() {
        return false;
    }

    let folder_name = relative_parent
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let extension = Path::new(folder_name)
        .extension()
        .and_then(|extension| extension.to_str());

    extension.is_none() || extension == Some("spriteatlas")
}

fn is_relevant_file(kind: CacheKind, path: &Path) -> bool {
    match kind {
        CacheKind::Xcassets => path.is_file(),
        CacheKind::Strings => path.extension().and_then(|ext| ext.to_str()) == Some("strings"),
        CacheKind::Xcstrings => path.extension().and_then(|ext| ext.to_str()) == Some("xcstrings"),
        CacheKind::Files => {
            // Keep cache relevance aligned with parse_files: only `.DS_Store` is excluded today.
            path.is_file() && path.file_name().is_none_or(|name| name != ".DS_Store")
        }
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, CacheError> {
    fs::canonicalize(path).map_err(|source| CacheError::CanonicalizePath {
        path: path.to_path_buf(),
        source,
    })
}

impl CacheKind {
    fn cache_key(self) -> &'static str {
        match self {
            Self::Xcassets => "xcassets",
            Self::Strings => "strings",
            Self::Xcstrings => "xcstrings",
            Self::Files => "files",
        }
    }

    fn matches(self, data: &CachedParseData) -> bool {
        matches!(
            (self, data),
            (Self::Xcassets, CachedParseData::Xcassets(_))
                | (Self::Strings, CachedParseData::Strings(_))
                | (Self::Xcstrings, CachedParseData::Xcstrings(_))
                | (Self::Files, CachedParseData::Files(_))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use numi_diagnostics::{Diagnostic, Severity};
    use numi_ir::{EntryKind, Metadata, RawEntry};
    use serde_json::Value;
    use std::{
        fs,
        path::PathBuf,
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

    fn sample_xcassets_report(
        source_root: &std::path::Path,
    ) -> crate::parse_xcassets::XcassetsReport {
        crate::parse_xcassets::XcassetsReport {
            entries: vec![RawEntry {
                path: "Brand/Primary".to_string(),
                source_path: Utf8PathBuf::from_path_buf(source_root.join("Assets.xcassets"))
                    .expect("utf8 path"),
                kind: EntryKind::Color,
                properties: Metadata::from([(
                    "assetName".to_string(),
                    Value::String("Brand/Primary".to_string()),
                )]),
            }],
            warnings: vec![Diagnostic {
                severity: Severity::Warning,
                message: "warning".to_string(),
                hint: Some("hint".to_string()),
                job: Some("assets".to_string()),
                path: Some(source_root.join("Assets.xcassets")),
            }],
        }
    }

    #[test]
    fn fingerprint_changes_when_matching_file_contents_change() {
        let temp_dir = make_temp_dir("parse-cache-fingerprint-change");
        let strings_path = temp_dir.join("Localizable.strings");
        fs::write(&strings_path, "\"title\" = \"Before\";\n").expect("strings file should exist");

        let before = fingerprint_input(CacheKind::Strings, &strings_path)
            .expect("initial fingerprint should succeed");

        fs::write(&strings_path, "\"title\" = \"After\";\n").expect("strings file should mutate");

        let after = fingerprint_input(CacheKind::Strings, &strings_path)
            .expect("updated fingerprint should succeed");

        assert_ne!(before, after);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn fingerprint_ignores_non_matching_files_for_strings_directory() {
        let temp_dir = make_temp_dir("parse-cache-fingerprint-ignore");
        let l10n_dir = temp_dir.join("Localization");
        fs::create_dir_all(&l10n_dir).expect("l10n dir should exist");
        fs::write(
            l10n_dir.join("Localizable.strings"),
            "\"title\" = \"Hello\";\n",
        )
        .expect("strings file should exist");
        fs::write(l10n_dir.join("preview.png"), "not-part-of-strings-cache")
            .expect("noise file should exist");

        let before =
            fingerprint_input(CacheKind::Strings, &l10n_dir).expect("fingerprint should succeed");

        fs::write(l10n_dir.join("preview.png"), "mutated-noise").expect("noise file should mutate");

        let after = fingerprint_input(CacheKind::Strings, &l10n_dir)
            .expect("fingerprint should still succeed");

        assert_eq!(before, after);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn cache_record_round_trips_xcassets_payload() {
        let temp_dir = make_temp_dir("parse-cache-round-trip");
        let catalog_path = temp_dir.join("Assets.xcassets");
        fs::create_dir_all(&catalog_path).expect("catalog should exist");
        fs::write(
            catalog_path.join("Contents.json"),
            "{\"info\":{\"author\":\"xcode\",\"version\":1}}",
        )
        .expect("catalog contents should exist");

        let fingerprint = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should succeed");
        let report = sample_xcassets_report(&temp_dir);

        store(
            CacheKind::Xcassets,
            &catalog_path,
            &fingerprint,
            &CachedParseData::Xcassets(report.clone()),
        )
        .expect("cache store should succeed");

        let loaded = load(CacheKind::Xcassets, &catalog_path).expect("cache load should succeed");

        assert_eq!(loaded, Some(CachedParseData::Xcassets(report)));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn cache_record_exists_only_after_store() {
        let temp_dir = make_temp_dir("parse-cache-exists");
        let catalog_path = temp_dir.join("Assets.xcassets");
        fs::create_dir_all(&catalog_path).expect("catalog should exist");
        fs::write(
            catalog_path.join("Contents.json"),
            "{\"info\":{\"author\":\"xcode\",\"version\":1}}",
        )
        .expect("catalog contents should exist");

        assert!(
            !cache_record_exists(CacheKind::Xcassets, &catalog_path)
                .expect("missing cache record check should succeed")
        );

        let fingerprint = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should succeed");
        let report = sample_xcassets_report(&temp_dir);

        store(
            CacheKind::Xcassets,
            &catalog_path,
            &fingerprint,
            &CachedParseData::Xcassets(report),
        )
        .expect("cache store should succeed");

        assert!(
            cache_record_exists(CacheKind::Xcassets, &catalog_path)
                .expect("stored cache record check should succeed")
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn input_snapshot_detects_when_relevant_file_changes() {
        let temp_dir = make_temp_dir("parse-cache-input-snapshot");
        let strings_path = temp_dir.join("Localizable.strings");
        fs::write(&strings_path, "\"title\" = \"Before\";\n").expect("strings file should exist");

        let snapshot = snapshot_input(CacheKind::Strings, &strings_path)
            .expect("initial snapshot should succeed");

        assert!(
            input_matches_snapshot(CacheKind::Strings, &strings_path, &snapshot)
                .expect("fresh snapshot should still match")
        );

        fs::write(&strings_path, "\"title\" = \"After\";\n").expect("strings file should mutate");

        assert!(
            !input_matches_snapshot(CacheKind::Strings, &strings_path, &snapshot)
                .expect("mutated snapshot check should succeed")
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcassets_fingerprint_ignores_image_binary_changes() {
        let temp_dir = make_temp_dir("parse-cache-xcassets-ignore-image-binaries");
        let catalog_path = temp_dir.join("Assets.xcassets");
        let imageset_path = catalog_path.join("Logo.imageset");
        fs::create_dir_all(&imageset_path).expect("imageset dir should exist");
        fs::write(
            imageset_path.join("Contents.json"),
            "{\"images\":[{\"filename\":\"logo.png\"}],\"info\":{\"author\":\"xcode\",\"version\":1}}",
        )
        .expect("imageset contents should exist");
        fs::write(imageset_path.join("logo.png"), "before").expect("image file should exist");

        let before = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should succeed");

        fs::write(imageset_path.join("logo.png"), "after").expect("image file should mutate");

        let after = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should still succeed");

        assert_eq!(before, after);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcassets_snapshot_ignores_image_binary_changes() {
        let temp_dir = make_temp_dir("parse-cache-xcassets-snapshot-ignore-image-binaries");
        let catalog_path = temp_dir.join("Assets.xcassets");
        let imageset_path = catalog_path.join("Logo.imageset");
        fs::create_dir_all(&imageset_path).expect("imageset dir should exist");
        fs::write(
            imageset_path.join("Contents.json"),
            "{\"images\":[{\"filename\":\"logo.png\"}],\"info\":{\"author\":\"xcode\",\"version\":1}}",
        )
        .expect("imageset contents should exist");
        fs::write(imageset_path.join("logo.png"), "before").expect("image file should exist");

        let snapshot =
            snapshot_input(CacheKind::Xcassets, &catalog_path).expect("snapshot should succeed");

        fs::write(imageset_path.join("logo.png"), "after").expect("image file should mutate");

        assert!(
            input_matches_snapshot(CacheKind::Xcassets, &catalog_path, &snapshot)
                .expect("snapshot comparison should succeed"),
            "image payload changes should not invalidate xcassets snapshot relevance"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcassets_fingerprint_changes_when_asset_folder_changes() {
        let temp_dir = make_temp_dir("parse-cache-xcassets-folder-change");
        let catalog_path = temp_dir.join("Assets.xcassets");
        fs::create_dir_all(catalog_path.join("Logo.imageset")).expect("imageset dir should exist");

        let before = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should succeed");

        fs::create_dir_all(catalog_path.join("Badge.imageset"))
            .expect("new imageset dir should exist");

        let after = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should still succeed");

        assert_ne!(before, after);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcassets_fingerprint_changes_when_namespace_contents_change() {
        let temp_dir = make_temp_dir("parse-cache-xcassets-namespace-change");
        let catalog_path = temp_dir.join("Assets.xcassets");
        let group_path = catalog_path.join("Icons");
        fs::create_dir_all(group_path.join("Logo.imageset"))
            .expect("nested imageset dir should exist");
        fs::write(
            group_path.join("Contents.json"),
            "{\"properties\":{\"provides-namespace\":false}}",
        )
        .expect("group contents should exist");

        let before = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should succeed");

        fs::write(
            group_path.join("Contents.json"),
            "{\"properties\":{\"provides-namespace\":true}}",
        )
        .expect("group contents should mutate");

        let after = fingerprint_input(CacheKind::Xcassets, &catalog_path)
            .expect("fingerprint should still succeed");

        assert_ne!(before, after);

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
