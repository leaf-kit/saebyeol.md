//! Compound-jamo composition, decomposition, and role-transfer tables.
//!
//! These tables encode the linguistic rules Hangul IMEs need when two
//! input jamo collide in the same slot of the syllable (compound vowel
//! such as ㅗ+ㅏ=ㅘ, compound final such as ㄹ+ㄱ=ㄺ) and when a final
//! must be pulled off the previous syllable to seed a new one ("받침
//! 이동", e.g. 갃 + ㅏ → 각 + 사).
//!
//! All code points in this module are conjoining Jamo (U+1100..=U+11FF);
//! presentation-form conversions live in [`crate::hangul::output`].

// ───────────────────────── Compound medial vowels ─────────────────────────

/// Try to compose two medial vowels into a compound vowel.
///
/// Returns `Some(compound_cp)` when `(first, second)` matches one of the
/// seven canonical compounds (ㅘ, ㅙ, ㅚ, ㅝ, ㅞ, ㅟ, ㅢ).
pub fn compose_jung(first: u32, second: u32) -> Option<u32> {
    match (first, second) {
        (0x1169, 0x1161) => Some(0x116A), // ㅗ + ㅏ = ㅘ
        (0x1169, 0x1162) => Some(0x116B), // ㅗ + ㅐ = ㅙ
        (0x1169, 0x1175) => Some(0x116C), // ㅗ + ㅣ = ㅚ
        (0x116E, 0x1165) => Some(0x116F), // ㅜ + ㅓ = ㅝ
        (0x116E, 0x1166) => Some(0x1170), // ㅜ + ㅔ = ㅞ
        (0x116E, 0x1175) => Some(0x1171), // ㅜ + ㅣ = ㅟ
        (0x1173, 0x1175) => Some(0x1174), // ㅡ + ㅣ = ㅢ
        _ => None,
    }
}

/// Decompose a compound vowel by removing its last component, returning
/// the surviving first component. Returns `None` for simple vowels.
///
/// Used by backspace to pop one jamo piece at a time.
pub fn decompose_jung(jung: u32) -> Option<u32> {
    match jung {
        0x116A..=0x116C => Some(0x1169), // ㅘ/ㅙ/ㅚ → ㅗ
        0x116F..=0x1171 => Some(0x116E), // ㅝ/ㅞ/ㅟ → ㅜ
        0x1174 => Some(0x1173),          // ㅢ → ㅡ
        _ => None,
    }
}

// ───────────────────────── Compound final consonants ─────────────────────

/// Try to compose two final consonants into a compound final (겹받침)
/// or into a doubled final (ᆨᆨ→ᆩ, ᆺᆺ→ᆻ).
pub fn compose_jong(first: u32, second: u32) -> Option<u32> {
    match (first, second) {
        // Doubled Jongs (same consonant pressed twice).
        (0x11A8, 0x11A8) => Some(0x11A9), // ㄱ + ㄱ = ㄲ (받침)
        (0x11BA, 0x11BA) => Some(0x11BB), // ㅅ + ㅅ = ㅆ (받침)
        // 겹받침.
        (0x11A8, 0x11BA) => Some(0x11AA), // ㄱ + ㅅ = ㄳ
        (0x11AB, 0x11BD) => Some(0x11AC), // ㄴ + ㅈ = ㄵ
        (0x11AB, 0x11C2) => Some(0x11AD), // ㄴ + ㅎ = ㄶ
        (0x11AF, 0x11A8) => Some(0x11B0), // ㄹ + ㄱ = ㄺ
        (0x11AF, 0x11B7) => Some(0x11B1), // ㄹ + ㅁ = ㄻ
        (0x11AF, 0x11B8) => Some(0x11B2), // ㄹ + ㅂ = ㄼ
        (0x11AF, 0x11BA) => Some(0x11B3), // ㄹ + ㅅ = ㄽ
        (0x11AF, 0x11C0) => Some(0x11B4), // ㄹ + ㅌ = ㄾ
        (0x11AF, 0x11C1) => Some(0x11B5), // ㄹ + ㅍ = ㄿ
        (0x11AF, 0x11C2) => Some(0x11B6), // ㄹ + ㅎ = ㅀ
        (0x11B8, 0x11BA) => Some(0x11B9), // ㅂ + ㅅ = ㅄ
        _ => None,
    }
}

/// Try to compose two **initial** consonants pressed consecutively into
/// a doubled initial (ㄱㄱ→ㄲ / ㄷㄷ→ㄸ / ㅂㅂ→ㅃ / ㅅㅅ→ㅆ / ㅈㅈ→ㅉ).
///
/// Returns `None` for consonants without a doubled form (ㄴㄹㅁㅇㅊㅋㅌㅍㅎ)
/// or when `first != second`.
pub fn compose_cho_double(first: u32, second: u32) -> Option<u32> {
    if first != second {
        return None;
    }
    match first {
        0x1100 => Some(0x1101), // ㄱ + ㄱ = ㄲ
        0x1103 => Some(0x1104), // ㄷ + ㄷ = ㄸ
        0x1107 => Some(0x1108), // ㅂ + ㅂ = ㅃ
        0x1109 => Some(0x110A), // ㅅ + ㅅ = ㅆ
        0x110C => Some(0x110D), // ㅈ + ㅈ = ㅉ
        _ => None,
    }
}

/// Decompose a compound final into its first component, if compound.
pub fn decompose_jong(jong: u32) -> Option<u32> {
    match jong {
        0x11A9 | 0x11AA => Some(0x11A8), // ㄲ/ㄳ → ㄱ
        0x11AC..=0x11AD => Some(0x11AB), // ㄵ/ㄶ → ㄴ
        0x11B0..=0x11B6 => Some(0x11AF), // ㄺ/ㄻ/ㄼ/ㄽ/ㄾ/ㄿ/ㅀ → ㄹ
        0x11B9 => Some(0x11B8),          // ㅄ → ㅂ
        0x11BB => Some(0x11BA),          // ㅆ → ㅅ (doubled final)
        _ => None,
    }
}

// ───────────────────── 받침 이동 (jong → next cho) ──────────────────────

/// Split a final when a vowel arrives after it.
///
/// Returns `(remaining_jong, moved_cho)`:
///   * for a compound final, the first component stays on the previous
///     syllable and the second component becomes the Cho of the new one;
///   * for a simple final, the previous syllable loses its jong entirely
///     and the consonant moves to Cho position.
pub fn split_jong(jong: u32) -> (Option<u32>, u32) {
    match jong {
        0x11AA => (Some(0x11A8), 0x1109), // ㄳ → ㄱ + ㅅ
        0x11AC => (Some(0x11AB), 0x110C), // ㄵ → ㄴ + ㅈ
        0x11AD => (Some(0x11AB), 0x1112), // ㄶ → ㄴ + ㅎ
        0x11B0 => (Some(0x11AF), 0x1100), // ㄺ → ㄹ + ㄱ
        0x11B1 => (Some(0x11AF), 0x1106), // ㄻ → ㄹ + ㅁ
        0x11B2 => (Some(0x11AF), 0x1107), // ㄼ → ㄹ + ㅂ
        0x11B3 => (Some(0x11AF), 0x1109), // ㄽ → ㄹ + ㅅ
        0x11B4 => (Some(0x11AF), 0x1110), // ㄾ → ㄹ + ㅌ
        0x11B5 => (Some(0x11AF), 0x1111), // ㄿ → ㄹ + ㅍ
        0x11B6 => (Some(0x11AF), 0x1112), // ㅀ → ㄹ + ㅎ
        0x11B9 => (Some(0x11B8), 0x1109), // ㅄ → ㅂ + ㅅ
        simple => (None, jong_to_cho(simple)),
    }
}

/// Map a simple Jong code point to its Cho counterpart.
///
/// The three Cho-only consonants (ㄸ U+1104, ㅃ U+1108, ㅉ U+110D) have
/// no Jong form and therefore never appear here.
fn jong_to_cho(jong: u32) -> u32 {
    match jong {
        0x11A8 => 0x1100, // ㄱ
        0x11A9 => 0x1101, // ㄲ
        0x11AB => 0x1102, // ㄴ
        0x11AE => 0x1103, // ㄷ
        0x11AF => 0x1105, // ㄹ
        0x11B7 => 0x1106, // ㅁ
        0x11B8 => 0x1107, // ㅂ
        0x11BA => 0x1109, // ㅅ
        0x11BB => 0x110A, // ㅆ
        0x11BC => 0x110B, // ㅇ
        0x11BD => 0x110C, // ㅈ
        0x11BE => 0x110E, // ㅊ
        0x11BF => 0x110F, // ㅋ
        0x11C0 => 0x1110, // ㅌ
        0x11C1 => 0x1111, // ㅍ
        0x11C2 => 0x1112, // ㅎ
        other => other,   // unreachable for valid Jong inputs
    }
}

/// Inverse of [`jong_to_cho`]: map a simple Cho to its Jong counterpart, if any.
pub fn cho_to_jong(cho: u32) -> Option<u32> {
    match cho {
        0x1100 => Some(0x11A8),
        0x1101 => Some(0x11A9),
        0x1102 => Some(0x11AB),
        0x1103 => Some(0x11AE),
        0x1105 => Some(0x11AF),
        0x1106 => Some(0x11B7),
        0x1107 => Some(0x11B8),
        0x1109 => Some(0x11BA),
        0x110A => Some(0x11BB),
        0x110B => Some(0x11BC),
        0x110C => Some(0x11BD),
        0x110E => Some(0x11BE),
        0x110F => Some(0x11BF),
        0x1110 => Some(0x11C0),
        0x1111 => Some(0x11C1),
        0x1112 => Some(0x11C2),
        _ => None, // ㄸ, ㅃ, ㅉ have no Jong form
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compound_vowel_roundtrip() {
        assert_eq!(compose_jung(0x1169, 0x1161), Some(0x116A)); // ㅗㅏ→ㅘ
        assert_eq!(decompose_jung(0x116A), Some(0x1169)); // ㅘ→ㅗ
    }

    #[test]
    fn compound_final_roundtrip() {
        assert_eq!(compose_jong(0x11AF, 0x11A8), Some(0x11B0)); // ㄹㄱ→ㄺ
        assert_eq!(decompose_jong(0x11B0), Some(0x11AF));
    }

    #[test]
    fn split_compound_jong_keeps_first_moves_second() {
        // ㄳ (0x11AA): ㄱ stays, ㅅ moves → Cho ㅅ (0x1109)
        assert_eq!(split_jong(0x11AA), (Some(0x11A8), 0x1109));
    }

    #[test]
    fn split_simple_jong_moves_whole_consonant() {
        // ㄱ (0x11A8): nothing stays, ㄱ moves → Cho ㄱ (0x1100)
        assert_eq!(split_jong(0x11A8), (None, 0x1100));
    }

    #[test]
    fn cho_double_covers_five_standard_pairs() {
        assert_eq!(compose_cho_double(0x1100, 0x1100), Some(0x1101)); // ㄱㄱ → ㄲ
        assert_eq!(compose_cho_double(0x1103, 0x1103), Some(0x1104)); // ㄷㄷ → ㄸ
        assert_eq!(compose_cho_double(0x1107, 0x1107), Some(0x1108)); // ㅂㅂ → ㅃ
        assert_eq!(compose_cho_double(0x1109, 0x1109), Some(0x110A)); // ㅅㅅ → ㅆ
        assert_eq!(compose_cho_double(0x110C, 0x110C), Some(0x110D)); // ㅈㅈ → ㅉ
    }

    #[test]
    fn cho_double_rejects_non_pairs() {
        // ㄴㄴ not a double form.
        assert!(compose_cho_double(0x1102, 0x1102).is_none());
        // Different consonants never double.
        assert!(compose_cho_double(0x1100, 0x1103).is_none());
    }

    #[test]
    fn jong_double_extends_compose_jong() {
        assert_eq!(compose_jong(0x11A8, 0x11A8), Some(0x11A9)); // ᆨᆨ → ᆩ
        assert_eq!(compose_jong(0x11BA, 0x11BA), Some(0x11BB)); // ᆺᆺ → ᆻ
    }

    #[test]
    fn decompose_handles_doubled_jongs() {
        assert_eq!(decompose_jong(0x11A9), Some(0x11A8)); // ᆩ → ᆨ
        assert_eq!(decompose_jong(0x11BB), Some(0x11BA)); // ᆻ → ᆺ
    }

    #[test]
    fn cho_only_consonants_have_no_jong() {
        assert!(cho_to_jong(0x1104).is_none()); // ㄸ
        assert!(cho_to_jong(0x1108).is_none()); // ㅃ
        assert!(cho_to_jong(0x110D).is_none()); // ㅉ
    }
}
