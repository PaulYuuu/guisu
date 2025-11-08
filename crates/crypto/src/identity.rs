//! Identity management for age encryption
//!
//! This module handles loading and managing age identities (private keys).

use crate::{Error, Recipient, Result};
use age::secrecy::ExposeSecret;
use age::x25519;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::str::FromStr;
use tracing::warn;

/// An age identity (private key) for decryption
#[derive(Clone)]
pub enum Identity {
    /// Native age x25519 identity
    Age(x25519::Identity),
    /// SSH private key with its corresponding public key recipient
    /// Note: recipient is boxed to reduce enum size (ssh::Recipient is large)
    Ssh {
        identity: age::ssh::Identity,
        recipient: Box<age::ssh::Recipient>,
    },
}

impl Identity {
    /// Generate a new random age identity
    pub fn generate() -> Self {
        let identity = x25519::Identity::generate();
        Self::Age(identity)
    }

    /// Create from SSH identity and recipient
    pub fn from_ssh(identity: age::ssh::Identity, recipient: age::ssh::Recipient) -> Self {
        Self::Ssh {
            identity,
            recipient: Box::new(recipient),
        }
    }

    /// Get the public key (recipient) for this identity
    pub fn to_public(&self) -> Recipient {
        match self {
            Self::Age(identity) => Recipient::from_age(identity.to_public()),
            Self::Ssh { recipient, .. } => Recipient::from_ssh((**recipient).clone()),
        }
    }

    /// Get a reference to the inner identity as a trait object
    pub(crate) fn as_dyn_identity(&self) -> &dyn age::Identity {
        match self {
            Self::Age(identity) => identity,
            Self::Ssh { identity, .. } => identity,
        }
    }
}

impl FromStr for Identity {
    type Err = Error;

    /// Parse an age identity from a string
    fn from_str(s: &str) -> Result<Self> {
        let identity = s
            .parse::<x25519::Identity>()
            .map_err(|e| Error::InvalidIdentity {
                reason: e.to_string(),
                path: "<string>".to_string(),
            })?;
        Ok(Self::Age(identity))
    }
}

impl fmt::Display for Identity {
    /// Get the identity as a string (private key)
    /// Only supported for age identities - SSH identities return a placeholder
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Age(identity) => write!(f, "{}", identity.to_string().expose_secret()),
            Self::Ssh { .. } => write!(f, "[SSH identity]"),
        }
    }
}

/// An identity file containing one or more age identities
pub struct IdentityFile {
    /// Path to the identity file
    path: String,

    /// Loaded identities
    identities: Vec<Identity>,
}

impl IdentityFile {
    /// Load identities from a file
    ///
    /// The file should contain one identity per line in the age format.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();

        let file = fs::File::open(path_ref).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::IdentityNotFound {
                    path: path_str.clone(),
                }
            } else {
                Error::IdentityFile {
                    operation: "read".to_string(),
                    path: path_str.clone(),
                    source: e,
                }
            }
        })?;

        let reader = BufReader::new(file);
        let mut identities = Vec::new();
        let mut line_num = 0;

        for line in reader.lines() {
            line_num += 1;

            let line = line.map_err(|e| Error::IdentityFile {
                operation: "read".to_string(),
                path: path_str.clone(),
                source: e,
            })?;

            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Try to parse as an identity
            match Identity::from_str(line) {
                Ok(identity) => identities.push(identity),
                Err(e) => {
                    // Warn about invalid lines - could indicate configuration errors
                    warn!(
                        "Skipping invalid identity on line {} in {}: {}",
                        line_num, path_str, e
                    );
                    continue;
                }
            }
        }

        if identities.is_empty() {
            return Err(Error::InvalidIdentity {
                reason: "No valid identities found in file".to_string(),
                path: path_str,
            });
        }

        Ok(Self {
            path: path_str,
            identities,
        })
    }

    /// Save identities to a file
    ///
    /// This will overwrite the file if it exists.
    pub fn save<P: AsRef<Path>>(path: P, identities: &[Identity]) -> Result<()> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();

        let mut content = String::new();

        // Add creation timestamp in RFC3339 format (UTC for consistency across machines)
        use chrono::Utc;
        let now = Utc::now();
        content.push_str(&format!("# created: {}\n", now.to_rfc3339()));

        // Add public key(s) and secret key(s)
        for identity in identities {
            let public_key = identity.to_public();
            content.push_str(&format!("# public key: {}\n", public_key));
            content.push_str(&identity.to_string());
            content.push('\n');
        }

        // On Unix, create file with restricted permissions atomically to avoid race condition
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600) // Set permissions on creation - no race condition
                .open(path_ref)
                .map_err(|e| Error::IdentityFile {
                    operation: "write".to_string(),
                    path: path_str.clone(),
                    source: e,
                })?;

            file.write_all(content.as_bytes())
                .map_err(|e| Error::IdentityFile {
                    operation: "write".to_string(),
                    path: path_str,
                    source: e,
                })?;
        }

        // On non-Unix platforms, use regular write (no permission restrictions)
        #[cfg(not(unix))]
        {
            fs::write(path_ref, content).map_err(|e| Error::IdentityFile {
                operation: "write".to_string(),
                path: path_str,
                source: e,
            })?;
        }

        Ok(())
    }

    /// Get the path to the identity file
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get the identities
    pub fn identities(&self) -> &[Identity] {
        &self.identities
    }

    /// Convert all identities to their public recipients
    ///
    /// This is useful when you want to encrypt data to all identities in the file,
    /// or when exporting public keys for team members.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_crypto::IdentityFile;
    ///
    /// let identity_file = IdentityFile::load("~/.config/guisu/key.txt").unwrap();
    /// let recipients = identity_file.to_recipients();
    ///
    /// // Use all recipients for encryption
    /// guisu_crypto::encrypt(b"secret", &recipients).unwrap();
    /// ```
    pub fn to_recipients(&self) -> Vec<Recipient> {
        self.identities
            .iter()
            .map(|identity| identity.to_public())
            .collect()
    }

    /// Write recipients (public keys) to a file or writer
    ///
    /// This is useful for sharing public keys with team members who need to
    /// encrypt files that you can decrypt. The output contains only public keys,
    /// never private keys, so it's safe to share.
    ///
    /// # Arguments
    ///
    /// * `output` - Where to write the recipients (file, stdout, etc.)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use guisu_crypto::IdentityFile;
    /// use std::fs::File;
    ///
    /// let identity_file = IdentityFile::load("~/.config/guisu/key.txt").unwrap();
    ///
    /// // Write to file
    /// let output = File::create("recipients.txt").unwrap();
    /// identity_file.write_recipients_file(output).unwrap();
    ///
    /// // Or write to stdout
    /// identity_file.write_recipients_file(std::io::stdout()).unwrap();
    /// ```
    pub fn write_recipients_file<W: Write>(&self, mut output: W) -> Result<()> {
        for recipient in self.to_recipients() {
            writeln!(output, "{}", recipient)?;
        }

        Ok(())
    }
}

/// Load identities from a file (supports both age and SSH keys)
///
/// # Arguments
///
/// * `path` - Path to the identity file
/// * `is_ssh` - If true, treat as SSH private key; otherwise as age identity file
///
/// # Examples
///
/// ```no_run
/// use guisu_crypto::load_identities;
/// use std::path::Path;
///
/// // Load age identity
/// let identities = load_identities(Path::new("~/.config/guisu/key.txt"), false).unwrap();
///
/// // Load SSH key
/// let identities = load_identities(Path::new("~/.ssh/id_ed25519"), true).unwrap();
/// ```
pub fn load_identities<P: AsRef<Path>>(path: P, is_ssh: bool) -> Result<Vec<Identity>> {
    let path_ref = path.as_ref();
    let path_str = path_ref.to_string_lossy().to_string();

    if is_ssh {
        // Load SSH private key
        let file = fs::File::open(path_ref).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::IdentityNotFound {
                    path: path_str.clone(),
                }
            } else {
                Error::IdentityFile {
                    operation: "read".to_string(),
                    path: path_str.clone(),
                    source: e,
                }
            }
        })?;

        let reader = BufReader::new(file);

        // Parse as SSH identity using age's SSH support
        // Note: from_buffer returns a single Identity enum, not a Vec
        let _ssh_identity_check =
            age::ssh::Identity::from_buffer(reader, None).map_err(|e| Error::InvalidIdentity {
                reason: format!("Failed to parse SSH key: {}", e),
                path: path_str.clone(),
            })?;

        // Load the corresponding public key file
        // Try appending .pub to the path
        let pub_key_path = path_ref.to_string_lossy().to_string() + ".pub";
        let pub_key_content = fs::read_to_string(&pub_key_path).map_err(|_| Error::InvalidIdentity {
            reason: format!(
                "SSH public key file not found: {}\n\
                 For SSH key encryption, the public key file (.pub) must exist alongside the private key.",
                pub_key_path
            ),
            path: path_str.clone(),
        })?;

        // Parse SSH public key - need to use str::parse with FromStr trait
        use std::str::FromStr;
        let ssh_recipient = age::ssh::Recipient::from_str(&pub_key_content).map_err(|e| {
            Error::InvalidIdentity {
                reason: format!("Failed to parse SSH public key: {:?}", e),
                path: pub_key_path.clone(),
            }
        })?;

        // For SSH, we can only create one Identity per file
        // Parse the SSH key as bytes to get the concrete type
        let content = fs::read(path_ref).map_err(|e| Error::IdentityFile {
            operation: "read".to_string(),
            path: path_str.clone(),
            source: e,
        })?;

        let ssh_identity = age::ssh::Identity::from_buffer(&content[..], None).map_err(|e| {
            Error::InvalidIdentity {
                reason: format!("Failed to parse SSH key: {}", e),
                path: path_str.clone(),
            }
        })?;

        // Create our Identity wrapper with the SSH identity and recipient
        // Note: from_buffer returns a single identity for SSH keys
        let identity = Identity::from_ssh(ssh_identity, ssh_recipient);

        Ok(vec![identity])
    } else {
        // Load age identity file
        let identity_file = IdentityFile::load(path_ref)?;
        Ok(identity_file.identities().to_vec())
    }
}
