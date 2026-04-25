//! Hangul Jamo newtypes.
//!
//! Internal state is always represented as Unicode *Hangul Jamo*
//! conjoining forms (U+1100..=U+11FF). The three roles — initial (Cho,
//! 초성), medial (Jung, 중성), and final (Jong, 종성) — each have a
//! disjoint conjoining code range, so a newtype per role eliminates
//! role confusion at compile time.
//!
//! NFC syllables (U+AC00..=U+D7A3) and compatibility jamo (U+3130..=U+318F)
//! are *output* forms; see [`crate::hangul::output`].

use core::fmt;

/// Initial consonant (초성). Conjoining range: `U+1100..=U+1112` (19 values).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Cho(u32);

impl Cho {
    /// First conjoining code point in the Cho range.
    pub const FIRST: u32 = 0x1100;
    /// Last conjoining code point in the Cho range.
    pub const LAST: u32 = 0x1112;
    /// Number of initial consonants (19).
    pub const COUNT: u32 = Self::LAST - Self::FIRST + 1;

    /// Construct from a raw code point if it lies in the Cho range.
    pub fn from_codepoint(cp: u32) -> Option<Self> {
        if (Self::FIRST..=Self::LAST).contains(&cp) {
            Some(Self(cp))
        } else {
            None
        }
    }

    /// Zero-based index into the 19-element Cho table.
    #[inline]
    pub fn index(self) -> u32 {
        self.0 - Self::FIRST
    }

    /// Conjoining code point.
    #[inline]
    pub fn codepoint(self) -> u32 {
        self.0
    }
}

/// Medial vowel (중성). Conjoining range: `U+1161..=U+1175` (21 values).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Jung(u32);

impl Jung {
    /// First conjoining code point in the Jung range.
    pub const FIRST: u32 = 0x1161;
    /// Last conjoining code point in the Jung range.
    pub const LAST: u32 = 0x1175;
    /// Number of medial vowels (21).
    pub const COUNT: u32 = Self::LAST - Self::FIRST + 1;

    /// Construct from a raw code point if it lies in the Jung range.
    pub fn from_codepoint(cp: u32) -> Option<Self> {
        if (Self::FIRST..=Self::LAST).contains(&cp) {
            Some(Self(cp))
        } else {
            None
        }
    }

    /// Zero-based index into the 21-element Jung table.
    #[inline]
    pub fn index(self) -> u32 {
        self.0 - Self::FIRST
    }

    /// Conjoining code point.
    #[inline]
    pub fn codepoint(self) -> u32 {
        self.0
    }
}

/// Final consonant (종성). Conjoining range: `U+11A8..=U+11C2` (27 values).
///
/// For NFC syllable composition the "no final" slot has index 0, so a
/// present [`Jong`] always has composition index `1..=27`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Jong(u32);

impl Jong {
    /// First conjoining code point in the Jong range.
    pub const FIRST: u32 = 0x11A8;
    /// Last conjoining code point in the Jong range.
    pub const LAST: u32 = 0x11C2;
    /// Number of final consonants excluding the empty slot (27).
    pub const COUNT: u32 = Self::LAST - Self::FIRST + 1;

    /// Construct from a raw code point if it lies in the Jong range.
    pub fn from_codepoint(cp: u32) -> Option<Self> {
        if (Self::FIRST..=Self::LAST).contains(&cp) {
            Some(Self(cp))
        } else {
            None
        }
    }

    /// NFC composition index in `1..=27`. (Index `0` means "no final" and
    /// is not representable by this type.)
    #[inline]
    pub fn composition_index(self) -> u32 {
        self.0 - Self::FIRST + 1
    }

    /// Conjoining code point.
    #[inline]
    pub fn codepoint(self) -> u32 {
        self.0
    }
}

/// A Jamo-level input emitted by a [`crate::Layout`] when a physical key
/// is mapped.
///
/// Consonant keys are role-polymorphic: in 두벌식 the same physical key
/// produces a Cho form at syllable start and a Jong form after a medial,
/// so both conjoining code points are carried. 세벌식 keys are
/// single-role and leave the unused side as [`None`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum JamoInput {
    /// Consonant. At least one of `cho` / `jong` is set for a valid key.
    ///
    /// The FSM prefers the Jong form when the current state already has a
    /// medial; otherwise it falls back to starting a new syllable with the
    /// Cho form.
    Cons {
        /// Conjoining Cho code point, if the key has an initial form.
        cho: Option<u32>,
        /// Conjoining Jong code point, if the key has a final form.
        jong: Option<u32>,
    },
    /// Medial vowel (conjoining Jung code point).
    Jung(u32),
}

impl JamoInput {
    /// Shorthand for a 두벌식 consonant that can act as both Cho and Jong.
    #[inline]
    pub fn cho_dual(cho: u32, jong: u32) -> Self {
        Self::Cons {
            cho: Some(cho),
            jong: Some(jong),
        }
    }

    /// Shorthand for a Cho-only consonant (e.g. ㄸ, ㅃ, ㅉ or 세벌식 초성 키).
    #[inline]
    pub fn cho_only(cho: u32) -> Self {
        Self::Cons {
            cho: Some(cho),
            jong: None,
        }
    }

    /// Shorthand for a Jong-only consonant (세벌식 종성 키).
    #[inline]
    pub fn jong_only(jong: u32) -> Self {
        Self::Cons {
            cho: None,
            jong: Some(jong),
        }
    }

    /// Shorthand for a medial vowel.
    #[inline]
    pub fn vowel(jung: u32) -> Self {
        Self::Jung(jung)
    }
}

impl fmt::Display for Cho {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X}", self.0)
    }
}

impl fmt::Display for Jung {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X}", self.0)
    }
}

impl fmt::Display for Jong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_bounds_match_spec() {
        assert_eq!(Cho::COUNT, 19);
        assert_eq!(Jung::COUNT, 21);
        assert_eq!(Jong::COUNT, 27);
    }

    #[test]
    fn from_codepoint_rejects_out_of_range() {
        assert!(Cho::from_codepoint(0x10FF).is_none());
        assert!(Cho::from_codepoint(0x1113).is_none());
        assert!(Jung::from_codepoint(0x1160).is_none());
        assert!(Jong::from_codepoint(0x11A7).is_none());
    }

    #[test]
    fn round_trip_codepoints() {
        let c = Cho::from_codepoint(0x1100).unwrap();
        assert_eq!(c.codepoint(), 0x1100);
        assert_eq!(c.index(), 0);
        let j = Jong::from_codepoint(0x11C2).unwrap();
        assert_eq!(j.composition_index(), 27);
    }
}
