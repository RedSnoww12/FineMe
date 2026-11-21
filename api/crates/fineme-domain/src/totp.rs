//! Second facteur : règles pures des codes TOTP (RFC 6238 / RFC 4226) et des codes de secours.
//! Le calcul HMAC-SHA1 et l'aléa sont fournis par l'appelant : ce module ne fait aucune E/S.

use chrono::{DateTime, Utc};

/// Durée d'un pas TOTP, en secondes.
pub const STEP_SECONDS: i64 = 30;
/// Nombre de chiffres d'un code.
pub const DIGITS: u32 = 6;
/// Pas tolérés avant et après le pas courant (horloge du téléphone décalée).
pub const SKEW_STEPS: i64 = 1;
/// Longueur du secret généré (recommandation RFC 4226 : 160 bits pour HMAC-SHA1).
pub const SECRET_BYTES: usize = 20;
/// Nombre de codes de secours délivrés à l'activation ou à la régénération.
pub const RECOVERY_CODE_COUNT: usize = 10;
/// Caractères d'un code de secours (hors tiret de lecture).
pub const RECOVERY_CODE_LEN: usize = 10;

const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
/// Alphabet des codes de secours : 32 caractères sans ambiguïté de lecture (ni o, ni l, ni i, ni 0).
const RECOVERY_ALPHABET: &[u8; 32] = b"abcdefghjkmnpqrstuvwxyz123456789";

/// Secret partagé avec l'application d'authentification. Trop court, il ne se construit pas.
#[derive(Clone, PartialEq, Eq)]
pub struct TotpSecret(Vec<u8>);

impl std::fmt::Debug for TotpSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TotpSecret(…)")
    }
}

impl TotpSecret {
    /// Au moins 128 bits (RFC 4226, § 4), sinon `None`.
    pub fn new(bytes: Vec<u8>) -> Option<Self> {
        (bytes.len() >= 16).then_some(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encodage base32 (RFC 4648) sans remplissage, attendu par les applications d'authentification.
    pub fn to_base32(&self) -> String {
        let mut out = String::with_capacity(self.0.len().div_ceil(5) * 8);
        let (mut buffer, mut bits) = (0u32, 0u32);
        for &byte in &self.0 {
            buffer = (buffer << 8) | u32::from(byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(char::from(BASE32[((buffer >> bits) & 31) as usize]));
            }
        }
        if bits > 0 {
            out.push(char::from(BASE32[((buffer << (5 - bits)) & 31) as usize]));
        }
        out
    }

    /// URI `otpauth://totp/…` à transformer en QR code (format « Key Uri » des applications d'authentification).
    pub fn otpauth_uri(&self, issuer: &str, account: &str) -> String {
        let issuer = percent_encode(issuer);
        format!(
            "otpauth://totp/{issuer}:{account}?secret={secret}&issuer={issuer}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
            account = percent_encode(account),
            secret = self.to_base32(),
        )
    }
}

/// Encodage pour URI : seuls les caractères non réservés (RFC 3986) restent tels quels.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Code TOTP saisi : exactement 6 chiffres (espaces ignorés).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotpCode(u32);

impl TotpCode {
    pub fn parse(input: &str) -> Option<Self> {
        let digits: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        if digits.len() != DIGITS as usize || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok().map(Self)
    }

    pub fn value(self) -> u32 {
        self.0
    }
}

/// Pas TOTP contenant l'instant `at` (compteur de la RFC 6238, T0 = 0).
pub fn time_step(at: DateTime<Utc>) -> i64 {
    at.timestamp().div_euclid(STEP_SECONDS)
}

/// Compteur HOTP d'un pas, encodé en 8 octets gros-boutistes (message du HMAC).
pub fn counter_message(step: i64) -> [u8; 8] {
    step.to_be_bytes()
}

/// Troncature dynamique (RFC 4226, § 5.3) d'un HMAC-SHA1 en code à 6 chiffres.
pub fn truncate(mac: &[u8; 20]) -> u32 {
    let offset = usize::from(mac[19] & 0x0f);
    let binary = u32::from_be_bytes([
        mac[offset] & 0x7f,
        mac[offset + 1],
        mac[offset + 2],
        mac[offset + 3],
    ]);
    binary % 10u32.pow(DIGITS)
}

/// Pas accepté pour `code` à l'instant `at`, parmi le pas courant et ses voisins tolérés, en refusant tout pas
/// déjà utilisé ou antérieur (`last_used`, anti-rejeu). `code_at` calcule le code d'un pas.
pub fn matching_step(
    code: TotpCode,
    at: DateTime<Utc>,
    last_used: Option<i64>,
    code_at: impl Fn(i64) -> u32,
) -> Option<i64> {
    let current = time_step(at);
    (current - SKEW_STEPS..=current + SKEW_STEPS)
        .filter(|step| last_used.is_none_or(|used| *step > used))
        .find(|step| code_at(*step) == code.value())
}

/// Code de secours `xxxxx-xxxxx` tiré de 10 octets aléatoires (chaque octet donne un caractère, sans biais :
/// 256 est un multiple de 32).
pub fn recovery_code_from_bytes(bytes: &[u8; RECOVERY_CODE_LEN]) -> String {
    let chars: String = bytes
        .iter()
        .map(|byte| char::from(RECOVERY_ALPHABET[usize::from(byte % 32)]))
        .collect();
    let (left, right) = chars.split_at(RECOVERY_CODE_LEN / 2);
    format!("{left}-{right}")
}

/// Forme canonique d'un code de secours saisi (casse, espaces et tirets ignorés), ou `None` s'il ne peut pas
/// en être un.
pub fn normalize_recovery_code(input: &str) -> Option<String> {
    let chars: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_lowercase)
        .collect();
    let valid =
        chars.len() == RECOVERY_CODE_LEN && chars.bytes().all(|b| RECOVERY_ALPHABET.contains(&b));
    valid.then(|| {
        let (left, right) = chars.split_at(RECOVERY_CODE_LEN / 2);
        format!("{left}-{right}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).unwrap()
    }

    #[test]
    fn a_short_secret_cannot_be_built() {
        assert!(TotpSecret::new(vec![0; 15]).is_none());
        assert!(TotpSecret::new(vec![0; 16]).is_some());
        assert_eq!(
            format!("{:?}", TotpSecret::new(vec![7; 20])),
            "Some(TotpSecret(…))"
        );
    }

    #[test]
    fn base32_matches_rfc_4648() {
        // Secret des vecteurs de la RFC 6238 (« 12345678901234567890 »).
        let secret = TotpSecret::new(b"12345678901234567890".to_vec()).unwrap();
        assert_eq!(secret.to_base32(), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        // Longueur non multiple de 5 : dernier groupe complété par des zéros, sans remplissage « = ».
        let odd = TotpSecret::new(b"foobarfoobarfoobar".to_vec()).unwrap();
        assert_eq!(odd.to_base32(), "MZXW6YTBOJTG633CMFZGM33PMJQXE");
    }

    #[test]
    fn otpauth_uri_encodes_issuer_and_account() {
        let secret = TotpSecret::new(b"12345678901234567890".to_vec()).unwrap();
        assert_eq!(
            secret.otpauth_uri("FineMe", "camille+test@exemple.test"),
            "otpauth://totp/FineMe:camille%2Btest%40exemple.test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ\
             &issuer=FineMe&algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn codes_are_six_digits_spaces_ignored() {
        assert_eq!(
            TotpCode::parse(" 123 456 ").map(TotpCode::value),
            Some(123_456)
        );
        assert_eq!(TotpCode::parse("000042").map(TotpCode::value), Some(42));
        for invalid in [
            "12345",
            "1234567",
            "12345a",
            "",
            "abcde-fghjk",
            "１２３４５６",
        ] {
            assert_eq!(TotpCode::parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn steps_follow_rfc_6238() {
        assert_eq!(time_step(at(59)), 1);
        assert_eq!(time_step(at(60)), 2);
        assert_eq!(time_step(at(1_111_111_109)), 0x023523EC);
        assert_eq!(time_step(at(20_000_000_000)), 0x27BC86AA);
        assert_eq!(
            counter_message(0x023523EC),
            [0, 0, 0, 0, 0x02, 0x35, 0x23, 0xEC]
        );
    }

    #[test]
    fn truncation_matches_rfc_4226_example() {
        // RFC 4226, § 5.4 : HMAC 1f8698690e02ca16618550ef7f19da8e945b555a → 872921.
        let mac = [
            0x1f, 0x86, 0x98, 0x69, 0x0e, 0x02, 0xca, 0x16, 0x61, 0x85, 0x50, 0xef, 0x7f, 0x19,
            0xda, 0x8e, 0x94, 0x5b, 0x55, 0x5a,
        ];
        assert_eq!(truncate(&mac), 872_921);
    }

    /// Générateur factice : le code d'un pas est le pas lui-même.
    fn code_at(step: i64) -> u32 {
        u32::try_from(step % 1_000_000).unwrap()
    }

    fn code(step: i64) -> TotpCode {
        TotpCode::parse(&format!("{:06}", code_at(step))).unwrap()
    }

    #[test]
    fn one_step_of_skew_is_tolerated_on_each_side() {
        let now = at(300 * 30 + 10); // pas 300
        assert_eq!(matching_step(code(300), now, None, code_at), Some(300));
        assert_eq!(matching_step(code(299), now, None, code_at), Some(299));
        assert_eq!(matching_step(code(301), now, None, code_at), Some(301));
        assert_eq!(matching_step(code(298), now, None, code_at), None);
        assert_eq!(matching_step(code(302), now, None, code_at), None);
    }

    #[test]
    fn a_used_step_or_an_older_one_is_refused() {
        let now = at(300 * 30);
        assert_eq!(matching_step(code(300), now, Some(300), code_at), None);
        assert_eq!(matching_step(code(299), now, Some(300), code_at), None);
        assert_eq!(matching_step(code(301), now, Some(300), code_at), Some(301));
        assert_eq!(matching_step(code(300), now, Some(299), code_at), Some(300));
    }

    #[test]
    fn recovery_codes_are_readable_and_normalized() {
        let code = recovery_code_from_bytes(&[0, 1, 2, 3, 4, 5, 6, 7, 30, 255]);
        assert_eq!(code, "abcde-fgh89");
        assert_eq!(code.len(), RECOVERY_CODE_LEN + 1);
        assert_eq!(normalize_recovery_code(" ABCDE fgh89 "), Some(code.clone()));
        assert_eq!(normalize_recovery_code("abcdefgh89"), Some(code));
        for invalid in ["abcde-fgh8", "abcde-fgh899", "abcde-fgh80", "123456", ""] {
            assert_eq!(normalize_recovery_code(invalid), None, "{invalid}");
        }
    }
}
