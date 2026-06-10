//! `Game Data/Language.txt` — the client's localizable string table.
//!
//! The Blitz client loads this in `Language.bb::LoadLanguage` (:282-318): each
//! line is `Trim$`-ed; blank lines and comment-only lines (`;` as the first
//! char) are skipped; an inline `;` truncates the line to the text before it;
//! the surviving lines are the string constants in declaration order, indexed
//! 0-based by the `LS_*` constants in `Language.bb`. A project translates the
//! game purely by editing this file — so honoring it lets the Rust client show
//! the project's authored strings instead of hardcoded English (the "respect
//! the engine's customizable nature" goal), with the hardcoded English as a
//! soft fallback when the file (or a given index) is absent.
//!
//! Note: this loader is for client **display** strings. Blitz additionally
//! upper-cases the `LS_SCKick..LS_SCSeason` slash-command-name range (190..219)
//! at load so server chat dispatch matches case-insensitively; that's a
//! server-side concern with no effect on the display strings the client renders,
//! so it is intentionally not replicated here.

/// Selected `LS_*` indices (from `src/Modules/Language.bb`) the Rust client
/// resolves. Add more as strings are localized.
pub mod ls {
    /// `LS_XPReceived` (61) — appended after the XP number: `"<N> <this>"`.
    pub const XP_RECEIVED: usize = 61;
    /// `LS_YouKilled` (63) — prefix of the kill line: `"<this> <name>!"`.
    pub const YOU_KILLED: usize = 63;
    /// `LS_PickedUpItem` (64) — prefix of the loot line: `"<this> <name> (xN)"`.
    pub const PICKED_UP_ITEM: usize = 64;
}

/// The project's localizable string table, parsed from `Language.txt`. `Default`
/// is the empty table (every `get` → `None`), so a `World`/store without the
/// file simply falls back to hardcoded English.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Language {
    strings: Vec<String>,
}

impl Language {
    /// Parse `Language.txt`, mirroring `Language.bb::LoadLanguage` exactly:
    /// `Trim$` each line; skip blanks and comment-only lines (`;` first); strip
    /// an inline comment to the text before the `;` (NOT re-trimmed, matching
    /// Blitz `Left$`); collect the rest in order (0-indexed per the `LS_*`
    /// constants).
    pub fn parse(text: &str) -> Language {
        let mut strings = Vec::new();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            match line.find(';') {
                Some(0) => continue,                              // comment-only line
                Some(pos) => strings.push(line[..pos].to_string()), // strip inline comment (no re-trim)
                None => strings.push(line.to_string()),
            }
        }
        Language { strings }
    }

    /// The string at `LS_*` index `idx`, or `None` when out of range or empty
    /// (the caller then uses its hardcoded English default).
    pub fn get(&self, idx: usize) -> Option<&str> {
        self.strings.get(idx).map(String::as_str).filter(|s| !s.is_empty())
    }

    /// The string at `idx`, or `default` (owned) when absent — the common
    /// "localize with an English fallback" call.
    pub fn get_or<'a>(&'a self, idx: usize, default: &'a str) -> &'a str {
        self.get(idx).unwrap_or(default)
    }

    /// Number of parsed string constants.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skips_comments_and_blanks_and_is_zero_indexed() {
        // Mirrors the real Language.txt header: comment lines, blanks, then the
        // first real string at index 0.
        let txt = "; header comment\n\
                   ; another\n\
                   \n\
                   ; section\n\
                   \n\
                   Status: Connecting to server...\n\
                   Current File Progress:\n";
        let lang = Language::parse(txt);
        assert_eq!(lang.len(), 2);
        assert_eq!(lang.get(0), Some("Status: Connecting to server..."));
        assert_eq!(lang.get(1), Some("Current File Progress:"));
        assert_eq!(lang.get(2), None, "out of range → None");
    }

    #[test]
    fn inline_comment_stripped_not_retrimmed() {
        // Blitz `Left$(s, Pos-1)` keeps the text before `;` WITHOUT re-trimming,
        // so a space before the inline `;` survives. A leading-`;` line is
        // dropped entirely.
        let lang = Language::parse("Hello ; trailing comment\n;dropped\nWorld\n");
        assert_eq!(lang.get(0), Some("Hello ")); // trailing space preserved
        assert_eq!(lang.get(1), Some("World"));
        assert_eq!(lang.len(), 2);
    }

    #[test]
    fn leading_whitespace_trimmed_then_comment_checked() {
        // `Trim$` runs before the comment check, so an indented comment is still
        // a comment, and an indented string is trimmed.
        let lang = Language::parse("   ; indented comment\n   indented value\n");
        assert_eq!(lang.len(), 1);
        assert_eq!(lang.get(0), Some("indented value"));
    }

    #[test]
    fn empty_and_default_fall_back() {
        let lang = Language::default();
        assert!(lang.is_empty());
        assert_eq!(lang.get(61), None);
        assert_eq!(lang.get_or(61, "experience points received!"), "experience points received!");
        // A present, non-empty entry wins over the default.
        let lang = Language::parse("a\nb\n");
        assert_eq!(lang.get_or(1, "fallback"), "b");
        assert_eq!(lang.get_or(9, "fallback"), "fallback");
    }
}
