//! Per-app encrypted secret vault — the "encrypt certain files" capability.
//!
//! AES-256-GCM at rest, **key-agnostic by construction**: this module never
//! touches the OS keyring (it lives in `biorouter-mcp`, which must not depend on
//! `biorouter::config`). The caller — the server, which can reach the keyring —
//! generates/loads a per-app data key and passes it in. That keeps the crypto
//! fully unit-testable and the key-management policy in one place upstream.
//!
//! On-disk layout: `workspace/.vault/<NAME>.enc` = `nonce(12) || AES-256-GCM`.
//! The `.vault/` dir is denied to the agent's file tools by the workspace jail,
//! so an app references secrets only by name (`{{vault:NAME}}`); the plaintext
//! is resolved server-side at tool-call time and never enters a WS frame or the
//! model context.

use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};

/// AES-256 key length.
pub const KEY_LEN: usize = 32;
/// A 256-bit data key.
pub type DataKey = [u8; KEY_LEN];
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    Encrypt,
    /// Decryption/authentication failed (wrong key or tampered ciphertext).
    Decrypt,
    /// The stored blob is too short to contain a nonce.
    Malformed,
    Io(String),
    /// A referenced secret name isn't in the vault / not allow-listed.
    Unknown(String),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::Encrypt => write!(f, "vault: encryption failed"),
            VaultError::Decrypt => write!(f, "vault: decryption failed (wrong key or tampered data)"),
            VaultError::Malformed => write!(f, "vault: malformed ciphertext"),
            VaultError::Io(e) => write!(f, "vault: io error: {e}"),
            VaultError::Unknown(n) => write!(f, "vault: unknown secret '{n}'"),
        }
    }
}
impl std::error::Error for VaultError {}

/// Generate a fresh random 256-bit data key from the OS CSPRNG.
pub fn generate_key() -> DataKey {
    let key = Aes256Gcm::generate_key(OsRng);
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(key.as_slice());
    k
}

/// Encrypt `plaintext` → `nonce(12) || ciphertext` (AES-256-GCM, fresh nonce).
pub fn seal(plaintext: &[u8], key: &DataKey) -> Result<Vec<u8>, VaultError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| VaultError::Encrypt)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a `nonce(12) || ciphertext` blob. GCM authentication means a wrong
/// key or any tampering yields `VaultError::Decrypt`, never garbage plaintext.
pub fn open(blob: &[u8], key: &DataKey) -> Result<Vec<u8>, VaultError> {
    if blob.len() < NONCE_LEN {
        return Err(VaultError::Malformed);
    }
    let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| VaultError::Decrypt)
}

/// A per-app vault: an `.vault/` directory under the workspace, encrypted with a
/// caller-supplied [`DataKey`]. Secret values are stored/read by name and the
/// `{{vault:NAME}}` template is resolved against allow-listed names only.
pub struct Vault {
    dir: PathBuf,
    key: DataKey,
}

impl Vault {
    /// A vault rooted at `<workspace>/.vault`.
    pub fn new(workspace: &Path, key: DataKey) -> Self {
        Vault {
            dir: workspace.join(".vault"),
            key,
        }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        // Names are flat identifiers; sanitize to a safe filename.
        let safe: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        self.dir.join(format!("{safe}.enc"))
    }

    /// Store (encrypt) a secret value under `name`.
    pub fn put(&self, name: &str, value: &str) -> Result<(), VaultError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| VaultError::Io(e.to_string()))?;
        let blob = seal(value.as_bytes(), &self.key)?;
        std::fs::write(self.path_for(name), blob).map_err(|e| VaultError::Io(e.to_string()))
    }

    /// Whether a secret exists.
    pub fn contains(&self, name: &str) -> bool {
        self.path_for(name).exists()
    }

    /// Load (decrypt) a secret value.
    pub fn get(&self, name: &str) -> Result<String, VaultError> {
        let path = self.path_for(name);
        if !path.exists() {
            return Err(VaultError::Unknown(name.to_string()));
        }
        let blob = std::fs::read(path).map_err(|e| VaultError::Io(e.to_string()))?;
        let bytes = open(&blob, &self.key)?;
        String::from_utf8(bytes).map_err(|_| VaultError::Decrypt)
    }

    /// Resolve every `{{vault:NAME}}` in `text`, but only for names in
    /// `allowed`. An unknown or non-allow-listed reference is left literal (so a
    /// typo/forbidden name can't silently leak a different secret). Used
    /// server-side at tool-call time so plaintext never crosses a wire.
    pub fn resolve_refs(&self, text: &str, allowed: &[String]) -> String {
        let mut out = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if text[i..].starts_with("{{vault:") {
                if let Some(end_rel) = text[i..].find("}}") {
                    let name = &text[i + 8..i + end_rel];
                    if allowed.iter().any(|a| a == name) {
                        if let Ok(val) = self.get(name) {
                            out.push_str(&val);
                            i += end_rel + 2;
                            continue;
                        }
                    }
                    // Not allow-listed or missing → leave the reference literal.
                    out.push_str(&text[i..i + end_rel + 2]);
                    i += end_rel + 2;
                    continue;
                }
            }
            // Push one UTF-8 char to stay on a char boundary.
            let ch = text[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = generate_key();
        let blob = seal(b"NCBI_API_KEY=abc123", &key).unwrap();
        // Ciphertext is not the plaintext.
        assert!(!blob.windows(3).any(|w| w == b"abc"));
        let back = open(&blob, &key).unwrap();
        assert_eq!(back, b"NCBI_API_KEY=abc123");
    }

    #[test]
    fn wrong_key_fails_authentication() {
        let blob = seal(b"secret", &generate_key()).unwrap();
        let other = generate_key();
        assert_eq!(open(&blob, &other), Err(VaultError::Decrypt));
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let key = generate_key();
        let mut blob = seal(b"secret-value", &key).unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xFF; // flip a bit in the ciphertext/tag
        assert_eq!(open(&blob, &key), Err(VaultError::Decrypt));
    }

    #[test]
    fn malformed_blob_rejected() {
        assert_eq!(open(b"short", &generate_key()), Err(VaultError::Malformed));
    }

    #[test]
    fn vault_put_get_roundtrip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::new(dir.path(), generate_key());
        assert!(!v.contains("OPENAI_KEY"));
        v.put("OPENAI_KEY", "sk-xyz").unwrap();
        assert!(v.contains("OPENAI_KEY"));
        assert_eq!(v.get("OPENAI_KEY").unwrap(), "sk-xyz");
        // On-disk file must not contain the plaintext.
        let raw = std::fs::read(dir.path().join(".vault/OPENAI_KEY.enc")).unwrap();
        assert!(!raw.windows(2).any(|w| w == b"xy"));
    }

    #[test]
    fn unknown_secret_errors() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::new(dir.path(), generate_key());
        assert_eq!(v.get("NOPE"), Err(VaultError::Unknown("NOPE".to_string())));
    }

    #[test]
    fn resolve_refs_only_allowlisted_names() {
        let dir = tempfile::tempdir().unwrap();
        let v = Vault::new(dir.path(), generate_key());
        v.put("API", "TOKEN123").unwrap();
        v.put("OTHER", "SHOULD_NOT_LEAK").unwrap();
        let allowed = vec!["API".to_string()];
        // Allow-listed → resolved.
        assert_eq!(
            v.resolve_refs("auth={{vault:API}}", &allowed),
            "auth=TOKEN123"
        );
        // Not allow-listed → left literal (no leak), even though it exists.
        assert_eq!(
            v.resolve_refs("x={{vault:OTHER}}", &allowed),
            "x={{vault:OTHER}}"
        );
        // Unknown name → literal.
        assert_eq!(
            v.resolve_refs("y={{vault:MISSING}}", &allowed),
            "y={{vault:MISSING}}"
        );
        // Multibyte text around a ref is preserved (no char-boundary panic).
        assert_eq!(
            v.resolve_refs("café {{vault:API}} déjà", &allowed),
            "café TOKEN123 déjà"
        );
    }
}
