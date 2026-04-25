//! Abbreviation data types.

/// A registered abbreviation: a short input pattern that expands to a
/// longer body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Abbreviation {
    /// Stable identifier (free-form; used for logging and UI references).
    pub id: String,
    /// How user input is matched.
    pub trigger: Trigger,
    /// What text to insert on expansion.
    pub body: String,
    /// Which event commits the expansion.
    pub trigger_on: TriggerEvent,
    /// Relevance score used to break ties in the picker. Higher =
    /// surfaces first when two candidates otherwise rank equally.
    /// The starter dictionary uses 100 for common forms (declaratives,
    /// most greetings) and lower values for rarer ones (interrogatives
    /// like `습니까?`, alternate polite forms).
    pub priority: u32,
}

/// How an abbreviation is matched against the committed-text stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A sequence of **conjoining initial-consonant** code points
    /// (U+1100..=U+1112). Matches when the tail of the committed buffer
    /// is exactly this Cho run.
    ChoSeq(Vec<u32>),
    /// Literal word match — requires a **word boundary** (start of
    /// buffer or whitespace) before the match point. Use for
    /// standalone abbreviations and conjunctions like `그러나` that
    /// shouldn't fire mid-word.
    Literal(String),
    /// Suffix / 어미 match — **no** word boundary required. Use for
    /// Korean verb endings like `습니다`, `합니다` that glue onto a
    /// preceding stem.
    Ending(String),
}

impl Trigger {
    /// Length in *characters* that the match consumes when fired.
    /// Used by the engine to compute the rollback size.
    pub fn match_len(&self) -> usize {
        match self {
            Self::ChoSeq(cs) => cs.len(),
            Self::Literal(s) | Self::Ending(s) => s.chars().count(),
        }
    }

    /// Human-readable display of the trigger (for UI).
    pub fn display(&self) -> String {
        match self {
            Self::ChoSeq(cs) => cs
                .iter()
                .filter_map(|cp| char::from_u32(cp_to_compat(*cp)))
                .collect(),
            Self::Literal(s) | Self::Ending(s) => s.clone(),
        }
    }

    /// Whether the trigger is the Ending (suffix) variant.
    pub fn is_ending(&self) -> bool {
        matches!(self, Self::Ending(_))
    }
}

fn cp_to_compat(cp: u32) -> u32 {
    // Quick Cho → compat ㄱㄲㄴ...ㅎ mapping to keep model.rs standalone.
    match cp {
        0x1100 => 0x3131,
        0x1101 => 0x3132,
        0x1102 => 0x3134,
        0x1103 => 0x3137,
        0x1104 => 0x3138,
        0x1105 => 0x3139,
        0x1106 => 0x3141,
        0x1107 => 0x3142,
        0x1108 => 0x3143,
        0x1109 => 0x3145,
        0x110A => 0x3146,
        0x110B => 0x3147,
        0x110C => 0x3148,
        0x110D => 0x3149,
        0x110E => 0x314A,
        0x110F => 0x314B,
        0x1110 => 0x314C,
        0x1111 => 0x314D,
        0x1112 => 0x314E,
        other => other,
    }
}

/// Event types that can cause a pending match to fire.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TriggerEvent {
    /// Fire as soon as the trigger sequence is complete in the buffer.
    /// Use for snappy, unambiguous expansions.
    Immediate,
    /// Fire when the user presses Space.
    Space,
    /// Fire when the user presses Enter.
    Enter,
    /// Fire when a punctuation character is typed.
    Punctuation,
    /// Fire when the Hangul FSM completes a syllable (세벌식 특화).
    /// Not yet wired to the FSM; reserved.
    JongCompletion,
    /// Fire on an explicit user action (e.g., Tab).
    Explicit,
}

impl TriggerEvent {
    /// Whether a trigger configured for `want` should fire on `got`.
    pub fn matches(want: TriggerEvent, got: TriggerEvent) -> bool {
        match want {
            TriggerEvent::Immediate => got == TriggerEvent::Immediate,
            TriggerEvent::Space => got == TriggerEvent::Space,
            TriggerEvent::Enter => got == TriggerEvent::Enter,
            TriggerEvent::Punctuation => got == TriggerEvent::Punctuation,
            TriggerEvent::JongCompletion => got == TriggerEvent::JongCompletion,
            TriggerEvent::Explicit => got == TriggerEvent::Explicit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cho_seq_display_converts_to_compat() {
        let t = Trigger::ChoSeq(vec![0x1100, 0x1109]); // ᄀ ᄉ
        assert_eq!(t.display(), "ㄱㅅ");
    }

    #[test]
    fn literal_display_is_identity() {
        let t = Trigger::Literal("메일끝".into());
        assert_eq!(t.display(), "메일끝");
    }

    #[test]
    fn match_len_counts_chars_not_bytes() {
        let t = Trigger::Literal("메일끝".into());
        assert_eq!(t.match_len(), 3);
    }
}
