//! `Game Data/Suns.dat` — the project's directional sun/moon lights. The Blitz
//! client reads this (`Environment.bb::LoadSuns`, :236-272) and colours the
//! scene's directional light with the *active* sun's authored RGB
//! (`Environment3D.bb:430-548` selects by time-of-day window and applies
//! `LightColor(L\EN, S\LightR, S\LightG, S\LightB)`), turning the plain white
//! default light off whenever a colored sun is visible.
//!
//! The shipped default project is **not** white: the day sun is a warm amber
//! `(167,153,124)` and the night/moon sun a cool blue `(51,68,93)`. A renderer
//! that lights with a fixed white sun therefore diverges from the engine across
//! every sunlit surface — this parser exposes the authored colours so the
//! directional term can match.
//!
//! Per-sun on-disk layout (little-endian, `Environment.bb:248-267`):
//! `TexID[8] i16 · ShowPhases u8 · Phase_Length u8 · Size f32 · LightR/G/B u8 ·
//! PathAngle f32 · 12×{StartH,StartM,EndH,EndM} u8 · ShowFlares u8` = 78 bytes,
//! preceded by a 4-byte sun count.

use crate::reader::{BlitzReader, ReadError};

/// Number of per-sun visibility windows (one per season, `Environment3D.bb`
/// indexes them by `CurrentSeason`).
pub const SEASONS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sun {
    /// Directional light colour, normalised to `0.0..=1.0` (on disk: `u8` RGB).
    pub light: [f32; 3],
    /// Per-season visibility window `[start_minute, end_minute)` of the game day
    /// (minutes since midnight, `0..1440`). When `start > end` the window wraps
    /// past midnight (e.g. a moon visible 18:00→06:00).
    pub windows: [(u16, u16); SEASONS],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Suns {
    pub suns: Vec<Sun>,
}

/// `[start, end)` minute window, wrapping past midnight when `start > end`.
fn window_contains((start, end): (u16, u16), t: u16) -> bool {
    if start <= end {
        t >= start && t < end
    } else {
        t >= start || t < end
    }
}

impl Suns {
    pub fn parse(data: &[u8]) -> Result<Suns, ReadError> {
        let mut r = BlitzReader::new(data);
        let count = r.read_int()?;
        // Soft-fail on a corrupt/huge header rather than allocating wildly.
        if !(0..=64).contains(&count) {
            return Err(ReadError::CountTooLarge { count: count as i64, max: 64 });
        }
        let mut suns = Vec::with_capacity(count as usize);
        for _ in 0..count {
            for _ in 0..8 {
                r.read_short()?; // TexID[8] — unused (phase textures)
            }
            r.read_byte()?; // ShowPhases
            r.read_byte()?; // Phase_Length
            r.read_float()?; // Size
            let lr = r.read_byte()? as f32 / 255.0;
            let lg = r.read_byte()? as f32 / 255.0;
            let lb = r.read_byte()? as f32 / 255.0;
            r.read_float()?; // PathAngle
            let mut windows = [(0u16, 0u16); SEASONS];
            for w in windows.iter_mut() {
                let sh = r.read_byte()? as u16;
                let sm = r.read_byte()? as u16;
                let eh = r.read_byte()? as u16;
                let em = r.read_byte()? as u16;
                // Clamp into a valid day-minute (corrupt hours/minutes soft-fail
                // to a bounded value instead of a wild window).
                *w = ((sh * 60 + sm).min(1440), (eh * 60 + em).min(1440));
            }
            r.read_byte()?; // ShowFlares
            suns.push(Sun { light: [lr, lg, lb], windows });
        }
        Ok(Suns { suns })
    }

    /// The directional light colour active at `minutes` (game-minutes since
    /// midnight) for `season` (`0..SEASONS`), or `None` when no sun is visible
    /// (the caller then falls back to a neutral white light). Mirrors
    /// `Environment3D.bb:430-455`: the first sun whose season window contains the
    /// time is the active directional sun.
    pub fn light_at(&self, minutes: u16, season: usize) -> Option<[f32; 3]> {
        let t = minutes % 1440;
        let s = season.min(SEASONS - 1);
        self.suns
            .iter()
            .find(|sun| window_contains(sun.windows[s], t))
            .map(|sun| sun.light)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a one-sun `Suns.dat` body with a single season-0 window and a colour.
    fn sun_bytes(rgb: [u8; 3], start: (u8, u8), end: (u8, u8)) -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..8 {
            v.extend_from_slice(&0i16.to_le_bytes()); // TexID
        }
        v.push(0); // ShowPhases
        v.push(0); // Phase_Length
        v.extend_from_slice(&1.0f32.to_le_bytes()); // Size
        v.extend_from_slice(&rgb); // LightR/G/B
        v.extend_from_slice(&0.0f32.to_le_bytes()); // PathAngle
        // 12 season windows; season 0 = the given window, the rest 0,0.
        v.extend_from_slice(&[start.0, start.1, end.0, end.1]);
        for _ in 1..SEASONS {
            v.extend_from_slice(&[0, 0, 0, 0]);
        }
        v.push(0); // ShowFlares
        v
    }

    #[test]
    fn parses_day_and_night_suns() {
        // Mirrors the shipped default: warm day sun (06:00-18:00) + cool night sun
        // (18:00-06:00, wrapping midnight).
        let mut data = 2i32.to_le_bytes().to_vec();
        data.extend(sun_bytes([167, 153, 124], (6, 0), (18, 0))); // day
        data.extend(sun_bytes([51, 68, 93], (18, 0), (6, 0))); // night (wraps)
        let suns = Suns::parse(&data).unwrap();
        assert_eq!(suns.suns.len(), 2);
        // Day colour ≈ 167/255, 153/255, 124/255.
        let day = suns.suns[0].light;
        assert!((day[0] - 167.0 / 255.0).abs() < 1e-4);
        assert!((day[1] - 153.0 / 255.0).abs() < 1e-4);
        assert!((day[2] - 124.0 / 255.0).abs() < 1e-4);

        // Noon → the warm day sun; midnight → the cool night sun.
        assert_eq!(suns.light_at(12 * 60, 0), Some([167.0 / 255.0, 153.0 / 255.0, 124.0 / 255.0]));
        assert_eq!(suns.light_at(0, 0), Some([51.0 / 255.0, 68.0 / 255.0, 93.0 / 255.0]));
        // 03:00 is still night (wrap window); 09:00 is day.
        assert_eq!(suns.light_at(3 * 60, 0), Some([51.0 / 255.0, 68.0 / 255.0, 93.0 / 255.0]));
        assert_eq!(suns.light_at(9 * 60, 0), Some([167.0 / 255.0, 153.0 / 255.0, 124.0 / 255.0]));
    }

    #[test]
    fn no_visible_sun_is_none() {
        // A single sun visible only 06:00-07:00; any other time → None (white).
        let mut data = 1i32.to_le_bytes().to_vec();
        data.extend(sun_bytes([200, 200, 200], (6, 0), (7, 0)));
        let suns = Suns::parse(&data).unwrap();
        assert_eq!(suns.light_at(6 * 60 + 30, 0), Some([200.0 / 255.0; 3]));
        assert_eq!(suns.light_at(12 * 60, 0), None);
    }

    #[test]
    fn truncated_and_corrupt_soft_fail() {
        assert!(Suns::parse(&[]).is_err()); // no count
        assert!(Suns::parse(&1i32.to_le_bytes()).is_err()); // count=1 but no sun body
        // A huge count is rejected, not allocated.
        assert!(Suns::parse(&1_000_000i32.to_le_bytes()).is_err());
        // A zero-sun file is valid + empty.
        assert_eq!(Suns::parse(&0i32.to_le_bytes()).unwrap().suns.len(), 0);
        assert_eq!(Suns { suns: vec![] }.light_at(720, 0), None);
    }
}
