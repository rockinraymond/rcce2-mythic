//! Screen-space weather particles (rain / snow) driven by the zone's weather
//! byte. Pure update logic (no GPU) so the motion + wrapping is unit-testable;
//! the client draws each particle as a small overlay rect.
//!
//! Weather byte values (`Environment.bb`): 0 Sun, 1 Rain, 2 Snow, 3 Fog,
//! 4 Storm, 5 Wind. Storm rains too.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weather {
    /// Sun (byte 0) — clear skies, no particles, authored fog unchanged.
    Clear,
    Rain,
    Snow,
    /// Heavy fog (byte 3) — no particles, but pulls the fog far plane sharply in
    /// (Blitz `SetWeather` `W_Fog`: `FogFar - 500`). First-class so it is no
    /// longer silently collapsed to `Clear` (which rendered nothing).
    Fog,
    /// Storm: rain particles + (in the client) wind loop and thunder one-shots.
    Storm,
    /// Wind (byte 5) — no particles and authored fog unchanged (Blitz `W_Wind`
    /// leaves the fog distances at base); distinct from `Clear` only for audio
    /// (a wind loop) and cloud handling at the call sites.
    Wind,
}

/// Map the wire weather byte to a weather mode.
///
/// Byte values (`Environment.bb`): 0 Sun, 1 Rain, 2 Snow, 3 Fog, 4 Storm,
/// 5 Wind. Unknown bytes fall back to `Clear`.
pub fn weather_from_byte(b: u8) -> Weather {
    match b {
        1 => Weather::Rain,
        2 => Weather::Snow,
        3 => Weather::Fog,
        4 => Weather::Storm,
        5 => Weather::Wind,
        _ => Weather::Clear, // Sun + unknown
    }
}

impl Weather {
    /// Whether this weather draws rain particles (Rain or Storm).
    pub fn is_rainy(self) -> bool {
        matches!(self, Weather::Rain | Weather::Storm)
    }

    /// Whether this weather spawns falling particles at all (rain or snow).
    pub fn has_particles(self) -> bool {
        matches!(self, Weather::Rain | Weather::Storm | Weather::Snow)
    }
}

/// Weather-driven fog adjustment, mirroring Blitz `Environment3D.bb::SetWeather`
/// (`src/Modules/Environment3D.bb:157-235`).
///
/// Given the zone's authored base fog `near`/`far` (read from the same area
/// `.dat` `FogNear`/`FogFar` fields Blitz uses) and base fog colour, returns the
/// values for the active weather:
///
/// * `Clear` / `Wind` — fog unchanged (Blitz `W_Sun` / `W_Wind` restore base).
/// * `Rain` — far plane pulled in by 50.
/// * `Storm` — far plane pulled in by 100.
/// * `Snow` — far plane pulled in by 125 **and** the fog colour whitened to
///   `(200,200,200)` (Blitz `CameraFogColor 200,200,200` — the whiteout look).
/// * `Fog` — far plane pulled in by 500 (heavy murk).
///
/// For every non-clear case the near plane snaps to Blitz's rule
/// (`0.5` when the base near is beyond it, else `-50`) and the far plane is
/// clamped to at least `near + 10` so it never crosses the near plane on
/// short-fog zones. The deltas are absolute world units, exactly as authored in
/// the engine, so the visible strength scales with each zone's own fog — same as
/// the Blitz client.
pub fn weather_fog(kind: Weather, base_near: f32, base_far: f32, base_color: [f32; 3]) -> (f32, f32, [f32; 3]) {
    let far_delta = match kind {
        Weather::Rain => 50.0,
        Weather::Storm => 100.0,
        Weather::Snow => 125.0,
        Weather::Fog => 500.0,
        // Sun / Wind leave the authored fog exactly as-is.
        Weather::Clear | Weather::Wind => return (base_near, base_far, base_color),
    };
    let near = if base_near > 0.5 { 0.5 } else { -50.0 };
    let mut far = base_far - far_delta;
    if far < near + 10.0 {
        far = near + 10.0;
    }
    let color = if kind == Weather::Snow { [200.0 / 255.0; 3] } else { base_color };
    (near, far, color)
}

#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    /// Per-particle phase, for snow sway + size variety.
    pub phase: f32,
}

/// A pool of falling particles. Positions are in screen pixels; the pool
/// respreads when the viewport size changes.
#[derive(Debug, Default)]
pub struct WeatherSystem {
    particles: Vec<Particle>,
    rng: u32,
    last_w: f32,
    last_h: f32,
    time: f32,
}

impl WeatherSystem {
    pub fn new(count: usize) -> WeatherSystem {
        WeatherSystem {
            particles: vec![Particle { x: 0.0, y: 0.0, phase: 0.0 }; count],
            rng: 0x1234_5678,
            last_w: 0.0,
            last_h: 0.0,
            time: 0.0,
        }
    }

    /// Deterministic LCG in [0,1) — keeps tests reproducible and avoids an rng
    /// dependency.
    fn rand(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn respread(&mut self, w: f32, h: f32) {
        for i in 0..self.particles.len() {
            let (rx, ry, rp) = (self.rand(), self.rand(), self.rand());
            self.particles[i] = Particle { x: rx * w, y: ry * h, phase: rp * std::f32::consts::TAU };
        }
        self.last_w = w;
        self.last_h = h;
    }

    /// Advance the pool by `dt` seconds for `kind`, wrapping particles that
    /// leave the bottom/sides back into view. No-op for `Clear`.
    pub fn update(&mut self, dt: f32, w: f32, h: f32, kind: Weather) {
        // Clear / Fog / Wind draw no falling particles.
        if !kind.has_particles() || w <= 0.0 || h <= 0.0 {
            return;
        }
        if (w - self.last_w).abs() > 0.5 || (h - self.last_h).abs() > 0.5 {
            self.respread(w, h);
        }
        self.time += dt;
        // Vertical fall speed + horizontal drift per mode.
        let (vy, drift) = match kind {
            Weather::Rain => (900.0, 140.0),
            // Storm rains harder with stronger wind-driven drift.
            Weather::Storm => (1050.0, 240.0),
            Weather::Snow => (130.0, 0.0),
            // Non-particle weathers are filtered out above.
            Weather::Clear | Weather::Fog | Weather::Wind => (0.0, 0.0),
        };
        let t = self.time;
        for p in &mut self.particles {
            p.y += vy * dt;
            p.x += match kind {
                Weather::Rain | Weather::Storm => drift * dt,
                // Snow sways side to side.
                Weather::Snow => (t * 1.5 + p.phase).sin() * 22.0 * dt,
                Weather::Clear | Weather::Fog | Weather::Wind => 0.0,
            };
            // Wrap vertically (off the bottom → back to the top).
            if p.y >= h {
                p.y -= h;
            }
            // Wrap horizontally both ways.
            if p.x >= w {
                p.x -= w;
            } else if p.x < 0.0 {
                p.x += w;
            }
        }
    }

    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_mapping() {
        assert_eq!(weather_from_byte(0), Weather::Clear);
        assert_eq!(weather_from_byte(1), Weather::Rain);
        assert_eq!(weather_from_byte(2), Weather::Snow);
        assert_eq!(weather_from_byte(3), Weather::Fog); // now first-class (was collapsed to Clear)
        assert_eq!(weather_from_byte(4), Weather::Storm);
        assert_eq!(weather_from_byte(5), Weather::Wind); // now first-class
        assert_eq!(weather_from_byte(99), Weather::Clear); // unknown → Clear
        assert!(Weather::Rain.is_rainy() && Weather::Storm.is_rainy());
        assert!(!Weather::Snow.is_rainy() && !Weather::Clear.is_rainy());
        assert!(!Weather::Fog.is_rainy() && !Weather::Wind.is_rainy());
        // Only rain/snow weathers spawn falling particles.
        assert!(Weather::Rain.has_particles() && Weather::Snow.has_particles() && Weather::Storm.has_particles());
        assert!(!Weather::Clear.has_particles() && !Weather::Fog.has_particles() && !Weather::Wind.has_particles());
    }

    #[test]
    fn fog_unchanged_for_clear_and_wind() {
        let (n, f, c) = (1000.0f32, 8000.0f32, [0.4f32, 0.5, 0.6]);
        // Clear and Wind restore the authored fog exactly.
        assert_eq!(weather_fog(Weather::Clear, n, f, c), (n, f, c));
        assert_eq!(weather_fog(Weather::Wind, n, f, c), (n, f, c));
    }

    #[test]
    fn fog_pulls_far_plane_in_per_weather() {
        let (n, f, c) = (1000.0f32, 8000.0f32, [0.4f32, 0.5, 0.6]);
        // Far-plane deltas mirror Blitz SetWeather (W_Rain -50, W_Storm -100,
        // W_Snow -125, W_Fog -500). Base near > 0.5 → snaps to 0.5.
        assert_eq!(weather_fog(Weather::Rain, n, f, c), (0.5, 7950.0, c));
        assert_eq!(weather_fog(Weather::Storm, n, f, c), (0.5, 7900.0, c));
        assert_eq!(weather_fog(Weather::Fog, n, f, c).1, 7500.0);
        // Heavier weathers pull the far plane in further (more murk).
        assert!(weather_fog(Weather::Fog, n, f, c).1 < weather_fog(Weather::Snow, n, f, c).1);
        assert!(weather_fog(Weather::Snow, n, f, c).1 < weather_fog(Weather::Rain, n, f, c).1);
    }

    #[test]
    fn snow_whitens_fog_color() {
        let (n, f, c) = (1000.0f32, 8000.0f32, [0.2f32, 0.3, 0.5]);
        let (_, far, col) = weather_fog(Weather::Snow, n, f, c);
        assert_eq!(far, 7875.0); // 8000 - 125
        assert_eq!(col, [200.0 / 255.0; 3]); // Blitz CameraFogColor 200,200,200
        // Only snow recolours; rain/fog keep the authored colour.
        assert_eq!(weather_fog(Weather::Rain, n, f, c).2, c);
        assert_eq!(weather_fog(Weather::Fog, n, f, c).2, c);
    }

    #[test]
    fn fog_near_rule_and_far_clamp() {
        // Base near <= 0.5 → snaps to -50 (Blitz else-branch).
        let (near, _, _) = weather_fog(Weather::Rain, 0.3, 8000.0, [0.0; 3]);
        assert_eq!(near, -50.0);
        // Short-fog zone: a big delta would cross the near plane → far clamps to
        // near + 10 (never inverts).
        let (near2, far2, _) = weather_fog(Weather::Fog, 1000.0, 100.0, [0.0; 3]);
        assert_eq!(near2, 0.5);
        assert_eq!(far2, near2 + 10.0); // 100 - 500 = -400 < near+10 → clamped
        assert!(far2 > near2);
    }

    #[test]
    fn storm_rains_harder_than_rain() {
        // Storm uses rain particles with a higher fall speed. Use a huge viewport
        // so no particle wraps in one step, making the comparison exact.
        let (w, h) = (100_000.0f32, 100_000.0f32);
        let mut a = WeatherSystem::new(30);
        let mut b = WeatherSystem::new(30);
        a.update(0.0, w, h, Weather::Rain); // respread (same seed in both)
        b.update(0.0, w, h, Weather::Storm);
        let y0: f32 = a.particles().iter().map(|p| p.y).sum();
        a.update(0.1, w, h, Weather::Rain);
        b.update(0.1, w, h, Weather::Storm);
        let da: f32 = a.particles().iter().map(|p| p.y).sum::<f32>() - y0;
        let db: f32 = b.particles().iter().map(|p| p.y).sum::<f32>() - y0;
        assert!(db > da, "storm should fall faster: storm {db} vs rain {da}");
    }

    #[test]
    fn clear_is_noop() {
        let mut ws = WeatherSystem::new(10);
        ws.update(0.1, 640.0, 480.0, Weather::Clear);
        // Untouched: all particles still at the origin.
        assert!(ws.particles().iter().all(|p| p.x == 0.0 && p.y == 0.0));
    }

    #[test]
    fn rain_falls_and_wraps() {
        let mut ws = WeatherSystem::new(50);
        ws.update(0.016, 640.0, 480.0, Weather::Rain); // first call respreads
        // Every particle is on-screen after a respread.
        assert!(ws.particles().iter().all(|p| (0.0..640.0).contains(&p.x) && (0.0..480.0).contains(&p.y)));
        // After a long run they stay wrapped in-bounds (never escape downward).
        for _ in 0..200 {
            ws.update(0.05, 640.0, 480.0, Weather::Rain);
        }
        assert!(ws.particles().iter().all(|p| p.y >= 0.0 && p.y < 480.0));
    }

    #[test]
    fn resize_respreads() {
        let mut ws = WeatherSystem::new(20);
        ws.update(0.016, 640.0, 480.0, Weather::Snow);
        ws.update(0.016, 1280.0, 720.0, Weather::Snow);
        assert!(ws.particles().iter().all(|p| p.x < 1280.0 && p.y < 720.0));
    }
}
