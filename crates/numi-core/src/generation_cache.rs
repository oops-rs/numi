use atomic_write_file::AtomicWriteFile;
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OutputRecord {
    path: String,
    digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManifestRecord {
    schema_version: u32,
    fingerprint: String,
    outputs: Vec<OutputRecord>,
}

#[derive(Debug)]
pub enum CacheError {
    CanonicalizePath {
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
                    "failed to canonicalize generation cache path {}: {source}",
                    path.display()
                )
            }
            Self::ReadFile { path, source } => {
                write!(
                    f,
                    "failed to read generation cache file {}: {source}",
                    path.display()
                )
            }
            Self::CreateDirectory { path, source } => {
                write!(
                    f,
                    "failed to create generation cache directory {}: {source}",
                    path.display()
                )
            }
            Self::CreateTemp { path, source } => {
                write!(
                    f,
                    "failed to create generation cache temp file {}: {source}",
                    path.display()
                )
            }
            Self::WriteTemp { path, source } => {
                write!(
                    f,
                    "failed to write generation cache temp file {}: {source}",
                    path.display()
                )
            }
            Self::Commit { path, source } => {
                write!(
                    f,
                    "failed to commit generation cache file {}: {source}",
                    path.display()
                )
            }
            Self::Serialize { path, source } => {
                write!(
                    f,
                    "failed to serialize generation cache file {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for CacheError {}

pub fn is_fresh(
    config_path: &Path,
    job_name: &str,
    expected_fingerprint: &str,
    output_path: &Path,
) -> Result<bool, CacheError> {
    let cache_path = cache_file_path(config_path, job_name)?;
    let bytes = match fs::read(&cache_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Ok(false),
    };

    let record: ManifestRecord = match serde_json::from_slice(&bytes) {
        Ok(record) => record,
        Err(_) => return Ok(false),
    };

    if record.schema_version != CACHE_SCHEMA_VERSION {
        return Ok(false);
    }

    if record.fingerprint != expected_fingerprint {
        return Ok(false);
    }

    if record.outputs.len() != 1 {
        return Ok(false);
    }

    let output = &record.outputs[0];
    if output.path != output_path.display().to_string() {
        return Ok(false);
    }

    if !output_path.exists() {
        return Ok(false);
    }

    Ok(output.digest == digest_file(output_path)?)
}

pub fn store(
    config_path: &Path,
    job_name: &str,
    fingerprint: &str,
    output_path: &Path,
) -> Result<(), CacheError> {
    let cache_path = cache_file_path(config_path, job_name)?;
    let parent = cache_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| CacheError::CreateDirectory {
        path: parent.to_path_buf(),
        source,
    })?;

    let payload = serde_json::to_vec(&ManifestRecord {
        schema_version: CACHE_SCHEMA_VERSION,
        fingerprint: fingerprint.to_owned(),
        outputs: vec![OutputRecord {
            path: output_path.display().to_string(),
            digest: digest_file(output_path)?,
        }],
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

fn digest_file(path: &Path) -> Result<String, CacheError> {
    let bytes = fs::read(path).map_err(|source| CacheError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(blake3_hex([bytes.as_slice()]))
}

fn cache_root() -> PathBuf {
    std::env::temp_dir()
        .join("numi-cache")
        .join(format!("generated-v{}", CACHE_SCHEMA_VERSION))
}

fn cache_file_path(config_path: &Path, job_name: &str) -> Result<PathBuf, CacheError> {
    let canonical = config_path
        .canonicalize()
        .map_err(|source| CacheError::CanonicalizePath {
            path: config_path.to_path_buf(),
            source,
        })?;
    let digest = blake3_hex([
        canonical.as_os_str().as_encoded_bytes(),
        b"\0",
        job_name.as_bytes(),
    ]);
    Ok(cache_root().join(format!("{digest}.json")))
}

pub fn blake3_hex<'a, I>(parts: I) -> String
where
    I: IntoIterator<Item = &'a [u8]>,
{
    let mut hasher = Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().to_hex().to_string()
}
