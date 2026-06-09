//! Floating combat-damage numbers. The renderer drains new [`CombatEvent`]s
//! from the world each frame into short-lived [`Floater`]s that rise and fade
//! above their target actor.
//!
//! The bookkeeping (consume only *new* events; expire by age) lives here so it
//! can be unit-tested without a window — the actual draw is the overlay path.

use crate::world::CombatEvent;

/// How long a damage number lives, in seconds.
pub const LIFETIME: f32 = 1.2;
/// How far (world-ish px applied at draw time) a number rises over its life.
pub const RISE: f32 = 38.0;

/// One on-screen floating number, anchored to a target actor by runtime id.
///
/// Two flavours share the same rise/fade/expiry machinery:
/// - **Combat damage** (the default): `value`/`color` are `None`; the draw shows
///   `damage` styled by `damage_type` (see `damage_color`).
/// - **Server-driven** (`P_FloatingNumber` / `BVM_CreateFloatingNumber`): `value`
///   is the signed amount the server sent and `color` is the server's explicit
///   RGB. The draw shows `value` in `color`, ignoring `damage`/`damage_type`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floater {
    pub rid: u16,
    pub damage: u16,
    pub damage_type: u8,
    /// Wall-clock (seconds since client start) when it spawned.
    pub t0: f32,
    /// `Some` for a server-driven floater: the signed amount to display (can be
    /// negative, unlike `damage`). `None` ⇒ a combat floater showing `damage`.
    pub value: Option<i32>,
    /// `Some` for a server-driven floater: explicit RGB (0..1). `None` ⇒ a combat
    /// floater coloured by `damage_type`.
    pub color: Option<[f32; 3]>,
}

impl Floater {
    /// Age in seconds at `now`.
    pub fn age(&self, now: f32) -> f32 {
        now - self.t0
    }
    /// Normalised 0..1 life progress (clamped).
    pub fn progress(&self, now: f32) -> f32 {
        (self.age(now) / LIFETIME).clamp(0.0, 1.0)
    }
    /// Fade alpha — full for the first third of life, then linearly to 0.
    pub fn alpha(&self, now: f32) -> f32 {
        let p = self.progress(now);
        if p < 0.33 { 1.0 } else { (1.0 - (p - 0.33) / 0.67).clamp(0.0, 1.0) }
    }
    /// Upward pixel offset at `now`.
    pub fn rise(&self, now: f32) -> f32 {
        self.progress(now) * RISE
    }
}

/// Tracks live floaters and how many combat events have been consumed so each
/// hit spawns exactly one number.
#[derive(Debug, Default)]
pub struct Floaters {
    items: Vec<Floater>,
    consumed: usize,
}

impl Floaters {
    pub fn new() -> Floaters {
        Floaters::default()
    }

    /// Spawn floaters for any combat events past the last consumed index.
    /// `events` is the world's append-only `combat_events` log; `now` is the
    /// current client elapsed time. Idempotent within a frame: events already
    /// consumed are never re-spawned, even if `events` is re-passed.
    pub fn ingest(&mut self, events: &[CombatEvent], now: f32) {
        // If the log was reset/shrank (shouldn't happen, but be safe), restart.
        if self.consumed > events.len() {
            self.consumed = 0;
        }
        for e in &events[self.consumed..] {
            self.items.push(Floater {
                rid: e.target,
                damage: e.damage,
                damage_type: e.damage_type,
                t0: now,
                value: None,
                color: None,
            });
        }
        self.consumed = events.len();
    }

    /// Spawn a server-driven floating number (`P_FloatingNumber`): a signed
    /// `value` in an explicit `color`, anchored over actor `rid`, rising and
    /// fading on the same timeline as a combat number. Independent of the
    /// `consumed` cursor — these are pushed directly, not drained from the
    /// combat log.
    pub fn spawn_value(&mut self, rid: u16, value: i32, color: [f32; 3], now: f32) {
        self.items.push(Floater {
            rid,
            damage: 0,
            damage_type: 0,
            t0: now,
            value: Some(value),
            color: Some(color),
        });
    }

    /// Drop floaters older than [`LIFETIME`].
    pub fn tick(&mut self, now: f32) {
        self.items.retain(|f| f.age(now) < LIFETIME);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Floater> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(target: u16, damage: u16) -> CombatEvent {
        CombatEvent { target, attacker: 0, damage, damage_type: 0 }
    }

    #[test]
    fn ingest_spawns_one_per_new_event() {
        let mut f = Floaters::new();
        let mut log = vec![ev(7, 10), ev(7, 12)];
        f.ingest(&log, 0.0);
        assert_eq!(f.len(), 2);
        // Re-ingesting the same log spawns nothing new.
        f.ingest(&log, 0.5);
        assert_eq!(f.len(), 2);
        // A new event appends exactly one.
        log.push(ev(9, 3));
        f.ingest(&log, 1.0);
        assert_eq!(f.len(), 3);
        assert_eq!(f.iter().last().unwrap().rid, 9);
    }

    #[test]
    fn tick_expires_by_age() {
        let mut f = Floaters::new();
        f.ingest(&[ev(1, 5)], 0.0);
        assert_eq!(f.len(), 1);
        f.tick(LIFETIME * 0.5); // still alive
        assert_eq!(f.len(), 1);
        f.tick(LIFETIME + 0.01); // expired
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn alpha_and_rise_progress() {
        let fl = Floater { rid: 1, damage: 5, damage_type: 0, t0: 0.0, value: None, color: None };
        assert_eq!(fl.alpha(0.0), 1.0); // fresh = opaque
        assert!(fl.rise(LIFETIME) >= RISE - 0.001); // fully risen at end
        assert!(fl.alpha(LIFETIME) <= 0.001); // faded out at end
        // Monotonic rise.
        assert!(fl.rise(LIFETIME * 0.25) < fl.rise(LIFETIME * 0.75));
    }

    #[test]
    fn server_value_floater_coexists_with_combat() {
        let mut f = Floaters::new();
        // A combat hit and a server-driven number live side by side, both on the
        // same rise/fade timeline but distinguished by `value`/`color`.
        f.ingest(&[ev(7, 10)], 0.0);
        f.spawn_value(7, -250, [0.0, 1.0, 0.0], 0.0); // a green heal "-250"
        assert_eq!(f.len(), 2);
        let combat = f.iter().find(|x| x.value.is_none()).unwrap();
        assert_eq!(combat.damage, 10);
        let server = f.iter().find(|x| x.value.is_some()).unwrap();
        assert_eq!(server.value, Some(-250));
        assert_eq!(server.color, Some([0.0, 1.0, 0.0]));
        assert_eq!(server.rid, 7);
        // Server floaters do NOT advance the combat-event cursor, so a later
        // combat ingest still fires.
        f.ingest(&[ev(7, 10), ev(8, 4)], 0.5);
        assert_eq!(f.len(), 3);
        // Both flavours expire on the same timeline (last one spawned at t=0.5).
        f.tick(0.5 + LIFETIME + 0.01);
        assert_eq!(f.len(), 0);
    }

    #[test]
    fn shrunk_log_restarts_cleanly() {
        let mut f = Floaters::new();
        f.ingest(&[ev(1, 1), ev(2, 2), ev(3, 3)], 0.0);
        assert_eq!(f.len(), 3);
        // A shorter log (e.g. a fresh world) must not panic and re-consumes.
        f.ingest(&[ev(4, 4)], 1.0);
        assert_eq!(f.len(), 4);
    }
}
