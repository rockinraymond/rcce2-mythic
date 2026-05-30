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
}
