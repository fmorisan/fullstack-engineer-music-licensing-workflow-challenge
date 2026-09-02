//! RSA keypair fixtures for RS256 token tests.

use rsa::pkcs8::EncodePrivateKey;
use rsa::pkcs8::EncodePublicKey;
use rsa::pkcs8::LineEnding;

/// A generated RS256 keypair in PEM form.
#[derive(Debug, Clone)]
pub struct RsaKeyPair {
    /// PKCS#8 private key PEM.
    pub private_pem: String,
    /// SubjectPublicKeyInfo public key PEM.
    pub public_pem: String,
}

impl RsaKeyPair {
    /// Generate a fresh 2048-bit keypair.
    ///
    /// # Panics
    ///
    /// Panics on key-generation failure; tests cannot proceed without keys.
    pub fn generate() -> Self {
        let mut rng = rand_core::OsRng;
        let private = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("rsa key generation failed");
        let private_pem = private
            .to_pkcs8_pem(LineEnding::LF)
            .expect("private pem encode failed")
            .to_string();
        let public_pem = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .expect("public pem encode failed");
        Self {
            private_pem,
            public_pem,
        }
    }
}
