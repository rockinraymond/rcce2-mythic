//! `Game Data/Other.dat` — project client-display options the launcher writes and
//! the Blitz client reads at startup (`ClientLoaders.bb:28-40`). Layout (all little
//! -endian, `ReadByte`/`ReadInt`): HideNametags u8 · DisableCollisions u8 · ViewMode
//! u8 · ServerPort i32 · RequireMemorise u8 · UseBubbles u8 · BubblesR/G/B u8.
//!
//! Blitz `ReadByte`/`ReadInt` past EOF return 0, and the shipped default file is
//! truncated (9 bytes — no Bubbles RGB), so each field defaults to 0 when absent —
//! matching the engine exactly.

use crate::reader::BlitzReader;

/// Parsed `Other.dat`. Fields are the raw bytes; the semantic accessors apply the
/// engine's comparison rules (e.g. nametags hide only on the exact value 1).
#[derive(Debug, Clone, Copy, Default)]
pub struct OtherConfig {
    pub hide_nametags: u8,
    pub disable_collisions: u8,
    pub view_mode: u8,
    pub server_port: i32,
    pub require_memorise: u8,
    pub use_bubbles: u8,
    pub bubbles_rgb: [u8; 3],
}

impl OtherConfig {
    /// Read the fields in order, each defaulting to 0 past EOF (Blitz `ReadByte`/
    /// `ReadInt` semantics) so a short/absent file yields engine-default behaviour.
    pub fn parse(data: &[u8]) -> OtherConfig {
        let mut r = BlitzReader::new(data);
        let hide_nametags = r.read_byte().unwrap_or(0);
        let disable_collisions = r.read_byte().unwrap_or(0);
        let view_mode = r.read_byte().unwrap_or(0);
        let server_port = r.read_int().unwrap_or(0);
        let require_memorise = r.read_byte().unwrap_or(0);
        let use_bubbles = r.read_byte().unwrap_or(0);
        let rr = r.read_byte().unwrap_or(0);
        let gg = r.read_byte().unwrap_or(0);
        let bb = r.read_byte().unwrap_or(0);
        OtherConfig {
            hide_nametags,
            disable_collisions,
            view_mode,
            server_port,
            require_memorise,
            use_bubbles,
            bubbles_rgb: [rr, gg, bb],
        }
    }

    /// Whether actor nametags (the floating name + tag text) should be hidden. Blitz
    /// gates nametag creation on `If HideNametags <> 1` (Actors3D.bb:508), so they
    /// are hidden ONLY for the exact value 1 — any other value (incl. the default 2)
    /// shows them.
    pub fn nametags_hidden(&self) -> bool {
        self.hide_nametags == 1
    }

    /// Whether the camera should start in first-person. Blitz `ViewMode`: 1 =
    /// first-person only (`If ViewMode = 1 Then CamMode = 1`, ClientLoaders.bb:41);
    /// 2 = both (third-person default, toggleable); 3 = third-person only. So the
    /// camera starts first-person ONLY for ViewMode 1.
    pub fn default_first_person(&self) -> bool {
        self.view_mode == 1
    }

    /// Whether player-chat speech bubbles are shown. Blitz turns a plain say
    /// (`"<Name> text"`) into a bubble over the speaker only `If ... And UseBubbles
    /// > 1` (ClientNet.bb:1237). The shipped default is 2 (enabled).
    pub fn bubbles_enabled(&self) -> bool {
        self.use_bubbles > 1
    }

    /// The project's chat-bubble text colour (`BubblesR/G/B`), as RGBA 0..1. When
    /// the project leaves it unset (all zero — incl. the truncated default file that
    /// omits these bytes), fall back to a readable warm white instead of invisible
    /// black on the dark bubble background. (Blitz uses the raw bytes; this keeps
    /// the default project's bubbles legible — a genuine improvement.)
    pub fn bubble_color(&self) -> [f32; 4] {
        let [r, g, b] = self.bubbles_rgb;
        if r == 0 && g == 0 && b == 0 {
            return [0.95, 0.95, 0.85, 1.0];
        }
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }

    /// Whether the player may toggle first/third-person. Blitz flips `CamMode` on
    /// the toggle key only `If ... And ViewMode = 2` (Interface3D.bb:466); ViewMode
    /// 1 (first-only) and 3 (third-only) LOCK the camera. Expressed as "not locked to
    /// a single mode" so an absent file (soft-failed to view_mode 0, which Blitz —
    /// requiring the file — never has) leaves the toggle enabled rather than trapping
    /// the player in third-person. For the real values 1/2/3 this equals `== 2`.
    pub fn view_toggle_allowed(&self) -> bool {
        self.view_mode != 1 && self.view_mode != 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nametags_hidden_only_on_exact_one() {
        // Byte 0 = HideNametags. Only the exact value 1 hides (Blitz `<> 1`).
        let cfg = |b: u8| OtherConfig::parse(&[b, 0, 2, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
        assert!(cfg(1).nametags_hidden(), "1 hides");
        assert!(!cfg(0).nametags_hidden(), "0 shows");
        assert!(!cfg(2).nametags_hidden(), "2 (the shipped default) shows");
        assert!(!cfg(255).nametags_hidden(), "any other value shows");
    }

    #[test]
    fn view_mode_semantics() {
        // Byte 2 = ViewMode. 1 = first-person only, 2 = both (default), 3 = third only.
        let cfg = |vm: u8| OtherConfig::parse(&[0, 0, vm, 0, 0, 0, 0, 0, 0]);
        assert!(cfg(1).default_first_person() && !cfg(1).view_toggle_allowed(), "1 = first-only, locked");
        assert!(!cfg(2).default_first_person() && cfg(2).view_toggle_allowed(), "2 = third default, toggleable");
        assert!(!cfg(3).default_first_person() && !cfg(3).view_toggle_allowed(), "3 = third-only, locked");
        // Absent file (soft-failed to 0) → third-person start, but toggle stays
        // ENABLED (don't trap the player; only an explicit 1/3 locks).
        assert!(!cfg(0).default_first_person() && cfg(0).view_toggle_allowed());
    }

    #[test]
    fn bubble_semantics() {
        // Full record layout: HideNametags, DisableCollisions, ViewMode, ServerPort
        // (i32 = 4 bytes!), RequireMemorise, UseBubbles, BubblesR/G/B. So UseBubbles
        // is byte index 8 and the colour is bytes 9-11.
        let with = |ub: u8, rgb: [u8; 3]| {
            OtherConfig::parse(&[0, 0, 2, 0, 0, 0, 0, 0, ub, rgb[0], rgb[1], rgb[2]])
        };
        assert!(with(2, [10, 20, 30]).bubbles_enabled(), "2 enables");
        assert!(!with(1, [10, 20, 30]).bubbles_enabled(), "1 disables");
        assert!(!with(0, [0, 0, 0]).bubbles_enabled(), "0 disables");
        // Set colour is honored.
        let c = with(2, [255, 128, 0]).bubble_color();
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.502).abs() < 0.01 && c[2] == 0.0);
        // Unset (all-zero / truncated) colour falls back to a readable warm white.
        assert_eq!(with(2, [0, 0, 0]).bubble_color(), [0.95, 0.95, 0.85, 1.0]);
    }

    #[test]
    fn parses_full_record_and_truncation() {
        // HideNametags 1, DisableCollisions 0, ViewMode 3, ServerPort 25000,
        // RequireMemorise 1, UseBubbles 2, Bubbles 10/20/30.
        let mut d = vec![1u8, 0, 3];
        d.extend_from_slice(&25000i32.to_le_bytes());
        d.extend_from_slice(&[1, 2, 10, 20, 30]);
        let c = OtherConfig::parse(&d);
        assert_eq!(c.hide_nametags, 1);
        assert_eq!(c.view_mode, 3);
        assert_eq!(c.server_port, 25000);
        assert_eq!(c.use_bubbles, 2);
        assert_eq!(c.bubbles_rgb, [10, 20, 30]);
        // The shipped default is 9 bytes (no Bubbles RGB) — those default to 0.
        let short = OtherConfig::parse(&d[..9]);
        assert_eq!(short.use_bubbles, 2);
        assert_eq!(short.bubbles_rgb, [0, 0, 0]);
        // Empty file → all zero (no panic).
        let empty = OtherConfig::parse(&[]);
        assert_eq!(empty.hide_nametags, 0);
        assert_eq!(empty.server_port, 0);
    }
}
