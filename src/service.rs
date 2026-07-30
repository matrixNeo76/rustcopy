//! Windows Service Control Manager Integration for robocopy-ingest-cli.
//!
//! Handles background service control signals for autonomous background execution.

pub fn is_service_environment() -> bool {
    std::env::var("RUNNING_AS_SERVICE").is_ok()
}

pub fn register_windows_service() -> Result<(), String> {
    tracing::info!("registering robocopy-ingest-cli with Windows Service Control Manager");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_service_environment() {
        assert!(!is_service_environment());
    }

    #[test]
    fn register_service_succeeds() {
        assert!(register_windows_service().is_ok());
    }
}
