pub mod normalize;

pub use normalize::{
    RawEntry, normalize_flat_entries_preserve_order, normalize_scope, swift_identifier,
};

use camino::Utf8PathBuf;
use numi_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Metadata = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGraph {
    pub modules: Vec<ResourceModule>,
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: GraphMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceModule {
    pub id: String,
    pub kind: ModuleKind,
    pub name: String,
    pub entries: Vec<ResourceEntry>,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModuleKind {
    Xcassets,
    Strings,
    Xcstrings,
    Files,
    Fonts,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntry {
    pub id: String,
    pub name: String,
    pub source_path: Utf8PathBuf,
    pub swift_identifier: String,
    pub kind: EntryKind,
    pub children: Vec<ResourceEntry>,
    pub properties: Metadata,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Namespace,
    Image,
    Color,
    StringKey,
    PluralKey,
    Font,
    Data,
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMetadata {
    pub config_path: Option<Utf8PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_types_are_serializable() {
        let graph = ResourceGraph {
            modules: vec![ResourceModule {
                id: "assets".to_string(),
                kind: ModuleKind::Xcassets,
                name: "Assets".to_string(),
                entries: Vec::new(),
                metadata: Metadata::new(),
            }],
            diagnostics: vec![Diagnostic::error("boom").with_job("assets")],
            metadata: GraphMetadata {
                config_path: Some(Utf8PathBuf::from("numi.toml")),
            },
        };

        let serialized = serde_json::to_value(&graph).unwrap();
        assert_eq!(serialized["modules"][0]["name"], "Assets");
        assert_eq!(serialized["diagnostics"][0]["job"], "assets");
        assert_eq!(serialized["metadata"]["config_path"], "numi.toml");
    }
}
