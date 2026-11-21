//! Primitives du second facteur : aléa système, HMAC-SHA1 (RFC 2104), chiffrement AES-256-GCM du secret au
//! repos et QR code SVG.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use fineme_application::ports::{ProviderError, TotpSupport};
use hmac::{Hmac, Mac};
use qrcode::QrCode;
use qrcode::render::svg;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// Préfixe de version du format chiffré (`v1:<hex(nonce ‖ chiffré)>`).
const SEALED_PREFIX: &str = "v1:";
const NONCE_BYTES: usize = 12;
/// Séparation de domaine : la clé de chiffrement n'est jamais la clé de signature des JWT elle-même.
const KEY_CONTEXT: &[u8] = b"fineme:totp-secret:v1:";

/// Clé de chiffrement dérivée de `JWT_SECRET` (SHA-256 avec contexte). Changer `JWT_SECRET` rend illisibles les
/// secrets enregistrés : les codes de secours ou `RECOVERY_TOKEN` restent alors le seul recours.
pub struct TotpCrypto {
    cipher: Aes256Gcm,
}

impl TotpCrypto {
    pub fn new(jwt_secret: &str) -> Self {
        let key: [u8; 32] = Sha256::new()
            .chain_update(KEY_CONTEXT)
            .chain_update(jwt_secret.as_bytes())
            .finalize()
            .into();
        Self {
            cipher: Aes256Gcm::new(&key.into()),
        }
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

impl TotpSupport for TotpCrypto {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, ProviderError> {
        let mut bytes = vec![0u8; len];
        getrandom::fill(&mut bytes)
            .map_err(|e| ProviderError(format!("aléa système indisponible : {e}")))?;
        Ok(bytes)
    }

    fn hmac_sha1(&self, key: &[u8], message: &[u8]) -> [u8; 20] {
        // HMAC accepte une clé de toute longueur : la construction ne peut pas échouer.
        match <Hmac<Sha1> as KeyInit>::new_from_slice(key) {
            Ok(mut mac) => {
                mac.update(message);
                mac.finalize().into_bytes().into()
            }
            Err(_) => [0; 20],
        }
    }

    fn seal(&self, secret: &[u8]) -> Result<String, ProviderError> {
        let nonce_bytes: [u8; NONCE_BYTES] = self
            .random_bytes(NONCE_BYTES)?
            .try_into()
            .map_err(|_| ProviderError("nonce invalide".into()))?;
        let nonce = Nonce::from(nonce_bytes);
        let encrypted = self
            .cipher
            .encrypt(&nonce, secret)
            .map_err(|_| ProviderError("chiffrement du secret impossible".into()))?;
        Ok(format!(
            "{SEALED_PREFIX}{}{}",
            to_hex(&nonce_bytes),
            to_hex(&encrypted)
        ))
    }

    fn open(&self, sealed: &str) -> Result<Vec<u8>, ProviderError> {
        const UNREADABLE: &str = "secret chiffré illisible";
        let bytes = sealed
            .strip_prefix(SEALED_PREFIX)
            .and_then(from_hex)
            .filter(|bytes| bytes.len() > NONCE_BYTES)
            .ok_or_else(|| ProviderError(UNREADABLE.into()))?;
        let (nonce_bytes, encrypted) = bytes.split_at(NONCE_BYTES);
        let nonce_bytes: [u8; NONCE_BYTES] = nonce_bytes
            .try_into()
            .map_err(|_| ProviderError(UNREADABLE.into()))?;
        self.cipher
            .decrypt(&Nonce::from(nonce_bytes), encrypted)
            .map_err(|_| ProviderError(UNREADABLE.into()))
    }

    fn qr_svg(&self, text: &str) -> Result<String, ProviderError> {
        let code = QrCode::new(text.as_bytes())
            .map_err(|e| ProviderError(format!("QR code impossible : {e}")))?;
        Ok(code
            .render::<svg::Color<'_>>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use fineme_domain::totp::{counter_message, time_step, truncate};

    /// Code TOTP à 8 chiffres (format des vecteurs de la RFC 6238) calculé avec l'adaptateur.
    fn rfc_code(crypto: &TotpCrypto, secret: &[u8], seconds: i64) -> u32 {
        let at = Utc.timestamp_opt(seconds, 0).unwrap();
        let mac = crypto.hmac_sha1(secret, &counter_message(time_step(at)));
        let offset = usize::from(mac[19] & 0x0f);
        let binary = u32::from_be_bytes([
            mac[offset] & 0x7f,
            mac[offset + 1],
            mac[offset + 2],
            mac[offset + 3],
        ]);
        binary % 100_000_000
    }

    #[test]
    fn matches_rfc_6238_sha1_test_vectors() {
        let crypto = TotpCrypto::new("secret-de-test");
        let secret = b"12345678901234567890";
        // RFC 6238, annexe B (SHA-1) : codes à 8 chiffres ; nos 6 chiffres en sont les derniers.
        for (seconds, expected) in [
            (59, 94_287_082),
            (1_111_111_109, 7_081_804),
            (1_111_111_111, 14_050_471),
            (1_234_567_890, 89_005_924),
            (2_000_000_000, 69_279_037),
            (20_000_000_000, 65_353_130),
        ] {
            assert_eq!(
                rfc_code(&crypto, secret, seconds),
                expected,
                "T = {seconds}"
            );
            let at = Utc.timestamp_opt(seconds, 0).unwrap();
            let mac = crypto.hmac_sha1(secret, &counter_message(time_step(at)));
            assert_eq!(truncate(&mac), expected % 1_000_000, "T = {seconds}");
        }
    }

    #[test]
    fn matches_rfc_4226_hotp_vectors() {
        let crypto = TotpCrypto::new("secret-de-test");
        let expected = [
            755_224, 287_082, 359_152, 969_429, 338_314, 254_676, 287_922, 162_583, 399_871,
            520_489,
        ];
        for (counter, code) in expected.into_iter().enumerate() {
            let mac = crypto.hmac_sha1(b"12345678901234567890", &(counter as u64).to_be_bytes());
            assert_eq!(truncate(&mac), code, "compteur {counter}");
        }
    }

    #[test]
    fn sealed_secret_round_trips_and_resists_tampering_and_key_change() {
        let crypto = TotpCrypto::new("secret-de-test");
        let secret = crypto.random_bytes(20).unwrap();
        let sealed = crypto.seal(&secret).unwrap();
        assert!(sealed.starts_with("v1:"));
        assert!(!sealed.contains(&to_hex(&secret)));
        assert_ne!(sealed, crypto.seal(&secret).unwrap(), "nonce aléatoire");
        assert_eq!(crypto.open(&sealed).unwrap(), secret);
        let mut tampered = sealed.clone();
        let last = tampered.pop().unwrap();
        tampered.push(if last == '0' { '1' } else { '0' });
        assert!(crypto.open(&tampered).is_err());
        assert!(TotpCrypto::new("autre-secret").open(&sealed).is_err());
        for invalid in ["", "v1:", "v1:zz", "v2:00"] {
            assert!(crypto.open(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn qr_code_is_an_svg_document() {
        let svg = TotpCrypto::new("s")
            .qr_svg("otpauth://totp/FineMe:a%40b.test?secret=GEZDGNBV")
            .unwrap();
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("<script"));
    }
}
