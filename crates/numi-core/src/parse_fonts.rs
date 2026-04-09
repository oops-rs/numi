use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedFontEntry {
    pub path: String,
    pub source_path: Utf8PathBuf,
    pub relative_path: String,
    pub file_name: String,
    pub path_extension: String,
    pub family_name: String,
    pub style_name: String,
    pub full_name: String,
    pub post_script_name: String,
}

impl ParsedFontEntry {
    pub(crate) fn into_raw_entry(self) -> RawEntry {
        RawEntry {
            path: self.path,
            source_path: self.source_path,
            kind: EntryKind::Font,
            properties: Metadata::from([
                (
                    "relativePath".to_string(),
                    Value::String(self.relative_path),
                ),
                ("fileName".to_string(), Value::String(self.file_name)),
                (
                    "pathExtension".to_string(),
                    Value::String(self.path_extension),
                ),
                ("familyName".to_string(), Value::String(self.family_name)),
                ("styleName".to_string(), Value::String(self.style_name)),
                ("fullName".to_string(), Value::String(self.full_name)),
                (
                    "postScriptName".to_string(),
                    Value::String(self.post_script_name),
                ),
            ]),
        }
    }
}

#[derive(Debug)]
pub enum ParseFontsError {
    ReadDirectory { path: PathBuf, source: io::Error },
    ReadFile { path: PathBuf, source: io::Error },
    InvalidPath { path: PathBuf },
    InvalidUtf8Path { path: PathBuf },
    InvalidFont { path: PathBuf, detail: String },
}

impl std::fmt::Display for ParseFontsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadDirectory { path, source } => {
                write!(
                    f,
                    "failed to read fonts input directory {}: {source}",
                    path.display()
                )
            }
            Self::ReadFile { path, source } => {
                write!(f, "failed to read font file {}: {source}", path.display())
            }
            Self::InvalidPath { path } => {
                write!(
                    f,
                    "fonts input path {} is not a file or directory",
                    path.display()
                )
            }
            Self::InvalidUtf8Path { path } => {
                write!(
                    f,
                    "font path {} is not valid UTF-8 and cannot be represented in the IR",
                    path.display()
                )
            }
            Self::InvalidFont { path, detail } => {
                write!(
                    f,
                    "failed to parse font metadata from {}: {detail}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ParseFontsError {}

pub fn parse_font_entries(path: &Path) -> Result<Vec<ParsedFontEntry>, ParseFontsError> {
    if path.is_file() {
        return Ok(vec![parse_single_font(path, path)?]);
    }

    if path.is_dir() {
        let mut entries = Vec::new();
        collect_fonts(path, path, &mut entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        return Ok(entries);
    }

    Err(ParseFontsError::InvalidPath {
        path: path.to_path_buf(),
    })
}

fn collect_fonts(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<ParsedFontEntry>,
) -> Result<(), ParseFontsError> {
    let read_dir = fs::read_dir(directory).map_err(|source| ParseFontsError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseFontsError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if path.file_name().is_some_and(|name| name == ".DS_Store") {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|source| ParseFontsError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            collect_fonts(root, &path, entries)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        entries.push(parse_single_font(root, &path)?);
    }

    Ok(())
}

fn parse_single_font(root: &Path, path: &Path) -> Result<ParsedFontEntry, ParseFontsError> {
    let relative = if root.is_file() {
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ParseFontsError::InvalidUtf8Path {
                path: path.to_path_buf(),
            })?
            .to_owned()
    } else {
        path.strip_prefix(root)
            .expect("font files should be discovered under input root")
            .iter()
            .map(|part| {
                part.to_str()
                    .ok_or_else(|| ParseFontsError::InvalidUtf8Path {
                        path: path.to_path_buf(),
                    })
                    .map(ToOwned::to_owned)
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/")
    };
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ParseFontsError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();
    let path_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_owned();
    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseFontsError::InvalidUtf8Path { path })?;
    let bytes = fs::read(path).map_err(|source| ParseFontsError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let metadata = parse_name_table_metadata(&bytes, path)?;

    Ok(ParsedFontEntry {
        path: file_name.clone(),
        source_path,
        relative_path: relative,
        file_name,
        path_extension,
        family_name: metadata.family_name,
        style_name: metadata.style_name,
        full_name: metadata.full_name,
        post_script_name: metadata.post_script_name,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FontMetadata {
    family_name: String,
    style_name: String,
    full_name: String,
    post_script_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NameRecordCandidate {
    platform_id: u16,
    encoding_id: u16,
    language_id: u16,
    text: String,
}

fn parse_name_table_metadata(bytes: &[u8], path: &Path) -> Result<FontMetadata, ParseFontsError> {
    let table = find_table(bytes, b"name", path)?;
    if table.len() < 6 {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: "name table is truncated".to_string(),
        });
    }

    let count = read_u16(table, 2, path, "name table record count")? as usize;
    let string_offset = read_u16(table, 4, path, "name table string offset")? as usize;
    let records_len = 6 + count * 12;
    if table.len() < records_len {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: "name table records are truncated".to_string(),
        });
    }
    if string_offset > table.len() {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: "name table string storage starts past the table end".to_string(),
        });
    }

    let mut names = BTreeMap::<u16, Vec<NameRecordCandidate>>::new();
    for index in 0..count {
        let record_offset = 6 + index * 12;
        let platform_id = read_u16(table, record_offset, path, "name record platform id")?;
        let encoding_id = read_u16(table, record_offset + 2, path, "name record encoding id")?;
        let language_id = read_u16(table, record_offset + 4, path, "name record language id")?;
        let name_id = read_u16(table, record_offset + 6, path, "name record name id")?;
        let length = read_u16(table, record_offset + 8, path, "name record length")? as usize;
        let offset = read_u16(table, record_offset + 10, path, "name record offset")? as usize;
        let start = string_offset + offset;
        let end = start + length;
        if end > table.len() {
            return Err(ParseFontsError::InvalidFont {
                path: path.to_path_buf(),
                detail: format!("name record {name_id} points outside string storage"),
            });
        }

        let text = decode_name_string(platform_id, encoding_id, &table[start..end])?;
        names.entry(name_id).or_default().push(NameRecordCandidate {
            platform_id,
            encoding_id,
            language_id,
            text,
        });
    }

    let family_name = select_name(&names, 1).ok_or_else(|| ParseFontsError::InvalidFont {
        path: path.to_path_buf(),
        detail: "missing family name record".to_string(),
    })?;
    let style_name = select_name(&names, 2).ok_or_else(|| ParseFontsError::InvalidFont {
        path: path.to_path_buf(),
        detail: "missing style name record".to_string(),
    })?;
    let full_name = select_name(&names, 4).ok_or_else(|| ParseFontsError::InvalidFont {
        path: path.to_path_buf(),
        detail: "missing full name record".to_string(),
    })?;
    let post_script_name = select_name(&names, 6).ok_or_else(|| ParseFontsError::InvalidFont {
        path: path.to_path_buf(),
        detail: "missing PostScript name record".to_string(),
    })?;

    Ok(FontMetadata {
        family_name,
        style_name,
        full_name,
        post_script_name,
    })
}

fn find_table<'a>(
    bytes: &'a [u8],
    tag: &[u8; 4],
    path: &Path,
) -> Result<&'a [u8], ParseFontsError> {
    if bytes.len() < 12 {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: "font header is truncated".to_string(),
        });
    }

    let table_count = read_u16(bytes, 4, path, "table count")? as usize;
    let directory_len = 12 + table_count * 16;
    if bytes.len() < directory_len {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: "font table directory is truncated".to_string(),
        });
    }

    for index in 0..table_count {
        let record_offset = 12 + index * 16;
        if &bytes[record_offset..record_offset + 4] != tag {
            continue;
        }

        let offset = read_u32(bytes, record_offset + 8, path, "table offset")? as usize;
        let length = read_u32(bytes, record_offset + 12, path, "table length")? as usize;
        let end = offset + length;
        if end > bytes.len() {
            return Err(ParseFontsError::InvalidFont {
                path: path.to_path_buf(),
                detail: "table points outside font data".to_string(),
            });
        }

        return Ok(&bytes[offset..end]);
    }

    Err(ParseFontsError::InvalidFont {
        path: path.to_path_buf(),
        detail: "font is missing a name table".to_string(),
    })
}

fn decode_name_string(
    platform_id: u16,
    _encoding_id: u16,
    bytes: &[u8],
) -> Result<String, ParseFontsError> {
    match platform_id {
        0 | 3 => decode_utf16_be(bytes),
        _ => String::from_utf8(bytes.to_vec()).map_err(|error| ParseFontsError::InvalidFont {
            path: PathBuf::from("<memory>"),
            detail: format!("failed to decode font name string: {error}"),
        }),
    }
}

fn decode_utf16_be(bytes: &[u8]) -> Result<String, ParseFontsError> {
    if bytes.len() % 2 != 0 {
        return Err(ParseFontsError::InvalidFont {
            path: PathBuf::from("<memory>"),
            detail: "UTF-16BE string has an odd byte count".to_string(),
        });
    }

    let code_units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();

    String::from_utf16(&code_units).map_err(|error| ParseFontsError::InvalidFont {
        path: PathBuf::from("<memory>"),
        detail: format!("failed to decode UTF-16BE font name string: {error}"),
    })
}

fn select_name(names: &BTreeMap<u16, Vec<NameRecordCandidate>>, name_id: u16) -> Option<String> {
    let mut records = names.get(&name_id)?.clone();
    records.sort_by_key(|record| name_record_priority(name_id, record));
    records.into_iter().next().map(|record| record.text)
}

fn name_record_priority(name_id: u16, record: &NameRecordCandidate) -> (u8, u8, u8, u16, String) {
    let readability_rank = if name_id == 4 {
        if record.text.contains(' ') {
            0
        } else if record.text.contains('-') {
            1
        } else {
            2
        }
    } else {
        0
    };
    let platform_rank = match record.platform_id {
        3 => 0,
        0 => 1,
        1 => 2,
        _ => 3,
    };
    let encoding_rank = match (record.platform_id, record.encoding_id) {
        (3, 10) => 0,
        (3, 1) => 1,
        (0, _) => 2,
        _ => 3,
    };
    let language_rank = match record.language_id {
        0x0409 => 0,
        0 => 1,
        _ => 2,
    };
    (
        readability_rank,
        platform_rank,
        encoding_rank,
        language_rank,
        record.text.clone(),
    )
}

fn read_u16(bytes: &[u8], offset: usize, path: &Path, field: &str) -> Result<u16, ParseFontsError> {
    let Some(slice) = bytes.get(offset..offset + 2) else {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: format!("{field} is truncated"),
        });
    };
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize, path: &Path, field: &str) -> Result<u32, ParseFontsError> {
    let Some(slice) = bytes.get(offset..offset + 4) else {
        return Err(ParseFontsError::InvalidFont {
            path: path.to_path_buf(),
            detail: format!("{field} is truncated"),
        });
    };
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
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

    fn push_u16(buffer: &mut Vec<u8>, value: u16) {
        buffer.extend_from_slice(&value.to_be_bytes());
    }

    fn push_u32(buffer: &mut Vec<u8>, value: u32) {
        buffer.extend_from_slice(&value.to_be_bytes());
    }

    fn utf16be(value: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for unit in value.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        bytes
    }

    fn make_test_font_bytes(family: &str, style: &str, post_script_name: &str) -> Vec<u8> {
        let full_name = if style == "Regular" {
            family.to_string()
        } else {
            format!("{family} {style}")
        };
        let name_records = [
            (1_u16, utf16be(family)),
            (2_u16, utf16be(style)),
            (4_u16, utf16be(&full_name)),
            (6_u16, utf16be(post_script_name)),
        ];

        let string_offset = 6 + (name_records.len() as u16 * 12);
        let mut name_table = Vec::new();
        push_u16(&mut name_table, 0);
        push_u16(&mut name_table, name_records.len() as u16);
        push_u16(&mut name_table, string_offset);

        let mut storage = Vec::new();
        for (name_id, encoded) in &name_records {
            push_u16(&mut name_table, 3);
            push_u16(&mut name_table, 1);
            push_u16(&mut name_table, 0x0409);
            push_u16(&mut name_table, *name_id);
            push_u16(&mut name_table, encoded.len() as u16);
            push_u16(&mut name_table, storage.len() as u16);
            storage.extend_from_slice(encoded);
        }
        name_table.extend_from_slice(&storage);

        let table_offset = 12 + 16;
        let mut font = Vec::new();
        push_u32(&mut font, 0x0001_0000);
        push_u16(&mut font, 1);
        push_u16(&mut font, 16);
        push_u16(&mut font, 0);
        push_u16(&mut font, 0);
        font.extend_from_slice(b"name");
        push_u32(&mut font, 0);
        push_u32(&mut font, table_offset as u32);
        push_u32(&mut font, name_table.len() as u32);
        font.extend_from_slice(&name_table);
        while font.len() % 4 != 0 {
            font.push(0);
        }
        font
    }

    #[test]
    fn parses_font_name_table_metadata() {
        let temp_dir = make_temp_dir("parse-fonts-metadata");
        let font_path = temp_dir.join("rank.otf");
        fs::write(
            &font_path,
            make_test_font_bytes("Lettown Hills Italic", "Italic", "LettownHills-Italic"),
        )
        .expect("font should be written");

        let entries = parse_font_entries(&font_path).expect("font should parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].family_name, "Lettown Hills Italic");
        assert_eq!(entries[0].style_name, "Italic");
        assert_eq!(entries[0].full_name, "Lettown Hills Italic");
        assert_eq!(entries[0].post_script_name, "LettownHills-Italic");
        assert_eq!(entries[0].file_name, "rank.otf");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn parses_recursive_font_directory() {
        let temp_dir = make_temp_dir("parse-fonts-directory");
        let fonts_root = temp_dir.join("Resources").join("Fonts");
        fs::create_dir_all(fonts_root.join("Nested")).expect("font directory should exist");
        fs::write(
            fonts_root.join("Baloo2-Bold.ttf"),
            make_test_font_bytes("Baloo 2", "Bold", "Baloo2-Bold"),
        )
        .expect("first font should be written");
        fs::write(
            fonts_root.join("Nested").join("level.ttf"),
            make_test_font_bytes("fonteditor", "Medium", "fonteditor"),
        )
        .expect("second font should be written");

        let entries = parse_font_entries(&fonts_root).expect("font directory should parse");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "Baloo2-Bold.ttf");
        assert_eq!(entries[1].relative_path, "Nested/level.ttf");
        assert_eq!(entries[1].family_name, "fonteditor");
        assert_eq!(entries[1].style_name, "Medium");
        assert_eq!(entries[1].full_name, "fonteditor");

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}
