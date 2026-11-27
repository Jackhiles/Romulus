//! Music for the focused game: one track at a time, looped, faded in and out.
//!
//! Played natively rather than in the web view: the AppImage's WebKit is built without
//! its media framework. Playback runs on its own thread; [`Player`] only sends it
//! commands, so the UI never waits on audio.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

const FADE_IN: Duration = Duration::from_millis(900);
const FADE_OUT: Duration = Duration::from_millis(350);
const TICK: Duration = Duration::from_millis(20);

enum Cmd {
    Play(PathBuf),
    Stop,
    Volume(f32),
}

/// Handle to the music thread. Cheap to clone; dropping the last one ends the thread.
#[derive(Clone)]
pub struct Player {
    tx: Sender<Cmd>,
}

impl Player {
    /// Start the music thread. Without an audio device it logs once and does nothing.
    pub fn new(volume: f32) -> Self {
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new()
            .name("music".into())
            .spawn(move || run(rx, volume.clamp(0.0, 1.0)));
        Self { tx }
    }

    /// Fade out what is playing and fade in `path`, looped.
    pub fn play(&self, path: PathBuf) {
        let _ = self.tx.send(Cmd::Play(path));
    }

    /// Fade out and stop.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    /// 0.0 to 1.0.
    pub fn set_volume(&self, v: f32) {
        let _ = self.tx.send(Cmd::Volume(v.clamp(0.0, 1.0)));
    }
}

/// Volume of a fade at `elapsed` (linear, 0 → `to` over `len`).
pub fn fade(elapsed: Duration, len: Duration, to: f32) -> f32 {
    if len.is_zero() {
        return to;
    }
    (elapsed.as_secs_f32() / len.as_secs_f32()).clamp(0.0, 1.0) * to
}

fn run(rx: Receiver<Cmd>, mut volume: f32) {
    let stream = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("no audio output for game music: {e}");
            // Keep draining so senders never block or fail loudly.
            while rx.recv().is_ok() {}
            return;
        }
    };
    let mut sink: Option<rodio::Player> = None;
    // (start, target) of a running fade-in.
    let mut fading_in: Option<Instant> = None;

    let fade_out = |s: &rodio::Player| {
        let from = s.volume() as f32;
        let start = Instant::now();
        while start.elapsed() < FADE_OUT {
            s.set_volume((from * (1.0 - fade(start.elapsed(), FADE_OUT, 1.0))) as _);
            std::thread::sleep(TICK);
        }
        s.stop();
    };

    loop {
        let cmd = if fading_in.is_some() {
            match rx.recv_timeout(TICK) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            }
        };
        match cmd {
            Some(Cmd::Play(path)) => {
                if let Some(s) = sink.take() {
                    fade_out(&s);
                }
                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(e) => {
                        tracing::debug!("music file {}: {e}", path.display());
                        continue;
                    }
                };
                match rodio::Decoder::new_looped(std::io::BufReader::new(file)) {
                    Ok(src) => {
                        let s = rodio::Player::connect_new(&stream.mixer());
                        s.set_volume(0.0 as _);
                        s.append(src);
                        sink = Some(s);
                        fading_in = Some(Instant::now());
                    }
                    Err(e) => tracing::debug!("can't play {}: {e}", path.display()),
                }
            }
            Some(Cmd::Stop) => {
                if let Some(s) = sink.take() {
                    fade_out(&s);
                }
                fading_in = None;
            }
            Some(Cmd::Volume(v)) => {
                volume = v;
                if fading_in.is_none()
                    && let Some(s) = &sink
                {
                    s.set_volume(volume as _);
                }
            }
            None => {}
        }
        if let (Some(start), Some(s)) = (fading_in, &sink) {
            s.set_volume(fade(start.elapsed(), FADE_IN, volume) as _);
            if start.elapsed() >= FADE_IN {
                fading_in = None;
            }
        } else if sink.is_none() {
            fading_in = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fades_are_linear_and_clamped() {
        let len = Duration::from_millis(1000);
        assert_eq!(fade(Duration::ZERO, len, 0.8), 0.0);
        assert!((fade(Duration::from_millis(500), len, 0.8) - 0.4).abs() < 1e-6);
        assert_eq!(fade(Duration::from_secs(5), len, 0.8), 0.8);
        assert_eq!(fade(Duration::ZERO, Duration::ZERO, 0.5), 0.5);
    }

    #[test]
    fn player_survives_missing_files() {
        // Works with or without an audio device; must never panic or block.
        let p = Player::new(0.5);
        p.play(PathBuf::from("/nonexistent/track.mp3"));
        p.set_volume(0.2);
        p.stop();
    }
}
