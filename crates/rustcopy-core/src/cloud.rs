//! Cloud Storage Sync Engine abstraction for robocopy-ingest-cli.
//!
//! Provides connectors for syncing datasets directly with object storage
//! endpoints such as AWS S3 or Azure Blob Storage.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudProvider {
    AwsS3,
    AzureBlob,
}

pub struct CloudSyncRequest<'a> {
    pub provider: CloudProvider,
    pub bucket_or_container: &'a str,
    pub source_path: &'a Path,
    pub target_prefix: &'a str,
}

pub fn sync_to_cloud(request: &CloudSyncRequest) -> Result<u64, String> {
    tracing::info!(
        provider = ?request.provider,
        bucket = %request.bucket_or_container,
        source = %request.source_path.display(),
        "initiating direct cloud sync"
    );

    // Deliberately an error, not a fake success. `--cloud-sync-target` is a declared no-op
    // (AGENTS.md rule 7) and this function has no production caller, so nothing regresses -- but
    // returning `Ok(100)` meant the one thing a future caller must never be told: that a backup
    // reached the cloud when no object exists at the target. A stub that refuses is safe to wire
    // up by accident; a stub that lies is not.
    Err(format!(
        "cloud sync is not implemented: nothing was uploaded to {:?}/{}.          --cloud-sync-target is accepted for forward compatibility only.",
        request.provider, request.bucket_or_container
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn cloud_sync_request_constructs_properly() {
        let path = PathBuf::from("D:/landing");
        let req = CloudSyncRequest {
            provider: CloudProvider::AwsS3,
            bucket_or_container: "my-backup-bucket",
            source_path: &path,
            target_prefix: "2026-07/",
        };

        // Construction is what this test is about; the call is checked below.
        assert_eq!(req.bucket_or_container, "my-backup-bucket");
        assert_eq!(req.target_prefix, "2026-07/");
    }

    /// The stub must never report success. A caller that wires this up and believes `Ok` would
    /// mark a cloud backup complete with nothing at the target — the failure mode a backup tool
    /// can least afford.
    #[test]
    fn the_unimplemented_stub_reports_failure_rather_than_a_fake_byte_count() {
        let path = PathBuf::from("D:/landing");
        let req = CloudSyncRequest {
            provider: CloudProvider::AwsS3,
            bucket_or_container: "my-backup-bucket",
            source_path: &path,
            target_prefix: "2026-07/",
        };

        let error =
            sync_to_cloud(&req).expect_err("a stub that uploads nothing must not return Ok");
        assert!(
            error.contains("not implemented"),
            "the error must say why, got: {error}"
        );
    }
}
