//! Crypto adapter that implements the Decryptor trait from engine

use crate::content::Decryptor;
use guisu_crypto::Identity;
use std::sync::Arc;
use thiserror::Error;

/// Error type for crypto adapter
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error(transparent)]
    Crypto(#[from] guisu_crypto::Error),
}

/// Adapter that wraps crypto functions to implement engine::content::Decryptor
pub struct CryptoDecryptorAdapter {
    identity: Arc<Identity>,
}

impl CryptoDecryptorAdapter {
    /// Create a new crypto adapter with the given identity
    pub fn new(identity: Identity) -> Self {
        Self::from_arc(Arc::new(identity))
    }

    /// Create a new crypto adapter from an Arc<Identity> (zero-copy)
    pub fn from_arc(identity: Arc<Identity>) -> Self {
        Self { identity }
    }

    /// Get a reference to the identity
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

impl Decryptor for CryptoDecryptorAdapter {
    type Error = CryptoError;

    fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>, Self::Error> {
        guisu_crypto::decrypt(encrypted, &[self.identity.as_ref().clone()]).map_err(Into::into)
    }

    fn decrypt_inline(&self, text: &str) -> Result<String, Self::Error> {
        guisu_crypto::decrypt_inline(text, &[self.identity.as_ref().clone()]).map_err(Into::into)
    }
}

impl Clone for CryptoDecryptorAdapter {
    fn clone(&self) -> Self {
        Self {
            identity: Arc::clone(&self.identity),
        }
    }
}
