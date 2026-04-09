use std::{
    fs,
    path::{Path, PathBuf},
};

pub const CONFIG_FILE_NAME: &str = "numi.toml";

#[derive(Debug)]
pub enum DiscoveryError {
    ExplicitPathNotFound(PathBuf),
    NotFound {
        start_dir: PathBuf,
    },
    Ambiguous {
        root: PathBuf,
        matches: Vec<PathBuf>,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExplicitPathNotFound(path) => {
                write!(f, "config file not found: {}", path.display())
            }
            Self::NotFound { start_dir } => write!(
                f,
                "No configuration file found from {}\n\nPlease specify one with:\n  numi config locate --config <path>",
                start_dir.display()
            ),
            Self::Ambiguous { root, matches } => {
                writeln!(
                    f,
                    "Multiple configuration files found under {}:",
                    root.display()
                )?;
                for path in matches {
                    writeln!(f, "  - {}", path.display())?;
                }
                write!(
                    f,
                    "\nPlease specify one with:\n  numi config locate --config <path>"
                )
            }
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

impl From<std::io::Error> for DiscoveryError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn discover_config(
    start_dir: &Path,
    explicit_path: Option<&Path>,
) -> Result<PathBuf, DiscoveryError> {
    if let Some(explicit_path) = explicit_path {
        let resolved = resolve_explicit_path(start_dir, explicit_path)?;
        if resolved.is_file() {
            return Ok(resolved);
        }
        return Err(DiscoveryError::ExplicitPathNotFound(
            explicit_path.to_path_buf(),
        ));
    }

    let canonical_start = start_dir.canonicalize()?;

    if let Some(path) = find_in_ancestors(&canonical_start) {
        return Ok(path);
    }

    let mut matches = Vec::new();
    collect_descendants(&canonical_start, &canonical_start, &mut matches)?;
    matches.sort();

    match matches.len() {
        0 => Err(DiscoveryError::NotFound {
            start_dir: canonical_start,
        }),
        1 => Ok(canonical_start.join(&matches[0])),
        _ => Err(DiscoveryError::Ambiguous {
            root: canonical_start,
            matches,
        }),
    }
}

fn resolve_explicit_path(
    start_dir: &Path,
    explicit_path: &Path,
) -> Result<PathBuf, DiscoveryError> {
    let candidate = if explicit_path.is_absolute() {
        explicit_path.to_path_buf()
    } else {
        start_dir.join(explicit_path)
    };

    if candidate.is_file() {
        Ok(candidate.canonicalize()?)
    } else {
        Err(DiscoveryError::ExplicitPathNotFound(candidate))
    }
}

fn find_in_ancestors(start_dir: &Path) -> Option<PathBuf> {
    for directory in start_dir.ancestors() {
        let candidate = directory.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn collect_descendants(
    root: &Path,
    current_dir: &Path,
    matches: &mut Vec<PathBuf>,
) -> Result<(), DiscoveryError> {
    let mut entries: Vec<_> = fs::read_dir(current_dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_descendants(root, &path, matches)?;
        } else if file_type.is_file() && entry.file_name() == CONFIG_FILE_NAME {
            let relative = path
                .strip_prefix(root)
                .expect("descendant path should stay under root")
                .to_path_buf();
            matches.push(relative);
        }
    }

    Ok(())
}
