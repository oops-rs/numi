use atomic_write_file::AtomicWriteFile;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Created,
    Updated,
    Unchanged,
    Skipped,
}

#[derive(Debug)]
pub enum OutputError {
    CreateDirectory { path: PathBuf, source: io::Error },
    ReadExisting { path: PathBuf, source: io::Error },
    CreateTemp { path: PathBuf, source: io::Error },
    WriteTemp { path: PathBuf, source: io::Error },
    Commit { path: PathBuf, source: io::Error },
    Cleanup { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateDirectory { path, source } => {
                write!(
                    f,
                    "failed to create output directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadExisting { path, source } => {
                write!(
                    f,
                    "failed to read existing output {}: {source}",
                    path.display()
                )
            }
            Self::CreateTemp { path, source } => {
                write!(
                    f,
                    "failed to create temp output {}: {source}",
                    path.display()
                )
            }
            Self::WriteTemp { path, source } => {
                write!(
                    f,
                    "failed to write temp output {}: {source}",
                    path.display()
                )
            }
            Self::Commit { path, source } => write!(
                f,
                "failed to commit atomic output {}: {source}",
                path.display()
            ),
            Self::Cleanup { path, source } => {
                write!(
                    f,
                    "failed to clean up atomic output {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for OutputError {}

pub fn write_if_changed_atomic(path: &Path, contents: &str) -> Result<WriteOutcome, OutputError> {
    if path.exists() {
        let existing = fs::read_to_string(path).map_err(|source| OutputError::ReadExisting {
            path: path.to_path_buf(),
            source,
        })?;
        if existing == contents {
            return Ok(WriteOutcome::Unchanged);
        }
    }

    let parent = parent_dir(path);
    fs::create_dir_all(parent).map_err(|source| OutputError::CreateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;

    let mut atomic_file =
        AtomicWriteFile::open(path).map_err(|source| OutputError::CreateTemp {
            path: path.to_path_buf(),
            source,
        })?;
    let destination_preexisted = path.exists();

    atomic_file
        .write_all(contents.as_bytes())
        .and_then(|_| atomic_file.sync_all())
        .map_err(|source| OutputError::WriteTemp {
            path: path.to_path_buf(),
            source,
        })?;

    atomic_file.commit().map_err(|source| OutputError::Commit {
        path: path.to_path_buf(),
        source,
    })?;

    let outcome = if destination_preexisted {
        WriteOutcome::Updated
    } else {
        WriteOutcome::Created
    };

    Ok(outcome)
}

pub fn output_is_stale(path: &Path, contents: &str) -> Result<bool, OutputError> {
    if !path.exists() {
        return Ok(true);
    }

    let existing = fs::read_to_string(path).map_err(|source| OutputError::ReadExisting {
        path: path.to_path_buf(),
        source,
    })?;

    Ok(existing != contents)
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn make_temp_dir(test_name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "numi-output-{test_name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("temp dir should be created");
        root
    }

    #[test]
    fn skips_rewrites_when_contents_are_unchanged() {
        let root = make_temp_dir("noop");
        let path = root.join("Generated/Assets.swift");

        let first = write_if_changed_atomic(&path, "import SwiftUI\n").expect("first write");
        let first_modified = fs::metadata(&path)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should be readable");

        thread::sleep(Duration::from_millis(20));

        let second = write_if_changed_atomic(&path, "import SwiftUI\n").expect("second write");
        let second_modified = fs::metadata(&path)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should be readable");

        assert_eq!(first, WriteOutcome::Created);
        assert_eq!(second, WriteOutcome::Unchanged);
        assert_eq!(first_modified, second_modified);

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }

    #[test]
    fn replaces_existing_output_when_contents_change() {
        let root = make_temp_dir("update");
        let path = root.join("Generated/Assets.swift");

        let first = write_if_changed_atomic(&path, "import SwiftUI\n")
            .expect("initial write should succeed");
        let first_modified = fs::metadata(&path)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should be readable");

        thread::sleep(Duration::from_millis(20));

        let second =
            write_if_changed_atomic(&path, "import UIKit\n").expect("updated write should succeed");
        let second_modified = fs::metadata(&path)
            .expect("metadata should exist")
            .modified()
            .expect("mtime should be readable");
        let contents = fs::read_to_string(&path).expect("updated contents should exist");

        assert_eq!(first, WriteOutcome::Created);
        assert_eq!(second, WriteOutcome::Updated);
        assert_eq!(contents, "import UIKit\n");
        assert!(second_modified >= first_modified);

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }

    #[test]
    fn update_path_does_not_leave_sidecar_files() {
        let root = make_temp_dir("sidecars");
        let generated_dir = root.join("Generated");
        let path = generated_dir.join("Assets.swift");

        write_if_changed_atomic(&path, "import SwiftUI\n").expect("initial write should succeed");
        write_if_changed_atomic(&path, "import UIKit\n").expect("update write should succeed");

        let entries = fs::read_dir(&generated_dir)
            .expect("generated dir should exist")
            .map(|entry| entry.expect("dir entry should exist").file_name())
            .collect::<Vec<_>>();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].to_string_lossy(), "Assets.swift");

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }

    #[test]
    fn reports_missing_and_different_outputs_as_stale() {
        let root = make_temp_dir("stale");
        let path = root.join("Generated/Assets.swift");

        assert!(
            output_is_stale(&path, "import SwiftUI\n").expect("missing output should be stale")
        );

        write_if_changed_atomic(&path, "import SwiftUI\n").expect("initial write should succeed");
        assert!(
            !output_is_stale(&path, "import SwiftUI\n").expect("matching output should be fresh")
        );
        assert!(
            output_is_stale(&path, "import UIKit\n").expect("different output should be stale")
        );

        fs::remove_dir_all(root).expect("temp dir should be removed");
    }
}
