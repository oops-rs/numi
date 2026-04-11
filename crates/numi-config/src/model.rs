use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub const ACCESS_LEVEL_VALUES: &[&str] = &["internal", "public"];
pub const BUNDLE_MODE_VALUES: &[&str] = &["module", "main", "custom"];
pub const INPUT_KIND_VALUES: &[&str] = &["xcassets", "strings", "xcstrings", "files", "fonts"];
pub const BUILTIN_TEMPLATE_LANGUAGES: &[&str] = &["swift", "objc"];
pub const SWIFT_BUILTIN_TEMPLATE_NAMES: &[&str] = &["swiftui-assets", "l10n", "files"];
pub const OBJC_BUILTIN_TEMPLATE_NAMES: &[&str] = &["assets", "l10n", "files"];
pub const DEFAULT_ACCESS_LEVEL: &str = "internal";
pub const DEFAULT_BUNDLE_MODE: &str = "module";
pub const DEFAULT_INCREMENTAL: bool = true;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub version: u32,
    pub defaults: DefaultsConfig,
    pub jobs: Vec<JobConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    pub version: u32,
    #[serde(default, skip_serializing_if = "DefaultsConfig::is_empty")]
    pub defaults: DefaultsConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub jobs: BTreeMap<String, RawJobConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawJobConfig {
    pub output: String,
    #[serde(default)]
    pub access_level: Option<String>,
    #[serde(default)]
    pub incremental: Option<bool>,
    #[serde(default, skip_serializing_if = "BundleConfig::is_empty")]
    pub bundle: BundleConfig,
    pub inputs: Vec<InputConfig>,
    #[serde(default, skip_serializing_if = "TemplateConfig::is_empty")]
    pub template: TemplateConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultsConfig {
    pub access_level: Option<String>,
    pub incremental: Option<bool>,
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
    #[serde(default)]
    pub incremental: Option<bool>,
    #[serde(default, skip_serializing_if = "BundleConfig::is_empty")]
    pub bundle: BundleConfig,
    pub inputs: Vec<InputConfig>,
    #[serde(default, skip_serializing_if = "TemplateConfig::is_empty")]
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
    #[serde(default, skip_serializing_if = "TemplateConfig::builtin_is_empty")]
    pub builtin: Option<BuiltinTemplateConfig>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinTemplateConfig {
    pub language: Option<String>,
    pub name: Option<String>,
}

impl TemplateConfig {
    pub fn is_empty(&self) -> bool {
        let builtin_is_empty = match &self.builtin {
            None => true,
            Some(builtin) => builtin.is_empty(),
        };

        builtin_is_empty && self.path.is_none()
    }

    fn builtin_is_empty(builtin: &Option<BuiltinTemplateConfig>) -> bool {
        match builtin {
            None => true,
            Some(builtin) => builtin.is_empty(),
        }
    }
}

impl BuiltinTemplateConfig {
    pub fn is_empty(&self) -> bool {
        self.language.is_none() && self.name.is_none()
    }
}

impl DefaultsConfig {
    pub fn is_empty(&self) -> bool {
        self.access_level.is_none() && self.incremental.is_none() && self.bundle.is_empty()
    }
}

impl BundleConfig {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.identifier.is_none()
    }
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        Ok(raw.into())
    }
}

impl Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let raw = RawConfig::from(self.clone());
        raw.serialize(serializer)
    }
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let jobs = raw
            .jobs
            .into_iter()
            .map(|(name, job)| JobConfig {
                name,
                output: job.output,
                access_level: job.access_level,
                incremental: job.incremental,
                bundle: job.bundle,
                inputs: job.inputs,
                template: job.template,
            })
            .collect();

        Self {
            version: raw.version,
            defaults: raw.defaults,
            jobs,
        }
    }
}

impl From<Config> for RawConfig {
    fn from(config: Config) -> Self {
        let mut jobs = BTreeMap::new();
        for job in config.jobs {
            let previous = jobs.insert(
                job.name,
                RawJobConfig {
                    output: job.output,
                    access_level: job.access_level,
                    incremental: job.incremental,
                    bundle: job.bundle,
                    inputs: job.inputs,
                    template: job.template,
                },
            );
            debug_assert!(previous.is_none(), "duplicate job names cannot serialize");
        }

        Self {
            version: config.version,
            defaults: config.defaults,
            jobs,
        }
    }
}
