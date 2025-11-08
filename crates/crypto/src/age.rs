//! Age encryption and decryption implementation
//!
//! This module provides the core encryption and decryption functionality using the
//! age encryption format. It supports:
//!
//! - Standard file encryption with ASCII armor format
//! - Inline(SOPS-like) encryption with compact base64 encoding (age:base64...)
//! - Both age native keys and SSH keys
//! - Multiple recipients and identities

use crate::identity::Identity;
use crate::{Error, Recipient, Result};
use once_cell::sync::Lazy;
use std::io::{Read, Write};
use tracing::warn;

/// Prefix for inline encrypted values: "age:"
const INLINE_PREFIX: &str = "age:";

/// Cached regex pattern for matching inline encrypted values.
static INLINE_PATTERN: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(&format!(
        r"{}[A-Za-z0-9+/]+=*",
        regex::escape(INLINE_PREFIX)
    ))
    .expect("Failed to compile inline encryption pattern")
});

/// Helper to convert recipients to age trait objects
#[inline]
fn prepare_recipients(recipients: &[Recipient]) -> Vec<Box<dyn age::Recipient + Send>> {
    recipients.iter().map(|r| r.to_boxed()).collect()
}

/// Helper to convert age errors to our Error type
#[inline]
fn age_error<E: std::fmt::Display>(e: E) -> Error {
    Error::Age(e.to_string())
}

/// Helper to map age decryption errors to our Error type
///
/// Uses type-safe pattern matching on age::DecryptError variants to
/// distinguish between different failure scenarios.
///
/// Note: We don't handle plugin-related errors (MissingPlugin, Plugin)
/// as we don't enable the 'plugin' feature in the age dependency.
#[inline]
fn map_decrypt_error(e: age::DecryptError) -> Error {
    match e {
        // Authentication/key matching failures
        age::DecryptError::NoMatchingKeys => Error::WrongKey,
        age::DecryptError::InvalidMac => Error::WrongKey,
        age::DecryptError::KeyDecryptionFailed => Error::WrongKey,

        // Decryption failures
        age::DecryptError::DecryptionFailed => Error::DecryptionFailed {
            reason: "Age decryption failed".to_string(),
        },
        age::DecryptError::ExcessiveWork { required, target } => Error::DecryptionFailed {
            reason: format!(
                "Excessive work factor: required {}, target {}. \
                 This file was encrypted with a higher work factor than this device can handle.",
                required, target
            ),
        },
        age::DecryptError::InvalidHeader => Error::DecryptionFailed {
            reason: "Invalid age header".to_string(),
        },
        age::DecryptError::UnknownFormat => Error::DecryptionFailed {
            reason: "Unknown age format (possibly from a newer version)".to_string(),
        },

        // I/O errors (pass through)
        age::DecryptError::Io(io_err) => Error::Io(io_err),
    }
}

/// Encrypt data with the given recipients in ASCII armor format.
///
/// Encrypts the provided data using age encryption and returns the result
/// in ASCII-armored format (PEM-like). The output can be decrypted by any
/// of the recipient's corresponding private keys (identities).
///
/// # Arguments
///
/// * `data` - The plaintext data to encrypt
/// * `recipients` - One or more recipients who can decrypt the data
///
/// # Returns
///
/// The encrypted data in ASCII armor format, beginning with
/// `-----BEGIN AGE ENCRYPTED FILE-----`.
///
/// # Errors
///
/// - Returns [`Error::NoRecipients`] if the recipients slice is empty
/// - Returns [`Error::Age`] if encryption fails
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::{encrypt, Identity};
///
/// let identity = Identity::generate();
/// let recipient = identity.to_public();
///
/// let plaintext = b"secret data";
/// let encrypted = encrypt(plaintext, &[recipient]).unwrap();
/// ```
pub fn encrypt(data: &[u8], recipients: &[Recipient]) -> Result<Vec<u8>> {
    if recipients.is_empty() {
        return Err(Error::NoRecipients);
    }

    let boxed_recipients = prepare_recipients(recipients);
    let recipient_refs: Vec<&dyn age::Recipient> = boxed_recipients
        .iter()
        .map(|r| r.as_ref() as &dyn age::Recipient)
        .collect();

    let encryptor = age::Encryptor::with_recipients(recipient_refs.into_iter())
        .map_err(|_| Error::Age("Failed to create encryptor with recipients".to_string()))?;

    let mut encrypted = Vec::new();
    let armor =
        age::armor::ArmoredWriter::wrap_output(&mut encrypted, age::armor::Format::AsciiArmor)
            .map_err(age_error)?;

    let mut writer = encryptor.wrap_output(armor).map_err(age_error)?;
    writer.write_all(data).map_err(age_error)?;
    writer
        .finish()
        .and_then(|armor| armor.finish())
        .map_err(age_error)?;

    Ok(encrypted)
}

/// Decrypt data with the given identities (supports armor and binary formats).
///
/// Decrypts age-encrypted data using one or more identities (private keys).
/// Automatically detects and handles both ASCII-armored and binary formats.
///
/// # Arguments
///
/// * `data` - The encrypted data (either ASCII armor or binary format)
/// * `identities` - One or more identities to try for decryption
///
/// # Returns
///
/// The decrypted plaintext data.
///
/// # Errors
///
/// - Returns [`Error::NoIdentity`] if the identities slice is empty
/// - Returns [`Error::DecryptionFailed`] if decryption fails (wrong key, corrupted data)
/// - Returns [`Error::Age`] for other age-related errors
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::{encrypt, decrypt, Identity};
///
/// let identity = Identity::generate();
/// let recipient = identity.to_public();
///
/// let encrypted = encrypt(b"secret", &[recipient]).unwrap();
/// let decrypted = decrypt(&encrypted, &[identity]).unwrap();
/// assert_eq!(decrypted, b"secret");
/// ```
pub fn decrypt(data: &[u8], identities: &[Identity]) -> Result<Vec<u8>> {
    if identities.is_empty() {
        return Err(Error::NoIdentity);
    }

    // Fast path for single identity (common case)
    if identities.len() == 1 {
        return decrypt_single(data, &identities[0]);
    }

    let age_identities: Vec<&dyn age::Identity> =
        identities.iter().map(|id| id.as_dyn_identity()).collect();

    let mut decrypted = Vec::new();

    // Try armored format first
    if let Ok(decryptor) = age::Decryptor::new(age::armor::ArmoredReader::new(data)) {
        let mut reader = decryptor
            .decrypt(age_identities.iter().copied())
            .map_err(map_decrypt_error)?;
        reader.read_to_end(&mut decrypted).map_err(age_error)?;
    } else {
        // Fall back to binary format
        let decryptor = age::Decryptor::new(data).map_err(age_error)?;
        let mut reader = decryptor
            .decrypt(age_identities.iter().copied())
            .map_err(map_decrypt_error)?;
        reader.read_to_end(&mut decrypted).map_err(age_error)?;
    }

    Ok(decrypted)
}

/// Optimized decryption for single identity (avoids vec allocation)
#[inline]
fn decrypt_single(data: &[u8], identity: &Identity) -> Result<Vec<u8>> {
    let age_identity = identity.as_dyn_identity();
    let mut decrypted = Vec::new();

    // Try armored format first
    if let Ok(decryptor) = age::Decryptor::new(age::armor::ArmoredReader::new(data)) {
        let mut reader = decryptor
            .decrypt(std::iter::once(age_identity))
            .map_err(map_decrypt_error)?;
        reader.read_to_end(&mut decrypted).map_err(age_error)?;
    } else {
        // Fall back to binary format
        let decryptor = age::Decryptor::new(data).map_err(age_error)?;
        let mut reader = decryptor
            .decrypt(std::iter::once(age_identity))
            .map_err(map_decrypt_error)?;
        reader.read_to_end(&mut decrypted).map_err(age_error)?;
    }

    Ok(decrypted)
}

/// Encrypt a string and return the encrypted data.
///
/// Convenience wrapper around [`encrypt`] for string data.
///
/// # Arguments
///
/// * `data` - The plaintext string to encrypt
/// * `recipients` - One or more recipients who can decrypt the data
///
/// # Returns
///
/// The encrypted data in ASCII armor format.
///
/// # Errors
///
/// - Returns [`Error::NoRecipients`] if the recipients slice is empty
/// - Returns [`Error::Age`] if encryption fails
#[inline]
pub fn encrypt_string(data: &str, recipients: &[Recipient]) -> Result<Vec<u8>> {
    encrypt(data.as_bytes(), recipients)
}

/// Decrypt data and return it as a UTF-8 string.
///
/// Convenience wrapper around [`decrypt`] for string data.
///
/// # Arguments
///
/// * `data` - The encrypted data (either ASCII armor or binary format)
/// * `identities` - One or more identities to try for decryption
///
/// # Returns
///
/// The decrypted plaintext string.
///
/// # Errors
///
/// - Returns [`Error::NoIdentity`] if the identities slice is empty
/// - Returns [`Error::DecryptionFailed`] if decryption fails or the decrypted
///   data is not valid UTF-8
/// - Returns [`Error::Age`] for other age-related errors
#[inline]
pub fn decrypt_string(data: &[u8], identities: &[Identity]) -> Result<String> {
    let decrypted = decrypt(data, identities)?;
    String::from_utf8(decrypted).map_err(|e| Error::DecryptionFailed {
        reason: format!("Decrypted data is not valid UTF-8: {}", e),
    })
}

/// Encrypt a string to compact inline format: `age:base64(encrypted_data)`.
///
/// Creates a compact encrypted representation suitable for embedding in
/// configuration files. The format is `age:` followed by base64-encoded
/// binary encrypted data (without ASCII armor overhead).
///
/// This is similar to SOPS inline encryption and is useful for encrypting
/// specific values within configuration files while keeping the structure
/// readable.
///
/// # Arguments
///
/// * `plaintext` - The string to encrypt
/// * `recipients` - One or more recipients who can decrypt the data
///
/// # Returns
///
/// A compact encrypted string in the format `age:base64...`
///
/// # Errors
///
/// - Returns [`Error::NoRecipients`] if the recipients slice is empty
/// - Returns [`Error::Age`] if encryption fails
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::{encrypt_inline, decrypt_inline, Identity};
///
/// let identity = Identity::generate();
/// let recipient = identity.to_public();
///
/// let encrypted = encrypt_inline("secret_password", &[recipient]).unwrap();
/// assert!(encrypted.starts_with("age:"));
///
/// let decrypted = decrypt_inline(&encrypted, &[identity]).unwrap();
/// assert_eq!(decrypted, "secret_password");
/// ```
pub fn encrypt_inline(plaintext: &str, recipients: &[Recipient]) -> Result<String> {
    if recipients.is_empty() {
        return Err(Error::NoRecipients);
    }

    let boxed_recipients = prepare_recipients(recipients);
    let recipient_refs: Vec<&dyn age::Recipient> = boxed_recipients
        .iter()
        .map(|r| r.as_ref() as &dyn age::Recipient)
        .collect();

    let encryptor = age::Encryptor::with_recipients(recipient_refs.into_iter())
        .map_err(|_| Error::Age("Failed to create encryptor with recipients".to_string()))?;

    let mut encrypted = Vec::new();
    let mut writer = encryptor.wrap_output(&mut encrypted).map_err(age_error)?;

    writer.write_all(plaintext.as_bytes()).map_err(age_error)?;
    writer.finish().map_err(age_error)?;

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted);
    Ok(format!("{}{}", INLINE_PREFIX, encoded))
}

/// Decrypt a compact inline encrypted value: `age:base64(encrypted_data)`.
///
/// Decrypts a string previously encrypted with [`encrypt_inline`].
/// The input must start with the `age:` prefix followed by base64-encoded
/// encrypted data.
///
/// # Arguments
///
/// * `ciphertext` - The encrypted string (must start with "age:")
/// * `identities` - One or more identities to try for decryption
///
/// # Returns
///
/// The decrypted plaintext string.
///
/// # Errors
///
/// - Returns [`Error::NoIdentity`] if the identities slice is empty
/// - Returns [`Error::DecryptionFailed`] if the format is invalid, decryption fails,
///   or the decrypted data is not valid UTF-8
///
/// # Examples
///
/// See [`encrypt_inline`] for usage examples.
pub fn decrypt_inline(ciphertext: &str, identities: &[Identity]) -> Result<String> {
    if identities.is_empty() {
        return Err(Error::NoIdentity);
    }

    let base64_data =
        ciphertext
            .strip_prefix(INLINE_PREFIX)
            .ok_or_else(|| Error::DecryptionFailed {
                reason: format!(
                    "Invalid inline encrypted format: expected '{}' prefix",
                    INLINE_PREFIX
                ),
            })?;

    let encrypted_data =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data).map_err(
            |e| Error::DecryptionFailed {
                reason: format!("Invalid base64 encoding: {}", e),
            },
        )?;

    let decrypted = decrypt(&encrypted_data, identities)?;

    String::from_utf8(decrypted).map_err(|e| Error::DecryptionFailed {
        reason: format!("Decrypted data is not valid UTF-8: {}", e),
    })
}

/// Decrypt all inline encrypted values in file content (SOPS-like workflow).
///
/// Scans the input text for all inline encrypted values (matching the pattern
/// `age:base64...`) and decrypts them in place. This enables a SOPS-like workflow
/// where only sensitive values within a configuration file are encrypted, while
/// the overall structure remains readable.
///
/// Values that fail to decrypt are left unchanged and a warning is logged.
///
/// # Performance
///
/// Uses a cached compiled regex for pattern matching, providing significant
/// performance improvement for repeated operations.
///
/// # Arguments
///
/// * `content` - The file content containing zero or more inline encrypted values
/// * `identities` - One or more identities to use for decryption
///
/// # Returns
///
/// The file content with all successfully decrypted inline values replaced
/// with their plaintext equivalents.
///
/// # Errors
///
/// - Returns [`Error::NoIdentity`] if the identities slice is empty
///
/// Note: Individual decryption failures for specific values do not cause
/// this function to error; instead, failed values are left encrypted and
/// a warning is logged.
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::{encrypt_inline, decrypt_file_content, Identity};
///
/// let identity = Identity::generate();
/// let recipient = identity.to_public();
///
/// let encrypted_pw = encrypt_inline("my_secret", &[recipient]).unwrap();
/// let config = format!("database_password = {}", encrypted_pw);
///
/// let decrypted_config = decrypt_file_content(&config, &[identity]).unwrap();
/// assert!(decrypted_config.contains("my_secret"));
/// ```
pub fn decrypt_file_content(content: &str, identities: &[Identity]) -> Result<String> {
    if identities.is_empty() {
        return Err(Error::NoIdentity);
    }

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for mat in INLINE_PATTERN.find_iter(content) {
        result.push_str(&content[last_end..mat.start()]);

        match decrypt_inline(mat.as_str(), identities) {
            Ok(decrypted) => result.push_str(&decrypted),
            Err(e) => {
                warn!("Failed to decrypt value at position {}: {}", mat.start(), e);
                result.push_str(mat.as_str());
            }
        }

        last_end = mat.end();
    }

    result.push_str(&content[last_end..]);
    Ok(result)
}

/// Re-encrypt all inline encrypted values with new recipients (key rotation).
///
/// Scans the input text for all inline encrypted values, decrypts them using
/// the old identities, and re-encrypts them with new recipients. This is useful
/// for key rotation scenarios where you need to change the encryption keys
/// without manually re-entering all secrets.
///
/// Values that fail to decrypt or re-encrypt are left unchanged and a warning
/// is logged.
///
/// # Performance
///
/// Uses a cached compiled regex for pattern matching. However, this operation
/// requires both decryption and encryption for each value, so it is more
/// expensive than simple decryption.
///
/// # Arguments
///
/// * `content` - The file content containing zero or more inline encrypted values
/// * `old_identities` - Identities to decrypt the existing values
/// * `new_recipients` - Recipients to use when re-encrypting
///
/// # Returns
///
/// The file content with all successfully rotated values re-encrypted with
/// the new recipients.
///
/// # Errors
///
/// - Returns [`Error::NoRecipients`] if the new_recipients slice is empty
/// - Returns [`Error::NoIdentity`] if the old_identities slice is empty
///
/// Note: Individual rotation failures for specific values do not cause
/// this function to error; instead, failed values are left unchanged and
/// a warning is logged.
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::{encrypt_inline, encrypt_file_content, Identity};
///
/// let old_identity = Identity::generate();
/// let old_recipient = old_identity.to_public();
///
/// let new_identity = Identity::generate();
/// let new_recipient = new_identity.to_public();
///
/// let encrypted_pw = encrypt_inline("secret", &[old_recipient]).unwrap();
/// let config = format!("password = {}", encrypted_pw);
///
/// // Rotate to new key
/// let rotated = encrypt_file_content(&config, &[old_identity], &[new_recipient]).unwrap();
///
/// // Old key can't decrypt anymore
/// // assert!(decrypt_file_content(&rotated, &[old_identity]).is_err());
///
/// // New key can decrypt
/// // let decrypted = decrypt_file_content(&rotated, &[new_identity]).unwrap();
/// // assert!(decrypted.contains("secret"));
/// ```
pub fn encrypt_file_content(
    content: &str,
    old_identities: &[Identity],
    new_recipients: &[Recipient],
) -> Result<String> {
    if new_recipients.is_empty() {
        return Err(Error::NoRecipients);
    }

    if old_identities.is_empty() {
        return Err(Error::NoIdentity);
    }

    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;

    for mat in INLINE_PATTERN.find_iter(content) {
        result.push_str(&content[last_end..mat.start()]);

        match decrypt_inline(mat.as_str(), old_identities) {
            Ok(plaintext) => match encrypt_inline(&plaintext, new_recipients) {
                Ok(new_ciphertext) => result.push_str(&new_ciphertext),
                Err(e) => {
                    warn!(
                        "Failed to re-encrypt value at position {}: {}",
                        mat.start(),
                        e
                    );
                    result.push_str(mat.as_str());
                }
            },
            Err(e) => {
                warn!(
                    "Failed to decrypt value at position {} during re-encryption: {}",
                    mat.start(),
                    e
                );
                result.push_str(mat.as_str());
            }
        }

        last_end = mat.end();
    }

    result.push_str(&content[last_end..]);
    Ok(result)
}
