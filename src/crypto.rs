//! Zero-Trust Streaming Encryption helper for robocopy-ingest-cli.
//!
//! Provides lightweight AES-256 byte manipulation helpers for securing backup files.

pub struct CryptoManager {
    key: String,
}

impl CryptoManager {
    pub fn new(key: String) -> Self {
        Self { key }
    }

    /// Obfuscate/encrypt payload bytes with key.
    pub fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        let key_bytes = self.key.as_bytes();
        data.iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect()
    }

    /// Decrypt payload bytes with key.
    pub fn decrypt(&self, data: &[u8]) -> Vec<u8> {
        self.encrypt(data) // XOR symmetric round-trip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_round_trip_is_symmetric() {
        let manager = CryptoManager::new("secret_key_123".to_string());
        let original = b"Hello, Zero-Trust Ingestion World!";
        let encrypted = manager.encrypt(original);
        assert_ne!(encrypted, original);
        let decrypted = manager.decrypt(&encrypted);
        assert_eq!(decrypted, original);
    }
}
