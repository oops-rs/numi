use camino::Utf8PathBuf;
use langcodec::{
    error::Error as LangcodecError,
    formats::{
        strings::Format as StringsFormat,
        xcstrings::{
            Format as XcstringsFormat, Item as XcstringsItem, Localization as XcstringsLocalization,
        },
    },
    traits::Parser,
};
use numi_diagnostics::{Diagnostic, Severity};
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizationTable {
    pub table_name: String,
    pub source_path: Utf8PathBuf,
    pub module_kind: ModuleKind,
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug)]
pub enum ParseL10nError {
    ReadDirectory { path: PathBuf, source: io::Error },
    ReadFile { path: PathBuf, source: io::Error },
    InvalidPath { path: PathBuf },
    InvalidUtf8Path { path: PathBuf },
    ParseFile { path: PathBuf, message: String },
}

impl std::fmt::Display for ParseL10nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDirectory { path, source } => {
                write!(
                    f,
                    "failed to read localization directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadFile { path, source } => {
                write!(
                    f,
                    "failed to read localization file {}: {source}",
                    path.display()
                )
            }
            Self::InvalidPath { path } => write!(
                f,
                "localization input {} must be a `.strings` or `.xcstrings` file or a directory containing supported localization files",
                path.display()
            ),
            Self::InvalidUtf8Path { path } => write!(
                f,
                "localization path {} is not valid UTF-8 and cannot be represented in the IR",
                path.display()
            ),
            Self::ParseFile { path, message } => {
                write!(
                    f,
                    "failed to parse localization file {}: {message}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ParseL10nError {}

pub fn parse_strings(path: &Path) -> Result<Vec<LocalizationTable>, ParseL10nError> {
    parse_localization(path, "strings", parse_strings_file)
}

pub fn parse_xcstrings(path: &Path) -> Result<Vec<LocalizationTable>, ParseL10nError> {
    parse_localization(path, "xcstrings", parse_xcstrings_file)
}

fn parse_localization(
    path: &Path,
    extension: &str,
    parse_file: fn(&Path) -> Result<LocalizationTable, ParseL10nError>,
) -> Result<Vec<LocalizationTable>, ParseL10nError> {
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) != Some(extension) {
            return Err(ParseL10nError::InvalidPath {
                path: path.to_path_buf(),
            });
        }
        return parse_file(path).map(|table| vec![table]);
    }

    if path.is_dir() {
        let mut files = Vec::new();
        collect_localization_files(path, extension, &mut files)?;
        files.sort();

        return files.into_iter().map(|file| parse_file(&file)).collect();
    }

    Err(ParseL10nError::InvalidPath {
        path: path.to_path_buf(),
    })
}

fn collect_localization_files(
    directory: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
) -> Result<(), ParseL10nError> {
    let read_dir = fs::read_dir(directory).map_err(|source| ParseL10nError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseL10nError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| ParseL10nError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            collect_localization_files(&path, extension, files)?;
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
            files.push(path);
        }
    }

    Ok(())
}

fn parse_strings_file(path: &Path) -> Result<LocalizationTable, ParseL10nError> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("strings") {
        return Err(ParseL10nError::InvalidPath {
            path: path.to_path_buf(),
        });
    }

    let strings =
        StringsFormat::read_from(path).map_err(|error| map_langcodec_error(path, error))?;
    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();

    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;
    let mut entries = Vec::with_capacity(strings.pairs.len());
    for pair in strings.pairs {
        let key = decode_strings_translation_escapes(path, &pair.key)?;
        let translation = decode_strings_translation_escapes(path, &pair.value)?;
        entries.push(RawEntry {
            path: key.clone(),
            source_path: source_path.clone(),
            kind: EntryKind::StringKey,
            properties: Metadata::from([
                ("key".to_string(), Value::String(key)),
                ("translation".to_string(), Value::String(translation)),
            ]),
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(LocalizationTable {
        table_name,
        source_path,
        module_kind: ModuleKind::Strings,
        entries,
        warnings: Vec::new(),
    })
}

fn parse_xcstrings_file(path: &Path) -> Result<LocalizationTable, ParseL10nError> {
    if path.extension().and_then(|ext| ext.to_str()) != Some("xcstrings") {
        return Err(ParseL10nError::InvalidPath {
            path: path.to_path_buf(),
        });
    }

    let xcstrings =
        XcstringsFormat::read_from(path).map_err(|error| map_langcodec_error(path, error))?;
    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();

    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;
    let adapter_metadata =
        parse_xcstrings_adapter_metadata(path, xcstrings.source_language.as_str())?;

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for (key, item) in xcstrings.strings {
        let Some(localization) = select_localization(&item, xcstrings.source_language.as_str())
        else {
            warnings.push(xcstrings_warning(
                path,
                &key,
                "does not contain a supported string unit",
            ));
            continue;
        };

        if let Some(reason) = adapter_metadata
            .get(&key)
            .and_then(|metadata| metadata.variation_reason)
            .or_else(|| unsupported_variation_reason(localization))
        {
            warnings.push(xcstrings_warning(path, &key, reason));
            continue;
        }

        let Some(string_unit) = localization.string_unit.as_ref() else {
            warnings.push(xcstrings_warning(
                path,
                &key,
                "does not contain a string unit",
            ));
            continue;
        };
        let translation = string_unit.value.clone();

        let mut properties = Metadata::from([
            ("key".to_string(), Value::String(key.clone())),
            ("translation".to_string(), Value::String(translation)),
        ]);

        if let Some(metadata) = adapter_metadata.get(&key) {
            if let Some(placeholders) = build_placeholder_metadata(&metadata.placeholder_specs) {
                properties.insert("placeholders".to_string(), Value::Array(placeholders));
            }
        }

        entries.push(RawEntry {
            path: key.clone(),
            source_path: source_path.clone(),
            kind: EntryKind::StringKey,
            properties,
        });
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    warnings.sort_by(|left, right| left.message.cmp(&right.message));

    Ok(LocalizationTable {
        table_name,
        source_path,
        module_kind: ModuleKind::Xcstrings,
        entries,
        warnings,
    })
}

fn map_langcodec_error(path: &Path, error: LangcodecError) -> ParseL10nError {
    match error {
        LangcodecError::Io(source) => ParseL10nError::ReadFile {
            path: path.to_path_buf(),
            source,
        },
        other => ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: other.to_string(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PlaceholderMetadataSpec {
    format_specifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct XcstringsEntryMetadata {
    placeholder_specs: BTreeMap<String, PlaceholderMetadataSpec>,
    variation_reason: Option<&'static str>,
}

fn parse_xcstrings_adapter_metadata(
    path: &Path,
    source_language: &str,
) -> Result<BTreeMap<String, XcstringsEntryMetadata>, ParseL10nError> {
    let bytes = fs::read(path).map_err(|source| ParseL10nError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let contents = decode_strings_bytes(&bytes, path)?;
    let root: Value =
        serde_json::from_str(&contents).map_err(|error| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: format!("invalid JSON: {error}"),
        })?;

    let Some(strings) = root.get("strings").and_then(Value::as_object) else {
        return Ok(BTreeMap::new());
    };

    let mut metadata_by_key = BTreeMap::new();
    for (key, record) in strings {
        let Some(localizations) = record.get("localizations").and_then(Value::as_object) else {
            continue;
        };
        let Some(localization) = select_localization_value(localizations, source_language) else {
            continue;
        };

        let mut metadata = XcstringsEntryMetadata::default();
        metadata.variation_reason = unsupported_variation_reason_from_value(localization);

        if let Some(substitutions) = localization.get("substitutions").and_then(Value::as_object) {
            for (name, substitution) in substitutions {
                let spec = PlaceholderMetadataSpec {
                    format_specifier: substitution
                        .get("formatSpecifier")
                        .and_then(Value::as_str)
                        .map(ToString::to_string),
                };
                metadata.placeholder_specs.insert(name.clone(), spec);

                if metadata.variation_reason.is_none() {
                    metadata.variation_reason = substitution
                        .get("variations")
                        .and_then(unsupported_variation_reason_from_variations_value);
                }
            }
        }

        if !metadata.placeholder_specs.is_empty() || metadata.variation_reason.is_some() {
            metadata_by_key.insert(key.clone(), metadata);
        }
    }

    Ok(metadata_by_key)
}

fn select_localization<'a>(
    item: &'a XcstringsItem,
    source_language: &str,
) -> Option<&'a XcstringsLocalization> {
    item.localizations.get(source_language).or_else(|| {
        item.localizations
            .iter()
            .min_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, localization)| localization)
    })
}

fn select_localization_value<'a>(
    localizations: &'a serde_json::Map<String, Value>,
    source_language: &str,
) -> Option<&'a Value> {
    localizations.get(source_language).or_else(|| {
        localizations
            .iter()
            .min_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, localization)| localization)
    })
}

fn decode_strings_bytes(bytes: &[u8], path: &Path) -> Result<String, ParseL10nError> {
    if let Some(stripped) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(stripped.to_vec()).map_err(|error| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: format!("invalid UTF-8 after BOM: {error}"),
        });
    }

    if let Some(stripped) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16_units(stripped, path, u16::from_le_bytes);
    }

    if let Some(stripped) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16_units(stripped, path, u16::from_be_bytes);
    }

    String::from_utf8(bytes.to_vec()).map_err(|error| ParseL10nError::ParseFile {
        path: path.to_path_buf(),
        message: format!("invalid UTF-8: {error}"),
    })
}

fn decode_utf16_units(
    bytes: &[u8],
    path: &Path,
    decode_unit: fn([u8; 2]) -> u16,
) -> Result<String, ParseL10nError> {
    if bytes.len() % 2 != 0 {
        return Err(ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: "UTF-16 file has an odd number of bytes".to_string(),
        });
    }

    let units = bytes
        .chunks_exact(2)
        .map(|chunk| decode_unit([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();

    String::from_utf16(&units).map_err(|error| ParseL10nError::ParseFile {
        path: path.to_path_buf(),
        message: format!("invalid UTF-16: {error}"),
    })
}

fn decode_strings_translation_escapes(path: &Path, value: &str) -> Result<String, ParseL10nError> {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let escaped = chars.next().ok_or_else(|| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: "incomplete escape sequence".to_string(),
        })?;

        let decoded = match escaped {
            '"' => '"',
            '\'' => '\'',
            '\\' => '\\',
            '/' => '/',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'U' => {
                let mut hex = String::with_capacity(4);
                for _ in 0..4 {
                    let digit = chars.next().ok_or_else(|| ParseL10nError::ParseFile {
                        path: path.to_path_buf(),
                        message: "unexpected end of input".to_string(),
                    })?;
                    if !digit.is_ascii_hexdigit() {
                        return Err(ParseL10nError::ParseFile {
                            path: path.to_path_buf(),
                            message: "invalid unicode escape".to_string(),
                        });
                    }
                    hex.push(digit);
                }
                let scalar =
                    u32::from_str_radix(&hex, 16).map_err(|_| ParseL10nError::ParseFile {
                        path: path.to_path_buf(),
                        message: "invalid unicode escape".to_string(),
                    })?;
                char::from_u32(scalar).ok_or_else(|| ParseL10nError::ParseFile {
                    path: path.to_path_buf(),
                    message: "invalid unicode scalar".to_string(),
                })?
            }
            other => {
                return Err(ParseL10nError::ParseFile {
                    path: path.to_path_buf(),
                    message: format!("unsupported escape `\\{other}`"),
                });
            }
        };
        output.push(decoded);
    }

    Ok(output)
}

fn build_placeholder_metadata(
    substitutions: &BTreeMap<String, PlaceholderMetadataSpec>,
) -> Option<Vec<Value>> {
    if substitutions.is_empty() {
        return None;
    }

    let mut placeholders = Vec::with_capacity(substitutions.len());

    for (name, placeholder_spec) in substitutions {
        let mut placeholder = Metadata::new();
        placeholder.insert("name".to_string(), Value::String(name.clone()));

        if let Some(format_specifier) = placeholder_spec.format_specifier.as_ref() {
            placeholder.insert(
                "format".to_string(),
                Value::String(format_specifier.clone()),
            );

            if let Some(swift_type) = infer_swift_type(format_specifier) {
                placeholder.insert(
                    "swiftType".to_string(),
                    Value::String(swift_type.to_string()),
                );
            }
        }

        placeholders.push(Value::Object(placeholder.into_iter().collect()));
    }

    Some(placeholders)
}

fn infer_swift_type(format_specifier: &str) -> Option<&'static str> {
    let kind = format_specifier
        .strip_prefix('%')
        .unwrap_or(format_specifier)
        .chars()
        .rev()
        .find(|ch| ch.is_ascii_alphabetic() || *ch == '@')?;

    match kind {
        '@' => Some("String"),
        'd' | 'i' | 'u' | 'o' | 'x' | 'X' => Some("Int"),
        'f' | 'F' | 'e' | 'E' | 'g' | 'G' | 'a' | 'A' => Some("Double"),
        _ => None,
    }
}

fn xcstrings_warning(path: &Path, key: &str, reason: &str) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        message: format!("skipping xcstrings key `{key}`: {reason}"),
        hint: None,
        job: None,
        path: Some(path.to_path_buf()),
    }
}

fn unsupported_variation_reason(localization: &XcstringsLocalization) -> Option<&'static str> {
    if localization
        .variations
        .as_ref()
        .and_then(|variations| variations.plural.as_ref())
        .is_some_and(|plural| !plural.is_empty())
    {
        Some("unsupported plural variations")
    } else {
        None
    }
}

fn unsupported_variation_reason_from_value(localization: &Value) -> Option<&'static str> {
    let localization = localization.as_object()?;
    if let Some(reason) = localization
        .get("variations")
        .and_then(unsupported_variation_reason_from_variations_value)
    {
        return Some(reason);
    }

    let substitutions = localization.get("substitutions")?.as_object()?;
    for substitution in substitutions.values() {
        if let Some(reason) = substitution
            .get("variations")
            .and_then(unsupported_variation_reason_from_variations_value)
        {
            return Some(reason);
        }
    }

    None
}

fn unsupported_variation_reason_from_variations_value(variations: &Value) -> Option<&'static str> {
    match variations {
        Value::Object(object) => {
            if object.is_empty() {
                return None;
            }

            if object
                .get("plural")
                .is_some_and(|value| !is_empty_variation_value(value))
            {
                return Some("unsupported plural variations");
            }

            if object
                .get("device")
                .is_some_and(|value| !is_empty_variation_value(value))
            {
                return Some("unsupported device-specific variations");
            }

            if object
                .values()
                .any(|value| !is_empty_variation_value(value))
            {
                return Some("unsupported variation tree");
            }

            None
        }
        Value::Array(array) => {
            if array.is_empty() {
                None
            } else {
                Some("unsupported variation tree")
            }
        }
        Value::Null => None,
        _ => Some("unsupported variation tree"),
    }
}

fn is_empty_variation_value(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.values().all(is_empty_variation_value),
        Value::Array(array) => array.iter().all(is_empty_variation_value),
        Value::Null => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ScopedTempDir {
        path: PathBuf,
    }

    impl ScopedTempDir {
        fn new(test_name: &str) -> Self {
            let path = make_temp_dir(test_name);
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for ScopedTempDir {
        fn drop(&mut self) {
            let result = fs::remove_dir_all(&self.path);
            if std::thread::panicking() {
                let _ = result;
            } else {
                result.expect("temp dir should be removed");
            }
        }
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
    fn parses_strings_files_from_directory() {
        let temp_dir = make_temp_dir("parse-strings");
        let localization_dir = temp_dir.join("Resources/Localization");
        fs::create_dir_all(&localization_dir).expect("localization dir should exist");
        let strings_path = localization_dir.join("Localizable.strings");
        fs::write(
            &strings_path,
            "\"profile.title\" = \"Profile\";\n\"settings.logout\" = \"Log Out\";\n",
        )
        .expect("strings file should be written");

        let tables = parse_strings(&localization_dir).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].table_name, "Localizable");
        assert_eq!(
            tables[0].source_path,
            Utf8PathBuf::from_path_buf(strings_path.clone()).expect("utf8 path")
        );
        assert_eq!(tables[0].entries.len(), 2);
        assert_eq!(tables[0].entries[0].path, "profile.title");
        assert_eq!(tables[0].entries[0].kind, EntryKind::StringKey);
        assert_eq!(
            tables[0].entries[0].properties["translation"],
            Value::String("Profile".to_string())
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_comments_and_escapes() {
        let temp_dir = make_temp_dir("parse-comments");
        let strings_path = temp_dir.join("Localizable.strings");
        fs::write(
            &strings_path,
            "// line comment\n/* block comment */\n\"escaped\" = \"Quote: \\\"hi\\\"\\nDone\";\n",
        )
        .expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].entries[0].properties["translation"],
            Value::String("Quote: \"hi\"\nDone".to_string())
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_escaped_apostrophe_in_strings() {
        let temp_dir = ScopedTempDir::new("parse-escaped-apostrophe");
        let strings_path = temp_dir.path().join("Localizable.strings");
        fs::write(
            &strings_path,
            "\"invite.key\" = \"Can\\'t accept the invitation\";\n",
        )
        .expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(
            tables[0].entries[0].properties["translation"],
            Value::String("Can't accept the invitation".to_string())
        );
    }

    #[test]
    fn parses_escaped_apostrophe_in_strings_key() {
        let temp_dir = ScopedTempDir::new("parse-escaped-key-apostrophe");
        let strings_path = temp_dir.path().join("Localizable.strings");
        fs::write(&strings_path, "\"invite.can\\'t\" = \"Invite\";\n")
            .expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries[0].path, "invite.can't");
        assert_eq!(
            tables[0].entries[0].properties["key"],
            Value::String("invite.can't".to_string())
        );
    }

    #[test]
    fn parses_utf8_with_bom() {
        let temp_dir = make_temp_dir("parse-utf8-bom");
        let strings_path = temp_dir.join("Localizable.strings");
        fs::write(
            &strings_path,
            b"\xEF\xBB\xBF\"profile.title\" = \"Profile\";\n",
        )
        .expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries[0].properties["key"], "profile.title");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_utf16_with_bom() {
        let temp_dir = make_temp_dir("parse-utf16-bom");
        let strings_path = temp_dir.join("Localizable.strings");
        let utf16: Vec<u16> = "\"profile.title\" = \"Profile\";\n"
            .encode_utf16()
            .collect();
        let mut bytes = vec![0xFF, 0xFE];
        for unit in utf16 {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        fs::write(&strings_path, bytes).expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries[0].properties["translation"], "Profile");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_utf16_big_endian_with_bom() {
        let temp_dir = make_temp_dir("parse-utf16be-bom");
        let strings_path = temp_dir.join("Localizable.strings");
        let utf16: Vec<u16> = "\"profile.title\" = \"Profile\";\n"
            .encode_utf16()
            .collect();
        let mut bytes = vec![0xFE, 0xFF];
        for unit in utf16 {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        fs::write(&strings_path, bytes).expect("strings file should be written");

        let tables = parse_strings(&strings_path).expect("strings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries[0].properties["translation"], "Profile");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_xcstrings_plain_string_and_placeholders() {
        let temp_dir = make_temp_dir("parse-xcstrings-plain");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "greeting.message": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Hello %#@name@, you have %#@count@ messages"
          },
          "substitutions": {
            "count": {
              "formatSpecifier": "lld"
            },
            "name": {
              "formatSpecifier": "@"
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].table_name, "Localizable");
        assert_eq!(tables[0].module_kind, ModuleKind::Xcstrings);
        assert_eq!(tables[0].entries.len(), 1);
        assert_eq!(tables[0].entries[0].path, "greeting.message");
        assert_eq!(
            tables[0].entries[0].properties["key"],
            Value::String("greeting.message".to_string())
        );
        assert_eq!(
            tables[0].entries[0].properties["translation"],
            Value::String("Hello %#@name@, you have %#@count@ messages".to_string())
        );
        assert_eq!(
            tables[0].entries[0].properties["placeholders"],
            json!([
                {"name": "count", "format": "lld", "swiftType": "Int"},
                {"name": "name", "format": "@", "swiftType": "String"}
            ])
        );
        assert!(tables[0].warnings.is_empty());

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn keeps_xcstrings_placeholder_name_without_format_specifier() {
        let temp_dir = make_temp_dir("parse-xcstrings-placeholder-name-only");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "welcome.message": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Hello %#@name@"
          },
          "substitutions": {
            "name": {}
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries.len(), 1);
        assert_eq!(
            tables[0].entries[0].properties["placeholders"],
            json!([{"name": "name"}])
        );
        assert!(tables[0].warnings.is_empty());

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn skips_xcstrings_plural_variations_with_warning() {
        let temp_dir = make_temp_dir("parse-xcstrings-plural");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "things.label": {
      "localizations": {
        "en": {
          "variations": {
            "plural": {
              "one": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%lld thing"
                }
              },
              "other": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%lld things"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].module_kind, ModuleKind::Xcstrings);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].warnings.len(), 1);
        assert_eq!(tables[0].warnings[0].severity, Severity::Warning);
        assert!(tables[0].warnings[0].message.contains("things.label"));
        assert!(tables[0].warnings[0]
            .message
            .contains("unsupported plural variations"));
        assert_eq!(tables[0].warnings[0].path, Some(xcstrings_path.clone()));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn skips_xcstrings_device_variations_with_warning() {
        let temp_dir = make_temp_dir("parse-xcstrings-device");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "title.label": {
      "localizations": {
        "en": {
          "variations": {
            "device": {
              "iphone": {
                "stringUnit": {
                  "state": "translated",
                  "value": "Title"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].warnings.len(), 1);
        assert!(tables[0].warnings[0]
            .message
            .contains("unsupported device-specific variations"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn skips_xcstrings_unknown_variation_tree_with_warning() {
        let temp_dir = make_temp_dir("parse-xcstrings-unknown-variation");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "title.label": {
      "localizations": {
        "en": {
          "variations": {
            "customDimension": {
              "foo": {
                "stringUnit": {
                  "state": "translated",
                  "value": "Title"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].warnings.len(), 1);
        assert!(tables[0].warnings[0]
            .message
            .contains("unsupported variation tree"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcstrings_warnings_are_sorted_for_multiple_skipped_keys() {
        let temp_dir = make_temp_dir("parse-xcstrings-warning-order");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "z.key": {
      "localizations": {
        "en": {}
      }
    },
    "a.key": {
      "localizations": {
        "en": {
          "variations": {
            "plural": {
              "one": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%lld item"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].warnings.len(), 2);
        assert!(tables[0].warnings[0].message.contains("a.key"));
        assert!(tables[0].warnings[1].message.contains("z.key"));

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn keeps_xcstrings_plain_string_when_other_localization_has_plural_variations() {
        let temp_dir = make_temp_dir("parse-xcstrings-other-loc-variations");
        let xcstrings_path = temp_dir.join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "greeting.message": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Hello world"
          }
        },
        "de": {
          "variations": {
            "plural": {
              "one": {
                "stringUnit": {
                  "state": "translated",
                  "value": "Hallo Welt"
                }
              },
              "other": {
                "stringUnit": {
                  "state": "translated",
                  "value": "Hallo Welten"
                }
              }
            }
          }
        }
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].entries.len(), 1);
        assert_eq!(tables[0].entries[0].path, "greeting.message");
        assert_eq!(
            tables[0].entries[0].properties["translation"],
            Value::String("Hello world".to_string())
        );
        assert!(tables[0].warnings.is_empty());

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn xcstrings_with_lv_lld_missing_string_unit_becomes_warning() {
        let temp_dir = ScopedTempDir::new("parse-xcstrings-lv-format");
        let xcstrings_path = temp_dir.path().join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "Lv.%lld": {
      "localizations": {
        "en": {}
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let result = parse_xcstrings(&xcstrings_path);

        assert!(
            result.is_ok(),
            "{}",
            match result {
                Err(error) => format!("expected warnings, got parse error: {error}"),
                Ok(_) => "".to_string(),
            }
        );

        let tables = result.expect("parse should now warn instead of error");
        assert_eq!(tables.len(), 1);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].table_name, "Localizable");
        assert_eq!(tables[0].module_kind, ModuleKind::Xcstrings);
        assert_eq!(tables[0].warnings.len(), 1);
        let warning = tables[0]
            .warnings
            .iter()
            .find(|warning| {
                warning.message.contains("`Lv.%lld`") && warning.message.contains("string unit")
            })
            .expect("expected warning for Lv.%lld");
        assert_eq!(warning.path, Some(xcstrings_path.clone()));
        assert_eq!(warning.severity, Severity::Warning);
    }

    #[test]
    fn xcstrings_with_empty_key_missing_string_unit_becomes_warning() {
        let temp_dir = ScopedTempDir::new("parse-xcstrings-empty-key");
        let xcstrings_path = temp_dir.path().join("Localizable.xcstrings");
        fs::write(
            &xcstrings_path,
            r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "": {
      "localizations": {
        "en": {}
      }
    }
  }
}
"#,
        )
        .expect("xcstrings file should be written");

        let result = parse_xcstrings(&xcstrings_path);

        assert!(
            result.is_ok(),
            "{}",
            match result {
                Err(error) => format!("expected warnings, got parse error: {error}"),
                Ok(_) => "".to_string(),
            }
        );

        let tables = result.expect("parse should now warn instead of error");
        assert_eq!(tables.len(), 1);
        assert!(tables[0].entries.is_empty());
        assert_eq!(tables[0].table_name, "Localizable");
        assert_eq!(tables[0].module_kind, ModuleKind::Xcstrings);
        assert_eq!(tables[0].warnings.len(), 1);
        let warning = tables[0]
            .warnings
            .iter()
            .find(|warning| warning.message.contains("string unit"))
            .expect("expected warning for empty key");
        assert_eq!(warning.path, Some(xcstrings_path.clone()));
        assert_eq!(warning.severity, Severity::Warning);
    }
}
