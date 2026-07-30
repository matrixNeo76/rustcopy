//! Zero-Trust Streaming Encryption helper for robocopy-ingest-cli.
//!
//! Encrypts files in the destination tree after a successful transfer using AES-256-GCM
//! (authenticated encryption): a random 96-bit nonce is generated per file and prepended to the
//! ciphertext, so the on-disk layout is `nonce(12 bytes) || ciphertext+tag`. The passphrase
//! supplied via `--encrypt-aes256` is stretched into a 256-bit key with SHA-256; this is a
//! stopgap over a real password-KDF (Argon2/PBKDF2 with a per-file salt) but already removes the
//! XOR "encryption" this module used to perform, which offered no confidentiality at all.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use sha2::{Digest, Sha256};

use crate::errors::IngestError;

/// Length of the random nonce prepended to every encrypted file, in bytes.
pub const NONCE_LEN: usize = 12;

pub struct CryptoManager {
    cipher: Aes256Gcm,
}

impl CryptoManager {
    /// Build a manager from a passphrase. The passphrase is hashed with SHA-256 to derive the
    /// 256-bit AES key; an empty passphrase is rejected rather than silently producing a
    /// degenerate key.
    pub fn new(key: &str) -> Result<Self, IngestError> {
        if key.is_empty() {
            return Err(IngestError::Crypto(
                "encryption key must not be empty".to_string(),
            ));
        }
        let digest = Sha256::digest(key.as_bytes());
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&digest));
        Ok(Self { cipher })
    }

    /// Encrypt `data`, returning `nonce || ciphertext+tag`.
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, IngestError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, data)
            .map_err(|e| IngestError::Crypto(format!("encryption failed: {e}")))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt a buffer produced by [`Self::encrypt`]: `nonce || ciphertext+tag`.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, IngestError> {
        if data.len() < NONCE_LEN {
            return Err(IngestError::Crypto(
                "ciphertext shorter than the nonce prefix".to_string(),
            ));
        }
        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| IngestError::Crypto(format!("decryption failed (wrong key?): {e}")))
    }
}

/// Resolve the key material for `--encrypt-aes256 <VALUE>`.
///
/// `VALUE` may be:
/// * `env:NAME`  — read the key from environment variable `NAME`;
/// * `file:PATH` — read the key from the first line of the file at `PATH`;
/// * anything else is treated as a literal passphrase (discouraged: it is visible in the
///   process list / shell history on most systems).
pub fn resolve_key(value: &str) -> Result<String, IngestError> {
    if let Some(name) = value.strip_prefix("env:") {
        std::env::var(name)
            .map_err(|_| IngestError::Crypto(format!("environment variable {name} is not set")))
    } else if let Some(path) = value.strip_prefix("file:") {
        let content = std::fs::read_to_string(path)
            .map_err(|e| IngestError::io(path, e))?;
        Ok(content.lines().next().unwrap_or("").trim().to_string())
    } else {
        tracing::warn!(
            "--encrypt-aes256 key passed directly on the command line is visible in the process \
             list; prefer env:NAME or file:PATH"
        );
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_round_trip_is_symmetric() {
        let manager = CryptoManager::new("secret_key_123").expect("key");
        let original = b"Hello, Zero-Trust Ingestion World!";
        let encrypted = manager.encrypt(original).expect("encrypt");
        assert_ne!(encrypted[NONCE_LEN..], original[..]);
        let decrypted = manager.decrypt(&encrypted).expect("decrypt");
        assert_eq!(decrypted, original);
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(CryptoManager::new("").is_err());
    }

    #[test]
    fn each_encryption_uses_a_fresh_nonce() {
        let manager = CryptoManager::new("k").expect("key");
        let a = manager.encrypt(b"same plaintext").expect("encrypt a");
        let b = manager.encrypt(b"same plaintext").expect("encrypt b");
        assert_ne!(a, b, "nonce must differ between calls");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let a = CryptoManager::new("key-a").expect("key a");
        let b = CryptoManager::new("key-b").expect("key b");
        let encrypted = a.encrypt(b"secret payload").expect("encrypt");
        assert!(b.decrypt(&encrypted).is_err());
    }

    #[test]
    fn truncated_ciphertext_is_rejected() {
        let manager = CryptoManager::new("k").expect("key");
        assert!(manager.decrypt(&[0u8; 4]).is_err());
    }

    #[test]
    fn resolve_key_reads_from_env() {
        std::env::set_var("ROBOCOPY_INGEST_TEST_KEY", "from-env");
        assert_eq!(
            resolve_key("env:ROBOCOPY_INGEST_TEST_KEY").expect("resolve"),
            "from-env"
        );
        std::env::remove_var("ROBOCOPY_INGEST_TEST_KEY");
    }

    #[test]
    fn resolve_key_reads_from_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("key.txt");
        std::fs::write(&path, "file-key\n").expect("write");
        assert_eq!(
            resolve_key(&format!("file:{}", path.display())).expect("resolve"),
            "file-key"
        );
    }

    #[test]
    fn resolve_key_treats_plain_value_as_literal() {
        assert_eq!(resolve_key("literal").expect("resolve"), "literal");
    }
}
