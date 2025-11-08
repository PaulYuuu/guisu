//! Recipient types for age encryption
//!
//! This module defines a unified Recipient type that can represent both
//! native age recipients and SSH-based recipients.

use age::{ssh, x25519};
use std::fmt;
use std::str::FromStr;

/// A recipient for age encryption
///
/// This enum wraps both native age x25519 recipients and SSH-based recipients,
/// allowing encryption to work with either key type.
#[derive(Clone)]
pub enum Recipient {
    /// Native age x25519 recipient
    Age(x25519::Recipient),
    /// SSH public key recipient
    Ssh(ssh::Recipient),
}

impl Recipient {
    /// Convert to a boxed trait object for use with age encryption
    pub fn to_boxed(&self) -> Box<dyn age::Recipient + Send> {
        match self {
            Self::Age(r) => Box::new(r.clone()),
            Self::Ssh(r) => Box::new(r.clone()),
        }
    }

    /// Create from an age x25519 recipient
    pub fn from_age(recipient: x25519::Recipient) -> Self {
        Self::Age(recipient)
    }

    /// Create from an SSH recipient
    pub fn from_ssh(recipient: ssh::Recipient) -> Self {
        Self::Ssh(recipient)
    }
}

impl From<x25519::Recipient> for Recipient {
    fn from(r: x25519::Recipient) -> Self {
        Self::Age(r)
    }
}

impl From<ssh::Recipient> for Recipient {
    fn from(r: ssh::Recipient) -> Self {
        Self::Ssh(r)
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Age(r) => write!(f, "{}", r),
            Self::Ssh(r) => write!(f, "{}", r),
        }
    }
}

impl FromStr for Recipient {
    type Err = crate::Error;

    /// Parse a recipient from a string
    ///
    /// This will try to parse as an age recipient first, then as an SSH recipient.
    fn from_str(s: &str) -> crate::Result<Self> {
        // Try parsing as age recipient first (starts with "age1")
        if let Ok(recipient) = s.parse::<x25519::Recipient>() {
            return Ok(Self::Age(recipient));
        }

        // Try parsing as SSH recipient
        if let Ok(recipient) = s.parse::<ssh::Recipient>() {
            return Ok(Self::Ssh(recipient));
        }

        Err(crate::Error::InvalidRecipient {
            recipient: s.to_string(),
            reason: "Expected age1... or ssh-... format".to_string(),
        })
    }
}
