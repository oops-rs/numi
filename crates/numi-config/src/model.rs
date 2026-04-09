use serde::{Deserialize, Serialize};

pub const ACCESS_LEVEL_VALUES: &[&str] = &["internal", "public"];
pub const BUNDLE_MODE_VALUES: &[&str] = &["module", "main", "custom"];
pub const INPUT_KIND_VALUES: &[&str] = &["xcassets", "strings", "xcstrings", "files"];
pub const DEFAULT_ACCESS_LEVEL: &str = "internal";
pub const DEFAULT_BUNDLE_MODE: &str = "module";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default, skip_serializing_if = "DefaultsConfig::is_empty")]
    pub defaults: DefaultsConfig,
    pub jobs: Vec<JobConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultsConfig {
    pub access_level: Option<String>,
    #[serde(default, skip_serializing_if = "BundleConfig::is_empty")]
    pub bundle: BundleConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BundleConfig {
    pub mode: Option<String>,
    pub identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobConfig {
    pub name: String,
    pub output: String,
    #[serde(default)]
    pub access_level: Option<String>,
    #[serde(default, skip_serializing_if = "BundleConfig::is_empty")]
    pub bundle: BundleConfig,
    pub inputs: Vec<InputConfig>,
    pub template: TemplateConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinTemplateConfig>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinTemplateConfig {
    pub swift: Option<String>,
}

impl DefaultsConfig {
    pub fn is_empty(&self) -> bool {
        self.access_level.is_none() && self.bundle.is_empty()
    }
}

impl BundleConfig {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.identifier.is_none()
    }
}
