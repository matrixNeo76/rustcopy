//! TOML Configuration file parser and mapper for robocopy_ingest.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::errors::IngestError;
use crate::integrity::HashAlgorithm;

/// TOML Configuration schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IngestConfig {
    pub source: Option<PathBuf>,
    pub dest: Option<PathBuf>,
    pub pattern: Option<String>,
    pub threads: Option<u16>,
    pub retries: Option<u32>,
    pub retry_wait_seconds: Option<u64>,
    pub verify_integrity: Option<bool>,
    pub hash_algo: Option<HashAlgorithm>,
    pub compare_baseline: Option<bool>,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub dry_run: Option<bool>,
    pub mirror: Option<bool>,
    pub exclude_files: Option<Vec<String>>,
    pub exclude_dirs: Option<Vec<String>>,
    pub min_age_days: Option<u32>,
    pub max_age_days: Option<u32>,
    pub bandwidth_limit_mbps: Option<u32>,
    pub no_prescan: Option<bool>,
    pub long_paths: Option<bool>,
    pub preserve_timestamps: Option<bool>,
    pub preserve_acl: Option<bool>,
    pub webhook_url: Option<String>,
}

impl IngestConfig {
    /// Load and parse a TOML configuration file from disk.
    pub fn load_from(path: &Path) -> Result<Self, IngestError> {
        let content = fs::read_to_string(path).map_err(|err| IngestError::io(path, err))?;
        toml::from_str(&content).map_err(|err| {
            IngestError::io(path, std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_toml_config() {
        let toml_str = r#"
            source = "D:/landing"
            dest = "E:/warehouse"
            pattern = "*.csv"
            threads = 16
            verify_integrity = true
            hash_algo = "blake3"
            mirror = true
            exclude_files = ["*.tmp", "thumbs.db"]
            exclude_dirs = [".git"]
        "#;

        let config: IngestConfig = toml::from_str(toml_str).expect("valid toml");
        assert_eq!(config.source, Some(PathBuf::from("D:/landing")));
        assert_eq!(config.dest, Some(PathBuf::from("E:/warehouse")));
        assert_eq!(config.threads, Some(16));
        assert_eq!(config.verify_integrity, Some(true));
        assert_eq!(config.hash_algo, Some(HashAlgorithm::Blake3));
        assert_eq!(config.mirror, Some(true));
        assert_eq!(config.exclude_files, Some(vec!["*.tmp".to_string(), "thumbs.db".to_string()]));
    }
}
