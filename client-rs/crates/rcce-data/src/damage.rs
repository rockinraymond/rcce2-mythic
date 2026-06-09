//! `Damage.dat` — the project's weapon damage-type names (`DamageTypes$`,
//! `Items.bb:486 LoadDamageTypes`). The file is 20 bounded strings (a 4-byte LE
//! length prefix + the bytes, i.e. `ReadBoundedString$(F, 256)`). A weapon's
//! `weapon_damage_type` indexes this table; Blitz shows the resolved name in the
//! weapon tooltip (`Interface3D.bb:1949`) and the combat damage output
//! (`ClientNet.bb:1129`). The names are project-customizable, so the client must
//! read them from disk rather than hard-code "Fire/Ice/…".

use crate::reader::BlitzReader;

/// The damage-type names from `Server Data/Damage.dat`, in file order (Blitz dims
/// `DamageTypes$(19)` = 20 slots).
#[derive(Debug, Clone, Default)]
pub struct DamageTypes {
    pub names: Vec<String>,
}

impl DamageTypes {
    /// Parse a `Damage.dat`: up to 20 bounded strings. Stops cleanly at EOF / the
    /// first unreadable entry, keeping what parsed — the same soft-fail posture as
    /// the other catalog parsers (and `LoadDamageTypes`, which aborts on a short
    /// read). Empty slots are kept so indices line up with the file.
    pub fn parse(data: &[u8]) -> DamageTypes {
        let mut r = BlitzReader::new(data);
        let mut names = Vec::with_capacity(20);
        for _ in 0..20 {
            match r.read_string(256) {
                Ok(s) => names.push(s),
                Err(_) => break,
            }
        }
        DamageTypes { names }
    }

    /// The damage-type name at `idx`, or `None` when out of range, negative, or an
    /// unnamed (empty) slot — so a caller can skip the "(…)" suffix entirely.
    pub fn name(&self, idx: i16) -> Option<&str> {
        if idx < 0 {
            return None;
        }
        self.names
            .get(idx as usize)
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bstr(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        let mut v = (b.len() as i32).to_le_bytes().to_vec();
        v.extend_from_slice(b);
        v
    }

    #[test]
    fn parses_names_and_resolves_index() {
        let mut d = Vec::new();
        for s in ["Piercing", "Slashing", "Fire"] {
            d.extend(bstr(s));
        }
        // Pad the remaining slots with empty strings (zero-length).
        for _ in 3..20 {
            d.extend((0i32).to_le_bytes());
        }
        let dt = DamageTypes::parse(&d);
        assert_eq!(dt.name(0), Some("Piercing"));
        assert_eq!(dt.name(2), Some("Fire"));
        assert_eq!(dt.name(3), None, "empty slot → None");
        assert_eq!(dt.name(99), None, "out of range → None");
        assert_eq!(dt.name(-1), None, "negative → None");
    }

    #[test]
    fn truncated_file_keeps_what_parsed() {
        let mut d = bstr("Piercing");
        d.extend_from_slice(&[0x05, 0x00]); // a truncated 4-byte length prefix
        let dt = DamageTypes::parse(&d);
        assert_eq!(dt.name(0), Some("Piercing"));
        assert_eq!(dt.name(1), None);
    }
}
