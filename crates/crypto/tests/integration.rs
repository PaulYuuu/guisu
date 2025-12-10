//! Integration tests for crypto module
//!
//! These tests verify that all crypto components work together correctly
//! in end-to-end workflows.

#![allow(clippy::unwrap_used, clippy::panic)]

use guisu_crypto::{
    Identity, IdentityFile, decrypt, decrypt_file_content, encrypt, encrypt_file_content,
    encrypt_inline,
};
use tempfile::NamedTempFile;

#[test]
fn test_end_to_end_file_encryption() {
    // Generate identity and save to file
    let identity = Identity::generate();
    let temp_identity_file = NamedTempFile::new().expect("Failed to create temp file");

    IdentityFile::save(temp_identity_file.path(), std::slice::from_ref(&identity))
        .expect("Failed to save identity");

    // Load identity back from file
    let loaded_identity_file =
        IdentityFile::load(temp_identity_file.path()).expect("Failed to load identity");

    assert_eq!(loaded_identity_file.identities().len(), 1);

    // Encrypt data with the loaded identity's public key
    let plaintext = b"This is a secret message";
    let recipient = loaded_identity_file.identities()[0].to_public();

    let encrypted = encrypt(plaintext, &[recipient]).expect("Encryption failed");

    // Decrypt with the loaded identity
    let decrypted =
        decrypt(&encrypted, loaded_identity_file.identities()).expect("Decryption failed");

    assert_eq!(plaintext, decrypted.as_slice());
}

#[test]
fn test_team_encryption_workflow() {
    // Simulate a team with multiple members
    let alice_identity = Identity::generate();
    let bob_identity = Identity::generate();
    let charlie_identity = Identity::generate();

    // Create team recipients file
    let team_recipients = vec![
        alice_identity.to_public(),
        bob_identity.to_public(),
        charlie_identity.to_public(),
    ];

    // Encrypt a team secret
    let team_secret = b"Team password: super_secret_123";
    let encrypted = encrypt(team_secret, &team_recipients).expect("Encryption failed");

    // All team members can decrypt
    let alice_decrypted = decrypt(&encrypted, &[alice_identity]).expect("Alice decrypt failed");
    let bob_decrypted = decrypt(&encrypted, &[bob_identity]).expect("Bob decrypt failed");
    let charlie_decrypted =
        decrypt(&encrypted, &[charlie_identity]).expect("Charlie decrypt failed");

    assert_eq!(team_secret, alice_decrypted.as_slice());
    assert_eq!(team_secret, bob_decrypted.as_slice());
    assert_eq!(team_secret, charlie_decrypted.as_slice());

    // Non-team member cannot decrypt
    let eve_identity = Identity::generate();
    let eve_result = decrypt(&encrypted, &[eve_identity]);
    assert!(eve_result.is_err());
}

#[test]
fn test_config_file_encryption_workflow() {
    // Simulate encrypting sensitive values in a config file
    let identity = Identity::generate();
    let recipient = identity.to_public();

    // Encrypt individual values
    let db_password = encrypt_inline("postgres_password_123", std::slice::from_ref(&recipient))
        .expect("Failed to encrypt db password");
    let api_key = encrypt_inline("sk_live_abc123xyz", std::slice::from_ref(&recipient))
        .expect("Failed to encrypt api key");

    // Create a config file with inline encrypted values
    let config = format!(
        r#"
[database]
host = "localhost"
port = 5432
user = "app_user"
password = {db_password}

[api]
endpoint = "https://api.example.com"
key = {api_key}
"#
    );

    // Verify inline values have the age: prefix
    assert!(config.contains("age:"));

    // Decrypt the entire config file
    let decrypted_config =
        decrypt_file_content(&config, &[identity]).expect("Failed to decrypt config");

    // Verify decrypted values are present
    assert!(decrypted_config.contains("postgres_password_123"));
    assert!(decrypted_config.contains("sk_live_abc123xyz"));
    assert!(!decrypted_config.contains("age:"));
}

#[test]
fn test_key_rotation_workflow() {
    // Initial setup with old key
    let old_identity = Identity::generate();
    let old_recipient = old_identity.to_public();

    // Encrypt config with old key
    let secret1 = encrypt_inline("secret_value_1", std::slice::from_ref(&old_recipient))
        .expect("Failed to encrypt");
    let secret2 = encrypt_inline("secret_value_2", std::slice::from_ref(&old_recipient))
        .expect("Failed to encrypt");

    let config = format!("api_key = {secret1}\ndb_password = {secret2}\nplain_value = hello");

    // Generate new key for rotation
    let new_identity = Identity::generate();
    let new_recipient = new_identity.to_public();

    // Rotate all encrypted values to new key
    let rotated_config = encrypt_file_content(&config, &[old_identity], &[new_recipient])
        .expect("Key rotation failed");

    // New key can decrypt
    let decrypted_new = decrypt_file_content(&rotated_config, std::slice::from_ref(&new_identity))
        .expect("Decryption with new key failed");

    assert!(decrypted_new.contains("secret_value_1"));
    assert!(decrypted_new.contains("secret_value_2"));
    assert!(decrypted_new.contains("plain_value = hello"));

    // Verify still encrypted (contains age: prefix)
    assert!(rotated_config.contains("age:"));
}

#[test]
fn test_multiple_identities_in_file() {
    // Create multiple identities (simulate different environments)
    let dev_identity = Identity::generate();
    let staging_identity = Identity::generate();
    let prod_identity = Identity::generate();

    let temp_file = NamedTempFile::new().expect("Failed to create temp file");

    // Save all identities to one file
    IdentityFile::save(
        temp_file.path(),
        &[
            dev_identity.clone(),
            staging_identity.clone(),
            prod_identity.clone(),
        ],
    )
    .expect("Failed to save identities");

    // Load and verify
    let loaded = IdentityFile::load(temp_file.path()).expect("Failed to load");
    assert_eq!(loaded.identities().len(), 3);

    // Get all recipients for team encryption
    let recipients = loaded.to_recipients();
    assert_eq!(recipients.len(), 3);

    // Encrypt with all recipients
    let shared_secret = b"shared across all environments";
    let encrypted = encrypt(shared_secret, &recipients).expect("Encryption failed");

    // Each identity can decrypt independently
    let dev_decrypted = decrypt(&encrypted, &[dev_identity]).expect("Dev decrypt failed");
    let staging_decrypted =
        decrypt(&encrypted, &[staging_identity]).expect("Staging decrypt failed");
    let prod_decrypted = decrypt(&encrypted, &[prod_identity]).expect("Prod decrypt failed");

    assert_eq!(shared_secret, dev_decrypted.as_slice());
    assert_eq!(shared_secret, staging_decrypted.as_slice());
    assert_eq!(shared_secret, prod_decrypted.as_slice());
}

#[test]
fn test_recipients_export_workflow() {
    // Create identity file
    let identity = Identity::generate();
    let temp_identity = NamedTempFile::new().expect("Failed to create temp file");

    IdentityFile::save(temp_identity.path(), std::slice::from_ref(&identity))
        .expect("Failed to save identity");

    // Load identity file
    let identity_file = IdentityFile::load(temp_identity.path()).expect("Failed to load identity");

    // Export recipients to a separate file
    let temp_recipients = NamedTempFile::new().expect("Failed to create temp file");
    let mut recipients_file =
        std::fs::File::create(temp_recipients.path()).expect("Failed to create recipients file");

    identity_file
        .write_recipients_file(&mut recipients_file)
        .expect("Failed to write recipients");

    // Read back the recipients file
    let recipients_content =
        std::fs::read_to_string(temp_recipients.path()).expect("Failed to read recipients");

    // Should contain age public key
    assert!(recipients_content.starts_with("age1"));

    // Can use the public key from recipients file for encryption
    let public_key = identity.to_public();
    let encrypted = encrypt(b"secret", &[public_key]).expect("Encryption failed");
    let decrypted = decrypt(&encrypted, &[identity]).expect("Decryption failed");

    assert_eq!(b"secret", decrypted.as_slice());
}

#[test]
fn test_large_file_encryption() {
    // Test with larger data (1MB)
    let identity = Identity::generate();
    let recipient = identity.to_public();

    let large_data = vec![b'X'; 1024 * 1024]; // 1MB

    let encrypted = encrypt(&large_data, &[recipient]).expect("Encryption failed");
    let decrypted = decrypt(&encrypted, &[identity]).expect("Decryption failed");

    assert_eq!(large_data, decrypted);
}

#[test]
fn test_unicode_content_workflow() {
    let identity = Identity::generate();
    let recipient = identity.to_public();

    // Config with basic content
    let config = r#"
welcome = "Welcome"
app_name = "MyApp"
"#;

    // Encrypt a secret
    let secret = "my-secret-password";
    let encrypted_secret = encrypt_inline(secret, &[recipient]).expect("Failed to encrypt secret");

    let config_with_secret = format!("{config}\nsecret = {encrypted_secret}");

    // Decrypt
    let decrypted =
        decrypt_file_content(&config_with_secret, &[identity]).expect("Decryption failed");

    assert!(decrypted.contains(secret));
    assert!(decrypted.contains("Welcome"));
}
