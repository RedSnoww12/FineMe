//! Primitives des jetons d'accès : aléa système (getrandom) et empreinte SHA-256.

use fineme_application::ports::{AccessTokenCrypto, ProviderError};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, Default)]
pub struct AccessTokenDigest;

impl AccessTokenCrypto for AccessTokenDigest {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, ProviderError> {
        let mut bytes = vec![0u8; len];
        getrandom::fill(&mut bytes)
            .map_err(|e| ProviderError(format!("aléa système indisponible : {e}")))?;
        Ok(bytes)
    }

    fn sha256(&self, data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_reference_vector() {
        let digest = AccessTokenDigest.sha256(b"abc");
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn random_bytes_have_the_requested_length_and_vary() {
        let a = AccessTokenDigest.random_bytes(32).unwrap();
        let b = AccessTokenDigest.random_bytes(32).unwrap();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
