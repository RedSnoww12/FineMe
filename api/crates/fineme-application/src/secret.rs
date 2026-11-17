//! Vérification des codes et secrets partagés (configuration, secours, tâche quotidienne).

/// Comparaison en temps constant : la durée ne dépend pas de la position du premier octet différent.
fn constant_time_eq(expected: &[u8], provided: &[u8]) -> bool {
    let mut difference = expected.len() ^ provided.len();
    for index in 0..expected.len().max(provided.len()) {
        let (a, b) = (
            expected.get(index).copied().unwrap_or(0),
            provided.get(index).copied().unwrap_or(0),
        );
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

/// Vrai si `provided` (espaces de début et de fin ignorés) correspond au code attendu.
pub(crate) fn code_matches(expected: &str, provided: Option<&str>) -> bool {
    constant_time_eq(
        expected.as_bytes(),
        provided.unwrap_or("").trim().as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison_is_exact() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(constant_time_eq(b"", b""));
        for other in [&b"abd"[..], b"ab", b"abcd", b"", b"ABC"] {
            assert!(!constant_time_eq(b"abc", other));
        }
    }

    #[test]
    fn code_matching_ignores_surrounding_spaces_only() {
        assert!(code_matches("code-fictif", Some("  code-fictif ")));
        assert!(!code_matches("code-fictif", Some("code fictif")));
        assert!(!code_matches("code-fictif", None));
    }
}
