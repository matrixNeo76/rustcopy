//! Zero-Trust Streaming Encryption helper for robocopy-ingest-cli.
//!
//! Encrypts files in the destination tree after a successful transfer using AES-256-GCM
//! (authenticated encryption). The passphrase supplied via `--encrypt-aes256`/`--decrypt` is
//! stretched into a 256-bit key with SHA-256; this is a stopgap over a real password-KDF
//! (Argon2/PBKDF2 with a per-file salt) but already removes the XOR "encryption" this module used
//! to perform originally, which offered no confidentiality at all.
//!
//! # On-disk format (chunked, streaming)
//!
//! F25a fix: the previous version read the whole file into RAM with `std::fs::read`, encrypted it
//! in one `cipher.encrypt()` call, and wrote the whole ciphertext back with `std::fs::write` — a
//! 50 GB file meant a 50 GB (times two, briefly) allocation. This is a genuine anti-OOM violation
//! in a project whose own architecture doc makes memory-boundedness a design principle elsewhere
//! (bounded log channel, capped integrity error lists, reused stdout buffer). Real streaming:
//!
//! ```text
//! MAGIC (4 bytes: "RCE1")
//! record*
//!   nonce            (12 bytes, unique per record — fresh random nonce per chunk)
//!   ciphertext_len   (4 bytes, little-endian u32; <= CHUNK_SIZE + TAG_LEN)
//!   ciphertext+tag   (ciphertext_len bytes)
//! ```
//!
//! Plaintext is split into [`CHUNK_SIZE`]-byte chunks; each chunk gets its own randomly generated
//! nonce (AES-GCM's 96-bit nonce space makes accidental collision across any realistic number of
//! chunks negligible) and is encrypted independently, so encryption/decryption never holds more
//! than one chunk plus its ciphertext in memory regardless of file size. The explicit length
//! prefix (rather than relying on a fixed chunk size and reading until EOF) keeps the format
//! self-describing and lets the decrypter bound its per-record allocation instead of trusting an
//! attacker-controlled or corrupted length unconditionally.

use std::fs::File;
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, Generate, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use sha2::{Digest, Sha256};

use crate::errors::IngestError;

/// Length of the random nonce prepended to every chunk record, in bytes.
pub const NONCE_LEN: usize = 12;
/// AES-GCM authentication tag length, in bytes.
const TAG_LEN: usize = 16;
/// Plaintext bytes per chunk. Bounds peak memory use during encrypt/decrypt to roughly this many
/// bytes regardless of total file size.
pub const CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB
/// 4-byte format tag at the start of every encrypted file, so a corrupted or non-encrypted input
/// fails fast with a clear error instead of a confusing AEAD authentication failure.
const MAGIC: &[u8; 4] = b"RCE1";
/// Upper bound on a single record's ciphertext length, enforced on decrypt before allocating —
/// without this, a corrupted or hostile 4-byte length prefix could request an arbitrarily large
/// allocation.
const MAX_RECORD_CIPHERTEXT_LEN: usize = CHUNK_SIZE + TAG_LEN;

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
        let key = Key::<Aes256Gcm>::try_from(&digest[..])
            .map_err(|_| IngestError::Crypto("derived key is not 32 bytes".into()))?;
        let cipher = Aes256Gcm::new(&key);
        Ok(Self { cipher })
    }

    /// Encrypt `input` chunk-by-chunk into `output`. Never holds more than one chunk (plaintext
    /// and ciphertext) in memory, so this is safe to call on arbitrarily large streams.
    pub fn encrypt_stream<R: Read, W: Write>(
        &self,
        mut input: R,
        mut output: W,
    ) -> Result<(), IngestError> {
        output
            .write_all(MAGIC)
            .map_err(|e| IngestError::Crypto(format!("cannot write header: {e}")))?;

        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let n = read_up_to(&mut input, &mut buf)
                .map_err(|e| IngestError::Crypto(format!("read error: {e}")))?;
            if n == 0 {
                break;
            }

            let nonce = Nonce::generate();
            let ciphertext = self
                .cipher
                .encrypt(&nonce, &buf[..n])
                .map_err(|e| IngestError::Crypto(format!("encryption failed: {e}")))?;

            output
                .write_all(&nonce)
                .and_then(|_| output.write_all(&(ciphertext.len() as u32).to_le_bytes()))
                .and_then(|_| output.write_all(&ciphertext))
                .map_err(|e| IngestError::Crypto(format!("write error: {e}")))?;
        }
        output
            .flush()
            .map_err(|e| IngestError::Crypto(format!("flush error: {e}")))
    }

    /// Decrypt a stream produced by [`Self::encrypt_stream`]. Bounds each chunk's allocation to
    /// `MAX_RECORD_CIPHERTEXT_LEN`, so a corrupted length prefix cannot force an unbounded
    /// allocation.
    pub fn decrypt_stream<R: Read, W: Write>(
        &self,
        mut input: R,
        mut output: W,
    ) -> Result<(), IngestError> {
        let mut magic = [0u8; 4];
        match input.read_exact(&mut magic) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
                return Err(IngestError::Crypto(
                    "not an encrypted file (empty or truncated before the header)".to_string(),
                ));
            }
            Err(e) => return Err(IngestError::Crypto(format!("read error: {e}"))),
        }
        if &magic != MAGIC {
            return Err(IngestError::Crypto(
                "not an encrypted file (missing RCE1 header) — wrong file, or already decrypted"
                    .to_string(),
            ));
        }

        loop {
            let mut nonce_buf = [0u8; NONCE_LEN];
            match input.read_exact(&mut nonce_buf) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break, // clean end of stream
                Err(e) => return Err(IngestError::Crypto(format!("read error: {e}"))),
            }

            let mut len_buf = [0u8; 4];
            input
                .read_exact(&mut len_buf)
                .map_err(|e| IngestError::Crypto(format!("truncated record (length): {e}")))?;
            let len = u32::from_le_bytes(len_buf) as usize;
            if len > MAX_RECORD_CIPHERTEXT_LEN {
                return Err(IngestError::Crypto(format!(
                    "corrupted file: record claims {len} bytes, exceeding the maximum of \
                     {MAX_RECORD_CIPHERTEXT_LEN} for a single chunk"
                )));
            }

            let mut ciphertext = vec![0u8; len];
            input
                .read_exact(&mut ciphertext)
                .map_err(|e| IngestError::Crypto(format!("truncated record (ciphertext): {e}")))?;

            let nonce = Nonce::try_from(&nonce_buf[..])
                .map_err(|_| IngestError::Crypto("nonce is not 12 bytes".into()))?;
            let plaintext = self
                .cipher
                .decrypt(&nonce, ciphertext.as_slice())
                .map_err(|e| IngestError::Crypto(format!("decryption failed (wrong key?): {e}")))?;

            output
                .write_all(&plaintext)
                .map_err(|e| IngestError::Crypto(format!("write error: {e}")))?;
        }
        output
            .flush()
            .map_err(|e| IngestError::Crypto(format!("flush error: {e}")))
    }

    /// Encrypt the file at `path` in place: streams to a sibling temp file, then atomically
    /// renames over the original so a crash or error mid-write never leaves a half-encrypted
    /// file at the real path.
    pub fn encrypt_file(&self, path: &Path) -> Result<(), IngestError> {
        self.transform_file(path, |mgr, r, w| mgr.encrypt_stream(r, w))
    }

    /// Decrypt the file at `path` in place, with the same temp-file-then-rename safety as
    /// [`Self::encrypt_file`].
    pub fn decrypt_file(&self, path: &Path) -> Result<(), IngestError> {
        self.transform_file(path, |mgr, r, w| mgr.decrypt_stream(r, w))
    }

    fn transform_file(
        &self,
        path: &Path,
        f: impl FnOnce(&Self, BufReader<File>, BufWriter<File>) -> Result<(), IngestError>,
    ) -> Result<(), IngestError> {
        let tmp_path = sibling_tmp_path(path);
        {
            let input = File::open(path).map_err(|e| IngestError::io(path, e))?;
            let output = File::create(&tmp_path).map_err(|e| IngestError::io(&tmp_path, e))?;
            if let Err(error) = f(self, BufReader::new(input), BufWriter::new(output)) {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(error);
            }
        }
        std::fs::rename(&tmp_path, path).map_err(|e| IngestError::io(path, e))
    }
}

/// Read up to `buf.len()` bytes, looping on short reads (a single `Read::read` call is not
/// guaranteed to fill the buffer even mid-stream). Returns the number of bytes actually read;
/// less than `buf.len()` only happens at EOF.
fn read_up_to<R: Read>(input: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match input.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
}

/// A same-directory sibling path for the temp file used during atomic encrypt/decrypt, so the
/// final `rename` stays on the same filesystem/volume (required for it to be atomic).
fn sibling_tmp_path(path: &Path) -> PathBuf {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".rustcopy-tmp");
    path.with_file_name(tmp_name)
}

/// Resolve the key material for `--encrypt-aes256`/`--decrypt <VALUE>`.
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
        let content = std::fs::read_to_string(path).map_err(|e| IngestError::io(path, e))?;
        Ok(content.lines().next().unwrap_or("").trim().to_string())
    } else {
        tracing::warn!(
            "encryption key passed directly on the command line is visible in the process list; \
             prefer env:NAME or file:PATH"
        );
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {

    /// Pins the on-disk `RCE1` format against a byte-for-byte blob produced by an **older build**
    /// (aes-gcm 0.10, before the 0.11 upgrade).
    ///
    /// This exists because nothing else checks it. Every other crypto test round-trips within one
    /// build: encrypt then decrypt with the same code, which stays green even if a library upgrade
    /// silently changes the bytes written to disk. For a backup tool that is the failure that
    /// matters — old archives become unreadable, and the test suite says everything is fine.
    ///
    /// The blob below was produced by the pre-upgrade binary and verified by hand to decrypt
    /// correctly under aes-gcm 0.11. If a future dependency bump breaks this test, **do not
    /// regenerate the blob**: it means that release can no longer read backups written by earlier
    /// ones, which is a migration problem, not a test problem.
    #[test]
    fn ciphertext_written_by_an_older_build_still_decrypts() {
        // RCE1 || nonce(12) || len(4, LE) || ciphertext+tag
        const BLOB_HEX: &str = "52434531835a79aa8b868692b4e649d73000000048d4c73a1a2019da2cf9d8d606673a127d19c6be520f86d2908296394c434868a7c4fce975aeacc139ed59c2815a4dd2";
        const KEY: &str = "passphrase-di-prova";
        const EXPECTED: &[u8] = b"contenuto segreto da preservare
";

        let blob: Vec<u8> = BLOB_HEX
            .as_bytes()
            .chunks(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).expect("ascii"), 16).expect("hex")
            })
            .collect();
        assert_eq!(
            &blob[..4],
            b"RCE1",
            "the header is part of the pinned format"
        );

        let manager = CryptoManager::new(KEY).expect("key");
        let mut out = Vec::new();
        manager
            .decrypt_stream(&mut blob.as_slice(), &mut out)
            .expect("a blob written by an older build must still decrypt");

        assert_eq!(
            out, EXPECTED,
            "the recovered plaintext must be byte-identical to what was encrypted"
        );
    }
    use super::*;
    use std::io::Cursor;

    fn round_trip(manager: &CryptoManager, plaintext: &[u8]) -> Vec<u8> {
        let mut ciphertext = Vec::new();
        manager
            .encrypt_stream(Cursor::new(plaintext), &mut ciphertext)
            .expect("encrypt");
        let mut decrypted = Vec::new();
        manager
            .decrypt_stream(Cursor::new(ciphertext), &mut decrypted)
            .expect("decrypt");
        decrypted
    }

    #[test]
    fn crypto_round_trip_is_symmetric() {
        let manager = CryptoManager::new("secret_key_123").expect("key");
        let original = b"Hello, Zero-Trust Ingestion World!";
        let mut ciphertext = Vec::new();
        manager
            .encrypt_stream(Cursor::new(original), &mut ciphertext)
            .expect("encrypt");
        assert!(!ciphertext
            .windows(original.len())
            .any(|w| w == &original[..]));
        assert_eq!(round_trip(&manager, original), original);
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(CryptoManager::new("").is_err());
    }

    #[test]
    fn each_encryption_uses_a_fresh_nonce() {
        let manager = CryptoManager::new("k").expect("key");
        let mut a = Vec::new();
        let mut b = Vec::new();
        manager
            .encrypt_stream(Cursor::new(b"same plaintext"), &mut a)
            .expect("encrypt a");
        manager
            .encrypt_stream(Cursor::new(b"same plaintext"), &mut b)
            .expect("encrypt b");
        assert_ne!(a, b, "nonce must differ between calls");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let a = CryptoManager::new("key-a").expect("key a");
        let b = CryptoManager::new("key-b").expect("key b");
        let mut ciphertext = Vec::new();
        a.encrypt_stream(Cursor::new(b"secret payload"), &mut ciphertext)
            .expect("encrypt");
        let mut out = Vec::new();
        assert!(b.decrypt_stream(Cursor::new(ciphertext), &mut out).is_err());
    }

    #[test]
    fn truncated_ciphertext_is_rejected() {
        let manager = CryptoManager::new("k").expect("key");
        let mut out = Vec::new();
        assert!(manager
            .decrypt_stream(Cursor::new([0u8; 4]), &mut out)
            .is_err());
    }

    #[test]
    fn empty_input_is_rejected_as_not_encrypted() {
        let manager = CryptoManager::new("k").expect("key");
        let mut out = Vec::new();
        let err = manager
            .decrypt_stream(Cursor::new([] as [u8; 0]), &mut out)
            .expect_err("empty input has no header");
        assert!(format!("{err}").contains("not an encrypted file"));
    }

    #[test]
    fn wrong_magic_is_rejected_with_a_clear_message() {
        let manager = CryptoManager::new("k").expect("key");
        let mut out = Vec::new();
        let err = manager
            .decrypt_stream(Cursor::new(b"NOTREALDATA"), &mut out)
            .expect_err("bad magic");
        assert!(format!("{err}").contains("not an encrypted file"));
    }

    #[test]
    fn oversized_record_length_is_rejected_without_allocating() {
        let manager = CryptoManager::new("k").expect("key");
        let mut malicious = Vec::new();
        malicious.extend_from_slice(MAGIC);
        malicious.extend_from_slice(&[0u8; NONCE_LEN]);
        malicious.extend_from_slice(&u32::MAX.to_le_bytes()); // absurd claimed length
        let mut out = Vec::new();
        let err = manager
            .decrypt_stream(Cursor::new(malicious), &mut out)
            .expect_err("must reject before allocating");
        assert!(format!("{err}").contains("exceeding the maximum"));
    }

    /// F25a: a plaintext spanning multiple CHUNK_SIZE-sized chunks must round-trip correctly,
    /// proving the chunking/reassembly logic itself (not just the single-chunk happy path).
    #[test]
    fn multi_chunk_file_round_trips() {
        let manager = CryptoManager::new("chunked-key").expect("key");
        // 2.5 chunks: exercises two full chunks plus one partial final chunk.
        let size = CHUNK_SIZE * 2 + CHUNK_SIZE / 2;
        let plaintext: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        assert_eq!(round_trip(&manager, &plaintext), plaintext);
    }

    #[test]
    fn exact_multiple_of_chunk_size_round_trips() {
        let manager = CryptoManager::new("exact-key").expect("key");
        let plaintext = vec![7u8; CHUNK_SIZE * 2];
        assert_eq!(round_trip(&manager, &plaintext), plaintext);
    }

    #[test]
    fn empty_file_round_trips_to_empty() {
        let manager = CryptoManager::new("k").expect("key");
        assert_eq!(round_trip(&manager, b""), b"");
    }

    #[test]
    fn encrypt_file_and_decrypt_file_round_trip_on_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.bin");
        let original: Vec<u8> = (0..(CHUNK_SIZE + 100)).map(|i| (i % 256) as u8).collect();
        std::fs::write(&path, &original).expect("seed file");

        let manager = CryptoManager::new("file-key").expect("key");
        manager.encrypt_file(&path).expect("encrypt in place");
        let encrypted = std::fs::read(&path).expect("read encrypted");
        assert_ne!(encrypted, original);
        assert!(encrypted.starts_with(MAGIC));

        manager.decrypt_file(&path).expect("decrypt in place");
        let restored = std::fs::read(&path).expect("read restored");
        assert_eq!(restored, original);
    }

    #[test]
    fn encrypt_file_leaves_no_temp_file_behind_on_success() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"some data").expect("seed file");

        let manager = CryptoManager::new("k").expect("key");
        manager.encrypt_file(&path).expect("encrypt in place");

        let tmp = sibling_tmp_path(&path);
        assert!(!tmp.exists(), "temp file must not survive a successful run");
    }

    #[test]
    fn decrypt_file_fails_cleanly_on_a_plain_unencrypted_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plain.txt");
        std::fs::write(&path, b"just a normal file, never encrypted").expect("seed file");

        let manager = CryptoManager::new("k").expect("key");
        let err = manager
            .decrypt_file(&path)
            .expect_err("must reject a non-encrypted file");
        assert!(format!("{err}").contains("not an encrypted file"));
        // The original file must be untouched (no partial/temp corruption left behind).
        assert_eq!(
            std::fs::read(&path).expect("read original"),
            b"just a normal file, never encrypted"
        );
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
