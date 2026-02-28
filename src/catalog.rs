use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CatalogFile {
    pub software: BTreeMap<String, SoftwareSpec>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SoftwareSpec {
    pub display_name: String,
    pub description: Option<String>,
    pub enabled_by_default: bool,
    pub install_dir: Option<String>,
    pub source: SourceSpec,
    #[serde(default)]
    pub setup_steps: Vec<SetupStep>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpec {
    #[serde(rename = "official_source")]
    OfficialSource { 
        id: Option<String>,
        url: Option<String>, 
        version_regex: Option<String>, 
        download_url_regex: Option<String> 
    },
    #[serde(rename = "package_manager")]
    PackageManager,
    #[serde(rename = "github")]
    Github { repo: Option<String>, asset_pattern: String },
}

impl SourceSpec {
    pub fn kind_key(&self) -> &'static str {
        match self {
            SourceSpec::OfficialSource { .. } => "official_source",
            SourceSpec::PackageManager => "package_manager",
            SourceSpec::Github { .. } => "github",
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupStep {
    Package { packages: Vec<String> },
    PathHint { value: String },
    Note { value: String },
    Shell { command: String },
}

pub fn load_catalog(path: &Path) -> Result<CatalogFile, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read catalog at {}: {e}", path.display()))?;
    let parsed: CatalogFile = toml::from_str(&content)
        .map_err(|e| format!("failed to parse catalog at {}: {e}", path.display()))?;
    Ok(parsed)
}