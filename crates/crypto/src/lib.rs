//! # Guisu Crypto
//!
//! Encryption and decryption support for guisu using age encryption.
//!
//! This crate provides functionality for encrypting and decrypting files
//! using the age encryption format with identity-based keys.

pub mod age;
pub mod identity;
pub mod recipient;

pub use age::{
    decrypt, decrypt_file_content, decrypt_inline, decrypt_string, encrypt, encrypt_file_content,
    encrypt_inline, encrypt_string,
};
pub use identity::{Identity, IdentityFile, load_identities};
pub use recipient::Recipient;

/// Age encryption provider that implements the `EncryptionProvider` trait
///
/// This struct wraps recipients and identities to provide encryption/decryption
/// functionality through a trait-based interface.
pub struct AgeEncryption {
    recipients: Vec<Recipient>,
    identities: Vec<Identity>,
}

impl AgeEncryption {
    /// Create a new `AgeEncryption` instance with the given recipients and identities
    #[must_use]
    pub fn new(recipients: Vec<Recipient>, identities: Vec<Identity>) -> Self {
        Self {
            recipients,
            identities,
        }
    }

    /// Create an instance with only recipients (encryption-only)
    #[must_use]
    pub fn with_recipients(recipients: Vec<Recipient>) -> Self {
        Self {
            recipients,
            identities: Vec::new(),
        }
    }

    /// Create an instance with only identities (decryption-only)
    #[must_use]
    pub fn with_identities(identities: Vec<Identity>) -> Self {
        Self {
            recipients: Vec::new(),
            identities,
        }
    }
}

// Implement EncryptionProvider trait for AgeEncryption
impl guisu_core::EncryptionProvider for AgeEncryption {
    fn encrypt(&self, data: &[u8]) -> guisu_core::Result<Vec<u8>> {
        encrypt(data, &self.recipients).map_err(|e| guisu_core::Error::Message(e.to_string()))
    }

    fn decrypt(&self, data: &[u8]) -> guisu_core::Result<Vec<u8>> {
        decrypt(data, &self.identities).map_err(|e| guisu_core::Error::Message(e.to_string()))
    }
}

use thiserror::Error;

/// Result type for crypto operations
pub type Result<T> = std::result::Result<T, Error>;

/// Crypto-related errors
#[derive(Error, Debug)]
pub enum Error {
    /// Age encryption/decryption error
    #[error("Age encryption error: {0}")]
    Age(String),

    /// No recipients provided for encryption
    #[error(
        "No recipients provided for encryption\n\
         \n\
         To fix this:\n\
         1. Add recipients to your config (~/.config/guisu/config.toml):\n\
         \n\
         [age]\n\
         recipient = \"age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p\"\n\
         \n\
         2. Or generate a recipient from your identity:\n\
            guisu age generate --show-recipient"
    )]
    NoRecipients,

    /// Identity file not found
    #[error(
        "Identity file not found: {path}\n\
         \n\
         To fix this:\n\
         1. Generate a new identity:    guisu age generate\n\
         2. Or check the file path:     ls {path}\n\
         3. Or configure in config:     ~/.config/guisu/config.toml\n\
         \n\
         [age]\n\
         identity = \"{path}\""
    )]
    IdentityNotFound {
        /// Path to the identity file that was not found
        path: String,
    },

    /// Identity file IO error (read/write failures)
    #[error(
        "Failed to {operation} identity file: {path}\n\
         Error: {source}\n\
         \n\
         To fix this:\n\
         1. Check file permissions:     ls -la {path}\n\
         2. Ensure directory exists:    mkdir -p $(dirname {path})\n\
         3. Check disk space:           df -h"
    )]
    IdentityFile {
        /// Operation that failed (read/write)
        operation: String,
        /// Path to the identity file
        path: String,
        /// Underlying IO error
        #[source]
        source: std::io::Error,
    },

    /// Invalid identity format or content
    #[error(
        "Invalid identity: {reason}\n\
         \n\
         Expected format:\n\
         - Age identity:  AGE-SECRET-KEY-1...\n\
         - SSH key:       -----BEGIN OPENSSH PRIVATE KEY-----\n\
         \n\
         To fix this:\n\
         1. Generate a new identity:    guisu age generate\n\
         2. Or use an SSH key:          ~/.ssh/id_ed25519\n\
         3. Check file contents:        cat {path}"
    )]
    InvalidIdentity {
        /// Reason for the invalid identity
        reason: String,
        /// Path to the identity file
        path: String,
    },

    /// Invalid recipient format
    #[error(
        "Invalid recipient: {recipient}\n\
         Reason: {reason}\n\
         \n\
         Expected format: age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p\n\
         \n\
         To fix this:\n\
         1. Get recipient from identity:  guisu age generate --show-recipient\n\
         2. Or from public key file:      cat ~/.config/guisu/key.txt.pub\n\
         3. Check the recipient string carefully"
    )]
    InvalidRecipient {
        /// Invalid recipient string
        recipient: String,
        /// Reason for the invalid recipient
        reason: String,
    },

    /// Decryption failed due to wrong key
    #[error("Decryption failed - wrong key or corrupted data")]
    WrongKey,

    /// Decryption failed for other reasons
    #[error(
        "Decryption failed: {reason}\n\
         \n\
         To fix this:\n\
         1. Check the encrypted file:   cat <file>\n\
         2. Verify identity is loaded:  guisu doctor\n\
         3. Check file format is valid"
    )]
    DecryptionFailed {
        /// Reason for decryption failure
        reason: String,
    },

    /// No identity available for decryption
    #[error(
        "No identity available for decryption\n\
         \n\
         To fix this:\n\
         1. Generate a new identity:  guisu age generate\n\
         2. Or configure an existing identity in ~/.config/guisu/config.toml:\n\
         \n\
         [age]\n\
         identity = \"~/.ssh/id_ed25519\"  # Use SSH key\n\
         # or\n\
         identity = \"~/.config/guisu/key.txt\"  # Use age key"
    )]
    NoIdentity,

    /// Attempted to encrypt empty value
    #[error(
        "Cannot encrypt empty value\n\
         \n\
         To fix this:\n\
         1. Provide non-empty content to encrypt\n\
         2. Or remove the encrypted file attribute if not needed"
    )]
    EmptyValue,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn test_age_encryption_new() {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let age_enc = AgeEncryption::new(vec![recipient.clone()], vec![identity.clone()]);

        assert_eq!(age_enc.recipients.len(), 1);
        assert_eq!(age_enc.identities.len(), 1);
    }

    #[test]
    fn test_age_encryption_with_recipients() {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let age_enc = AgeEncryption::with_recipients(vec![recipient.clone()]);

        assert_eq!(age_enc.recipients.len(), 1);
        assert_eq!(age_enc.identities.len(), 0);
    }

    #[test]
    fn test_age_encryption_with_identities() {
        let identity = Identity::generate();

        let age_enc = AgeEncryption::with_identities(vec![identity.clone()]);

        assert_eq!(age_enc.recipients.len(), 0);
        assert_eq!(age_enc.identities.len(), 1);
    }

    #[test]
    fn test_encryption_provider_trait_encrypt() {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let age_enc = AgeEncryption::with_recipients(vec![recipient]);

        let data = b"secret message";
        let encrypted = guisu_core::EncryptionProvider::encrypt(&age_enc, data)
            .expect("Encryption should succeed");

        // Encrypted data should be different from original
        assert_ne!(encrypted, data);
        // Should be longer due to age envelope
        assert!(encrypted.len() > data.len());
    }

    #[test]
    fn test_encryption_provider_trait_decrypt() {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        // Encrypt with recipient
        let age_enc_encrypt = AgeEncryption::with_recipients(vec![recipient]);
        let data = b"secret message";
        let encrypted = guisu_core::EncryptionProvider::encrypt(&age_enc_encrypt, data)
            .expect("Encryption should succeed");

        // Decrypt with identity
        let age_enc_decrypt = AgeEncryption::with_identities(vec![identity]);
        let decrypted = guisu_core::EncryptionProvider::decrypt(&age_enc_decrypt, &encrypted)
            .expect("Decryption should succeed");

        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_encryption_provider_trait_roundtrip() {
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let age_enc = AgeEncryption::new(vec![recipient], vec![identity]);

        let original = b"test data for roundtrip";

        // Encrypt
        let encrypted = guisu_core::EncryptionProvider::encrypt(&age_enc, original)
            .expect("Encryption should succeed");

        // Decrypt
        let decrypted = guisu_core::EncryptionProvider::decrypt(&age_enc, &encrypted)
            .expect("Decryption should succeed");

        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_encryption_provider_no_recipients_error() {
        let age_enc = AgeEncryption::with_recipients(vec![]);

        let data = b"cannot encrypt this";
        let result = guisu_core::EncryptionProvider::encrypt(&age_enc, data);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No recipients"));
    }

    #[test]
    fn test_encryption_provider_no_identities_error() {
        // Create some encrypted data first
        let identity = Identity::generate();
        let recipient = identity.to_public();

        let age_enc_encrypt = AgeEncryption::with_recipients(vec![recipient]);
        let encrypted = guisu_core::EncryptionProvider::encrypt(&age_enc_encrypt, b"data")
            .expect("Encryption should succeed");

        // Try to decrypt with no identities
        let age_enc_decrypt = AgeEncryption::with_identities(vec![]);
        let result = guisu_core::EncryptionProvider::decrypt(&age_enc_decrypt, &encrypted);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("No identity") || err.to_string().contains("decrypt"));
    }

    #[test]
    fn test_encryption_provider_wrong_identity() {
        // Encrypt with one identity's recipient
        let identity1 = Identity::generate();
        let recipient1 = identity1.to_public();

        let age_enc_encrypt = AgeEncryption::with_recipients(vec![recipient1]);
        let encrypted = guisu_core::EncryptionProvider::encrypt(&age_enc_encrypt, b"data")
            .expect("Encryption should succeed");

        // Try to decrypt with a different identity
        let identity2 = Identity::generate();
        let age_enc_decrypt = AgeEncryption::with_identities(vec![identity2]);
        let result = guisu_core::EncryptionProvider::decrypt(&age_enc_decrypt, &encrypted);

        assert!(result.is_err());
    }
}
