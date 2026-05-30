//! Audio output: zone music (looped) via rodio. Mirrors the engine, which on
//! zone load does `LoadSound("Data\Music\" + GetMusicName(LoadingMusicID))` +
//! `LoopSound` + `PlaySound` (ClientAreas.bb:147-149).
//!
//! Construction is fallible and non-fatal: a machine with no audio device (or a
//! headless CI run) yields `None` and the client simply runs silently. The
//! music-id → filename lookup lives in [`crate::assets`] via `Music.dat`.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

/// Owns the output stream + the current music sink. Dropping it stops audio.
pub struct Audio {
    // Keep the stream alive for as long as we play; dropping it cuts output.
    _stream: OutputStream,
    handle: OutputStreamHandle,
    music: Option<Sink>,
    /// The music id currently playing, so a re-entered zone doesn't restart it.
    current_music: Option<u16>,
}

impl Audio {
    /// Open the default output device. `None` if there is no device (headless /
    /// no audio) — callers treat audio as optional.
    pub fn new() -> Option<Audio> {
        match OutputStream::try_default() {
            Ok((stream, handle)) => Some(Audio {
                _stream: stream,
                handle,
                music: None,
                current_music: None,
            }),
            Err(e) => {
                eprintln!("[audio] no output device ({e}); running silent");
                None
            }
        }
    }

    /// Loop a music track at `path` (replaces any current track). `volume` is a
    /// linear gain (0..1). `id` tags the track so [`set_music`] can skip a
    /// redundant restart. Returns false (and logs) on open/decode failure.
    pub fn play_music_looped(&mut self, path: &Path, volume: f32, id: u16) -> bool {
        if let Some(s) = self.music.take() {
            s.stop();
        }
        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[audio] open {}: {e}", path.display());
                return false;
            }
        };
        let decoder = match Decoder::new(BufReader::new(file)) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[audio] decode {}: {e}", path.display());
                return false;
            }
        };
        let sink = match Sink::try_new(&self.handle) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] sink: {e}");
                return false;
            }
        };
        sink.set_volume(volume);
        // `.buffered()` makes the source `Clone`, which `repeat_infinite`
        // requires (a bare Decoder isn't restartable).
        sink.append(decoder.buffered().repeat_infinite());
        self.music = Some(sink);
        self.current_music = Some(id);
        true
    }

    /// Play the zone's music by id if it differs from what's already playing.
    /// `resolve` maps the id to an on-disk path (None = no track for that id).
    pub fn set_music<F>(&mut self, id: u16, volume: f32, resolve: F)
    where
        F: FnOnce(u16) -> Option<std::path::PathBuf>,
    {
        if id == 65535 || self.current_music == Some(id) {
            return;
        }
        if let Some(path) = resolve(id) {
            if self.play_music_looped(&path, volume, id) {
                println!("[audio] zone music #{id}: {}", path.display());
            }
        }
    }

    /// Fire-and-forget a one-shot sound (footstep, UI blip). The sink detaches
    /// and frees itself when the clip finishes. Silently no-ops on failure.
    pub fn play_oneshot(&self, path: &Path, volume: f32) {
        let Ok(file) = File::open(path) else { return };
        let Ok(decoder) = Decoder::new(BufReader::new(file)) else { return };
        let Ok(sink) = Sink::try_new(&self.handle) else { return };
        sink.set_volume(volume);
        sink.append(decoder);
        sink.detach();
    }
}

/// Decides when the local player's footstep one-shot should fire, based on a
/// cadence that quickens when running. Pure (no audio device) so it's testable.
#[derive(Debug)]
pub struct FootstepTimer {
    last_step: f32,
    /// Advances each step so the caller can alternate between sound files.
    count: usize,
}

/// Seconds between footsteps at a walk / run.
pub const WALK_INTERVAL: f32 = 0.46;
pub const RUN_INTERVAL: f32 = 0.30;

impl Default for FootstepTimer {
    fn default() -> Self {
        FootstepTimer { last_step: f32::MIN, count: 0 }
    }
}

impl FootstepTimer {
    pub fn new() -> FootstepTimer {
        FootstepTimer::default()
    }

    /// Call once per frame. Returns `Some(step_index)` when a footstep should
    /// play (the index increments each step, for alternating sounds); `None`
    /// otherwise. Standing still never steps.
    pub fn tick(&mut self, now: f32, moving: bool, running: bool) -> Option<usize> {
        if !moving {
            return None;
        }
        let interval = if running { RUN_INTERVAL } else { WALK_INTERVAL };
        if now - self.last_step >= interval {
            self.last_step = now;
            let idx = self.count;
            self.count = self.count.wrapping_add(1);
            Some(idx)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_step_when_still() {
        let mut t = FootstepTimer::new();
        assert_eq!(t.tick(0.0, false, false), None);
        assert_eq!(t.tick(100.0, false, false), None);
    }

    #[test]
    fn first_move_steps_immediately_then_paces() {
        let mut t = FootstepTimer::new();
        // First moving frame fires (last_step starts at -inf).
        assert_eq!(t.tick(0.0, true, false), Some(0));
        // Too soon for the next.
        assert_eq!(t.tick(0.2, true, false), None);
        // After a walk interval, the next step (index advances).
        assert_eq!(t.tick(WALK_INTERVAL + 0.01, true, false), Some(1));
    }

    #[test]
    fn running_is_faster_than_walking() {
        let mut t = FootstepTimer::new();
        assert_eq!(t.tick(0.0, true, true), Some(0));
        // A gap that's enough for a run but not a walk only steps when running.
        assert_eq!(t.tick(RUN_INTERVAL + 0.01, true, false), None);
        assert_eq!(t.tick(RUN_INTERVAL + 0.01, true, true), Some(1));
    }
}
