use camino::Utf8PathBuf;
use numi_diagnostics::{Diagnostic, Severity};
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
use serde::Deserialize;
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

    let bytes = fs::read(path).map_err(|source| ParseL10nError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let contents = decode_strings_bytes(&bytes, path)?;
    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();

    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;
    let mut entries = StringsParser::new(&contents, path).parse_entries()?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    for entry in &mut entries {
        entry.source_path = source_path.clone();
    }

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

    let bytes = fs::read(path).map_err(|source| ParseL10nError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let contents = decode_strings_bytes(&bytes, path)?;
    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();

    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;
    let catalog: XcstringsCatalog =
        serde_json::from_str(&contents).map_err(|error| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: format!("invalid JSON: {error}"),
        })?;

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for (key, record) in catalog.strings {
        let Some(localization) = record.selected_localization(catalog.source_language.as_deref())
        else {
            return Err(ParseL10nError::ParseFile {
                path: path.to_path_buf(),
                message: format!("xcstrings key `{key}` does not contain a supported string unit"),
            });
        };

        if let Some(reason) = localization.unsupported_variation_reason() {
            warnings.push(xcstrings_warning(path, &key, reason));
            continue;
        }

        let translation = localization
            .string_unit
            .as_ref()
            .ok_or_else(|| ParseL10nError::ParseFile {
                path: path.to_path_buf(),
                message: format!("xcstrings key `{key}` does not contain a string unit"),
            })?
            .value
            .clone();

        let mut properties = Metadata::from([
            ("key".to_string(), Value::String(key.clone())),
            ("translation".to_string(), Value::String(translation)),
        ]);

        if let Some(placeholders) = build_placeholder_metadata(&localization.substitutions) {
            properties.insert("placeholders".to_string(), Value::Array(placeholders));
        }

        entries.push(RawEntry {
            path: key.clone(),
            source_path: source_path.clone(),
            kind: EntryKind::StringKey,
            properties,
        });
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(LocalizationTable {
        table_name,
        source_path,
        module_kind: ModuleKind::Xcstrings,
        entries,
        warnings,
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

fn build_placeholder_metadata(
    substitutions: &BTreeMap<String, XcstringsSubstitution>,
) -> Option<Vec<Value>> {
    if substitutions.is_empty() {
        return None;
    }

    let mut placeholders = Vec::with_capacity(substitutions.len());

    for (name, substitution) in substitutions {
        let mut placeholder = Metadata::new();
        placeholder.insert("name".to_string(), Value::String(name.clone()));

        if let Some(format_specifier) = substitution.format_specifier.as_ref() {
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

struct StringsParser<'a> {
    input: &'a str,
    offset: usize,
    path: &'a Path,
}

impl<'a> StringsParser<'a> {
    fn new(input: &'a str, path: &'a Path) -> Self {
        Self {
            input,
            offset: 0,
            path,
        }
    }

    fn parse_entries(&mut self) -> Result<Vec<RawEntry>, ParseL10nError> {
        let mut entries = Vec::new();

        loop {
            self.skip_ws_and_comments()?;
            if self.is_eof() {
                break;
            }

            let key = self.parse_quoted_string()?;
            self.skip_ws_and_comments()?;
            self.expect_char('=')?;
            self.skip_ws_and_comments()?;
            let translation = self.parse_quoted_string()?;
            self.skip_ws_and_comments()?;
            self.expect_char(';')?;

            entries.push(RawEntry {
                path: key.clone(),
                source_path: Utf8PathBuf::from("fixture"),
                kind: EntryKind::StringKey,
                properties: Metadata::from([
                    ("key".to_string(), Value::String(key)),
                    ("translation".to_string(), Value::String(translation)),
                ]),
            });
        }

        Ok(entries)
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), ParseL10nError> {
        loop {
            let remaining = self.remaining();
            let trimmed = remaining.trim_start_matches(char::is_whitespace);
            self.offset += remaining.len() - trimmed.len();

            if self.remaining().starts_with("//") {
                while let Some(ch) = self.peek_char() {
                    self.advance_char(ch);
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }

            if self.remaining().starts_with("/*") {
                self.offset += 2;
                if let Some(end) = self.remaining().find("*/") {
                    self.offset += end + 2;
                    continue;
                }

                return Err(self.error("unterminated block comment"));
            }

            break;
        }

        Ok(())
    }

    fn parse_quoted_string(&mut self) -> Result<String, ParseL10nError> {
        self.expect_char('"')?;
        let mut value = String::new();

        while let Some(ch) = self.peek_char() {
            self.advance_char(ch);

            match ch {
                '"' => return Ok(value),
                '\\' => value.push(self.parse_escape()?),
                _ => value.push(ch),
            }
        }

        Err(self.error("unterminated string literal"))
    }

    fn parse_escape(&mut self) -> Result<char, ParseL10nError> {
        let escaped = self
            .peek_char()
            .ok_or_else(|| self.error("incomplete escape sequence"))?;
        self.advance_char(escaped);

        match escaped {
            '"' => Ok('"'),
            '\\' => Ok('\\'),
            '/' => Ok('/'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            't' => Ok('\t'),
            'U' => {
                let hex = self.take_exact(4)?;
                let value = u32::from_str_radix(&hex, 16)
                    .map_err(|_| self.error("invalid unicode escape"))?;
                char::from_u32(value).ok_or_else(|| self.error("invalid unicode scalar"))
            }
            other => Err(self.error(format!("unsupported escape `\\{other}`"))),
        }
    }

    fn take_exact(&mut self, len: usize) -> Result<String, ParseL10nError> {
        if self.remaining().len() < len {
            return Err(self.error("unexpected end of input"));
        }

        let segment = &self.remaining()[..len];
        if !segment.is_ascii() {
            return Err(self.error("unicode escape must contain ASCII hex digits"));
        }
        self.offset += len;
        Ok(segment.to_owned())
    }

    fn expect_char(&mut self, expected: char) -> Result<(), ParseL10nError> {
        let actual = self
            .peek_char()
            .ok_or_else(|| self.error(format!("expected `{expected}`")))?;

        if actual == expected {
            self.advance_char(actual);
            Ok(())
        } else {
            Err(self.error(format!("expected `{expected}`, found `{actual}`")))
        }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.offset..]
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance_char(&mut self, ch: char) {
        self.offset += ch.len_utf8();
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.input.len()
    }

    fn error(&self, message: impl Into<String>) -> ParseL10nError {
        ParseL10nError::ParseFile {
            path: self.path.to_path_buf(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct XcstringsCatalog {
    #[serde(rename = "sourceLanguage", default)]
    source_language: Option<String>,
    #[serde(default)]
    strings: BTreeMap<String, XcstringsRecord>,
}

#[derive(Debug, Deserialize)]
struct XcstringsRecord {
    #[serde(default)]
    localizations: BTreeMap<String, XcstringsLocalization>,
}

#[derive(Debug, Deserialize)]
struct XcstringsLocalization {
    #[serde(rename = "stringUnit", default)]
    string_unit: Option<XcstringsStringUnit>,
    #[serde(default)]
    substitutions: BTreeMap<String, XcstringsSubstitution>,
    #[serde(default)]
    variations: Option<XcstringsVariations>,
    #[serde(flatten)]
    other: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct XcstringsStringUnit {
    value: String,
}

#[derive(Debug, Deserialize)]
struct XcstringsSubstitution {
    #[serde(rename = "formatSpecifier", default)]
    format_specifier: Option<String>,
    #[serde(default)]
    variations: Option<XcstringsVariations>,
    #[serde(flatten)]
    other: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct XcstringsVariations {
    #[serde(default)]
    plural: BTreeMap<String, Value>,
    #[serde(default)]
    device: BTreeMap<String, Value>,
    #[serde(flatten)]
    other: BTreeMap<String, Value>,
}

impl XcstringsRecord {
    fn selected_localization(
        &self,
        source_language: Option<&str>,
    ) -> Option<&XcstringsLocalization> {
        if let Some(source_language) = source_language {
            if let Some(localization) = self.localizations.get(source_language) {
                return Some(localization);
            }
        }

        self.localizations.values().next()
    }
}

impl XcstringsLocalization {
    fn unsupported_variation_reason(&self) -> Option<&'static str> {
        if self
            .variations
            .as_ref()
            .is_some_and(|variations| !variations.is_empty())
        {
            return Some(self.variations.as_ref().unwrap().reason());
        }

        for substitution in self.substitutions.values() {
            if let Some(reason) = substitution.unsupported_variation_reason() {
                return Some(reason);
            }
        }

        if self
            .other
            .get("variations")
            .is_some_and(|value| !is_empty_variation_value(value))
        {
            return Some("unsupported variation tree");
        }

        None
    }
}

impl XcstringsSubstitution {
    fn unsupported_variation_reason(&self) -> Option<&'static str> {
        if self
            .variations
            .as_ref()
            .is_some_and(|variations| !variations.is_empty())
        {
            return Some(self.variations.as_ref().unwrap().reason());
        }

        if self
            .other
            .get("variations")
            .is_some_and(|value| !is_empty_variation_value(value))
        {
            return Some("unsupported variation tree");
        }

        None
    }
}

impl XcstringsVariations {
    fn is_empty(&self) -> bool {
        self.plural.is_empty() && self.device.is_empty() && self.other.is_empty()
    }

    fn reason(&self) -> &'static str {
        if !self.plural.is_empty() {
            "unsupported plural variations"
        } else if !self.device.is_empty() {
            "unsupported device-specific variations"
        } else {
            "unsupported variation tree"
        }
    }
}

fn is_empty_variation_value(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.is_empty(),
        Value::Array(array) => array.is_empty(),
        Value::Null => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
        assert!(
            tables[0].warnings[0]
                .message
                .contains("unsupported plural variations")
        );
        assert_eq!(tables[0].warnings[0].path, Some(xcstrings_path.clone()));

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
}
