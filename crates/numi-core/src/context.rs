use numi_ir::{EntryKind, Metadata, ModuleKind, ResourceEntry, ResourceModule, swift_identifier};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetTemplateContext {
    pub job: JobTemplateContext,
    pub access_level: String,
    pub bundle: BundleTemplateContext,
    pub modules: Vec<ModuleTemplateContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobTemplateContext {
    pub name: String,
    #[serde(rename = "swiftIdentifier")]
    pub swift_identifier: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleTemplateContext {
    pub mode: String,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleTemplateContext {
    pub kind: String,
    pub name: String,
    pub properties: Metadata,
    pub entries: Vec<EntryTemplateContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryTemplateContext {
    pub name: String,
    #[serde(rename = "swiftIdentifier")]
    pub swift_identifier: String,
    pub kind: String,
    pub children: Vec<EntryTemplateContext>,
    pub properties: Metadata,
    pub metadata: Metadata,
}

#[derive(Debug)]
pub struct ContextError {
    message: String,
}

impl ContextError {
    fn unsupported_module(kind: &ModuleKind) -> Self {
        Self {
            message: format!("unsupported module kind `{kind:?}`"),
        }
    }

    fn unsupported_entry(kind: EntryKind, entry_id: &str) -> Self {
        Self {
            message: format!("unsupported entry kind `{kind:?}` for `{entry_id}`"),
        }
    }
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ContextError {}

impl AssetTemplateContext {
    pub fn new(
        job_name: &str,
        job_output: &str,
        access_level: &str,
        bundle_mode: &str,
        bundle_identifier: Option<&str>,
        modules: &[ResourceModule],
    ) -> Result<Self, ContextError> {
        Ok(Self {
            job: JobTemplateContext {
                name: job_name.to_owned(),
                swift_identifier: swift_identifier(job_name),
                output: job_output.to_owned(),
            },
            access_level: access_level.to_owned(),
            bundle: BundleTemplateContext {
                mode: bundle_mode.to_owned(),
                identifier: bundle_identifier.map(ToOwned::to_owned),
            },
            modules: modules
                .iter()
                .map(ModuleTemplateContext::from_resource_module)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl ModuleTemplateContext {
    fn from_resource_module(module: &ResourceModule) -> Result<Self, ContextError> {
        Ok(Self {
            kind: match &module.kind {
                ModuleKind::Xcassets => "xcassets".to_string(),
                ModuleKind::Files => "files".to_string(),
                ModuleKind::Fonts => "fonts".to_string(),
                ModuleKind::Strings => "strings".to_string(),
                ModuleKind::Xcstrings => "xcstrings".to_string(),
                other => return Err(ContextError::unsupported_module(other)),
            },
            name: if module.name.is_empty() {
                swift_identifier(&module.id)
            } else {
                module.name.clone()
            },
            properties: module.metadata.clone(),
            entries: module
                .entries
                .iter()
                .map(EntryTemplateContext::from_resource_entry)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl EntryTemplateContext {
    fn from_resource_entry(entry: &ResourceEntry) -> Result<Self, ContextError> {
        let kind = match entry.kind {
            EntryKind::Namespace => "namespace".to_string(),
            EntryKind::Image => "image".to_string(),
            EntryKind::Color => "color".to_string(),
            EntryKind::StringKey => "string".to_string(),
            EntryKind::Font => "font".to_string(),
            EntryKind::Data => "data".to_string(),
            other => return Err(ContextError::unsupported_entry(other, &entry.id)),
        };

        Ok(Self {
            name: entry.name.clone(),
            swift_identifier: entry.swift_identifier.clone(),
            kind,
            children: entry
                .children
                .iter()
                .map(Self::from_resource_entry)
                .collect::<Result<Vec<_>, _>>()?,
            properties: entry.properties.clone(),
            metadata: entry.metadata.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use serde_json::json;

    #[test]
    fn builds_stable_template_surface_for_assets() {
        let module = ResourceModule {
            id: "assets".to_string(),
            kind: ModuleKind::Xcassets,
            name: "Assets".to_string(),
            entries: vec![
                ResourceEntry {
                    id: "Brand".to_string(),
                    name: "Brand".to_string(),
                    source_path: Utf8PathBuf::from("fixture"),
                    swift_identifier: "Brand".to_string(),
                    kind: EntryKind::Color,
                    children: Vec::new(),
                    properties: Metadata::from([("assetName".to_string(), json!("Brand"))]),
                    metadata: Metadata::new(),
                },
                ResourceEntry {
                    id: "Icons".to_string(),
                    name: "Icons".to_string(),
                    source_path: Utf8PathBuf::from("virtual"),
                    swift_identifier: "Icons".to_string(),
                    kind: EntryKind::Namespace,
                    children: vec![ResourceEntry {
                        id: "Icons/add".to_string(),
                        name: "add".to_string(),
                        source_path: Utf8PathBuf::from("fixture"),
                        swift_identifier: "Add".to_string(),
                        kind: EntryKind::Image,
                        children: Vec::new(),
                        properties: Metadata::from([("assetName".to_string(), json!("Icons/add"))]),
                        metadata: Metadata::new(),
                    }],
                    properties: Metadata::new(),
                    metadata: Metadata::new(),
                },
            ],
            metadata: Metadata::new(),
        };

        let context = AssetTemplateContext::new(
            "assets",
            "Generated/Assets.swift",
            "internal",
            "module",
            None,
            &[module],
        )
        .expect("context should build");
        let serialized = serde_json::to_value(&context).expect("context should serialize");

        assert_eq!(serialized["job"]["name"], "assets");
        assert_eq!(serialized["job"]["swiftIdentifier"], "Assets");
        assert_eq!(serialized["job"]["output"], "Generated/Assets.swift");
        assert_eq!(serialized["access_level"], "internal");
        assert_eq!(serialized["bundle"]["mode"], "module");
        assert_eq!(serialized["bundle"]["identifier"], serde_json::Value::Null);
        assert_eq!(serialized["modules"][0]["kind"], "xcassets");
        assert_eq!(serialized["modules"][0]["name"], "Assets");
        assert_eq!(serialized["modules"][0]["properties"], json!({}));
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["assetName"],
            "Brand"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][1]["children"][0]["properties"]["assetName"],
            "Icons/add"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][1]["children"][0]["swiftIdentifier"],
            "Add"
        );
    }

    #[test]
    fn builds_stable_template_surface_for_localization() {
        let module = ResourceModule {
            id: "Localizable".to_string(),
            kind: ModuleKind::Strings,
            name: "Localizable".to_string(),
            entries: vec![ResourceEntry {
                id: "profile.title".to_string(),
                name: "profile.title".to_string(),
                source_path: Utf8PathBuf::from("fixture"),
                swift_identifier: "ProfileTitle".to_string(),
                kind: EntryKind::StringKey,
                children: Vec::new(),
                properties: Metadata::from([
                    ("key".to_string(), json!("profile.title")),
                    ("translation".to_string(), json!("Profile")),
                ]),
                metadata: Metadata::new(),
            }],
            metadata: Metadata::from([("tableName".to_string(), json!("Localizable"))]),
        };

        let context = AssetTemplateContext::new(
            "l10n",
            "Generated/L10n.swift",
            "internal",
            "module",
            None,
            &[module],
        )
        .expect("context should build");
        let serialized = serde_json::to_value(&context).expect("context should serialize");

        assert_eq!(serialized["job"]["name"], "l10n");
        assert_eq!(serialized["job"]["swiftIdentifier"], "L10n");
        assert_eq!(serialized["modules"][0]["kind"], "strings");
        assert_eq!(serialized["modules"][0]["name"], "Localizable");
        assert_eq!(
            serialized["modules"][0]["properties"]["tableName"],
            "Localizable"
        );
        assert_eq!(serialized["modules"][0]["entries"][0]["kind"], "string");
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["key"],
            "profile.title"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["translation"],
            "Profile"
        );
    }

    #[test]
    fn builds_stable_template_surface_for_xcstrings_localization() {
        let module = ResourceModule {
            id: "Localizable".to_string(),
            kind: ModuleKind::Xcstrings,
            name: "Localizable".to_string(),
            entries: vec![
                ResourceEntry {
                    id: "greeting.message".to_string(),
                    name: "greeting.message".to_string(),
                    source_path: Utf8PathBuf::from("fixture"),
                    swift_identifier: "GreetingMessage".to_string(),
                    kind: EntryKind::StringKey,
                    children: Vec::new(),
                    properties: Metadata::from([
                        ("key".to_string(), json!("greeting.message")),
                        (
                            "translation".to_string(),
                            json!("Hello %#@name@, you have %#@count@ messages"),
                        ),
                        ("status".to_string(), json!("translated")),
                        ("comment".to_string(), json!("Greeting")),
                        (
                            "placeholders".to_string(),
                            json!([
                                {"name": "count", "format": "lld", "swiftType": "Int"},
                                {"name": "name", "format": "@", "swiftType": "String"}
                            ]),
                        ),
                    ]),
                    metadata: Metadata::new(),
                },
                ResourceEntry {
                    id: "profile.title".to_string(),
                    name: "profile.title".to_string(),
                    source_path: Utf8PathBuf::from("fixture"),
                    swift_identifier: "ProfileTitle".to_string(),
                    kind: EntryKind::StringKey,
                    children: Vec::new(),
                    properties: Metadata::from([
                        ("key".to_string(), json!("profile.title")),
                        ("translation".to_string(), json!("Profile")),
                    ]),
                    metadata: Metadata::new(),
                },
            ],
            metadata: Metadata::from([("tableName".to_string(), json!("Localizable"))]),
        };

        let context = AssetTemplateContext::new(
            "l10n",
            "Generated/L10n.swift",
            "internal",
            "module",
            None,
            &[module],
        )
        .expect("context should build");
        let serialized = serde_json::to_value(&context).expect("context should serialize");

        assert_eq!(serialized["modules"][0]["kind"], "xcstrings");
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["status"],
            "translated"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["comment"],
            "Greeting"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["placeholders"],
            json!([
                {"name": "count", "format": "lld", "swiftType": "Int"},
                {"name": "name", "format": "@", "swiftType": "String"}
            ])
        );
        assert!(
            !serialized["modules"][0]["entries"][1]["properties"]
                .as_object()
                .expect("entry properties should be an object")
                .contains_key("placeholders")
        );
    }

    #[test]
    fn builds_stable_template_surface_for_files_data_entries() {
        let module = ResourceModule {
            id: "Files".to_string(),
            kind: ModuleKind::Files,
            name: "Files".to_string(),
            entries: vec![ResourceEntry {
                id: "readme.md".to_string(),
                name: "readme.md".to_string(),
                source_path: Utf8PathBuf::from("fixture"),
                swift_identifier: "ReadmeMd".to_string(),
                kind: EntryKind::Data,
                children: Vec::new(),
                properties: Metadata::from([
                    ("relativePath".to_string(), json!("readme.md")),
                    ("fileName".to_string(), json!("readme.md")),
                    ("pathExtension".to_string(), json!("md")),
                ]),
                metadata: Metadata::new(),
            }],
            metadata: Metadata::new(),
        };

        let context = AssetTemplateContext::new(
            "files",
            "Generated/Files.swift",
            "internal",
            "module",
            None,
            &[module],
        )
        .expect("context should build");
        let serialized = serde_json::to_value(&context).expect("context should serialize");

        assert_eq!(serialized["modules"][0]["kind"], "files");
        assert_eq!(serialized["modules"][0]["name"], "Files");
        assert_eq!(
            serialized["modules"][0]["entries"][0]["swiftIdentifier"],
            "ReadmeMd"
        );
        assert_eq!(serialized["modules"][0]["entries"][0]["kind"], "data");
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["relativePath"],
            "readme.md"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["fileName"],
            "readme.md"
        );
        assert_eq!(
            serialized["modules"][0]["entries"][0]["properties"]["pathExtension"],
            "md"
        );
    }
}
