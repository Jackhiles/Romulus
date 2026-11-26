//! Native controller input. WebKitGTK's Gamepad API is unreliable on Linux, so the pad
//! is read here with gilrs (evdev) and sent to the UI as `pad` events.
//!
//! Steam Input (Gaming Mode on the Deck) adds a virtual Xbox-style pad next to the real
//! one. Like KlikCore2's `pad_source.rs`, the virtual pad is preferred so the user's Steam
//! layout applies.
//!
//! Directions repeat while held (first repeat after [`REPEAT_DELAY`], then every
//! [`REPEAT_RATE`]). Buttons fire once per press.

use gilrs::{Axis, Button, Gilrs};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const POLL: Duration = Duration::from_millis(8);
const REPEAT_DELAY: Duration = Duration::from_millis(320);
const REPEAT_RATE: Duration = Duration::from_millis(70);
const STICK: f32 = 0.55;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Lt,
    Rt,
    Start,
    Select,
}

#[derive(Debug, Clone, Serialize)]
pub struct PadEvent {
    pub action: Action,
    pub repeat: bool,
}

const BUTTONS: [(Button, Action); 10] = [
    (Button::South, Action::A),
    (Button::East, Action::B),
    (Button::West, Action::X),
    (Button::North, Action::Y),
    (Button::LeftTrigger, Action::Lb),
    (Button::RightTrigger, Action::Rb),
    (Button::LeftTrigger2, Action::Lt),
    (Button::RightTrigger2, Action::Rt),
    (Button::Start, Action::Start),
    (Button::Select, Action::Select),
];

/// Which actions are held right now on the preferred pad.
fn held(g: &Gilrs) -> Vec<Action> {
    let pads: Vec<_> = g.gamepads().filter(|(_, p)| p.is_connected()).collect();
    let is_virtual = |n: &str| {
        let n = n.to_ascii_lowercase();
        n.contains("steam") && n.contains("virtual")
    };
    let Some((_, p)) = pads
        .iter()
        .find(|(_, p)| is_virtual(p.name()) || is_virtual(p.os_name()))
        .or(pads.first())
    else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(4);
    let (x, y) = (p.value(Axis::LeftStickX), p.value(Axis::LeftStickY));
    if p.is_pressed(Button::DPadUp) || y > STICK {
        out.push(Action::Up);
    }
    if p.is_pressed(Button::DPadDown) || y < -STICK {
        out.push(Action::Down);
    }
    if p.is_pressed(Button::DPadLeft) || x < -STICK {
        out.push(Action::Left);
    }
    if p.is_pressed(Button::DPadRight) || x > STICK {
        out.push(Action::Right);
    }
    for (b, a) in BUTTONS {
        if p.is_pressed(b) {
            out.push(a);
        }
    }
    out
}

fn repeats(a: Action) -> bool {
    matches!(
        a,
        Action::Up | Action::Down | Action::Left | Action::Right | Action::Lb | Action::Rb
    )
}

/// Start the reader thread. While `paused` is set (a game is running, or the window is in
/// the background) nothing is sent.
pub fn spawn(app: AppHandle, paused: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("gamepad".into())
        .spawn(move || {
            let mut g = match Gilrs::new() {
                Ok(g) => g,
                Err(e) => {
                    tracing::error!("gamepad input unavailable: {e}");
                    return;
                }
            };
            for (_, p) in g.gamepads() {
                tracing::info!("gamepad: {} ({})", p.name(), p.os_name());
            }
            // When each held action was pressed, and when it last fired.
            let mut down: Vec<(Action, Instant, Instant)> = Vec::new();
            loop {
                while let Some(ev) = g.next_event() {
                    if let gilrs::EventType::Connected = ev.event {
                        tracing::info!("gamepad connected: {}", g.gamepad(ev.id).name());
                    }
                }
                let now = Instant::now();
                let held_now = held(&g);
                if paused.load(Ordering::Relaxed) {
                    // Treat everything as already handled so nothing fires on resume.
                    down = held_now
                        .iter()
                        .map(|&a| (a, now, now + Duration::from_secs(3600)))
                        .collect();
                    std::thread::sleep(POLL * 4);
                    continue;
                }
                down.retain(|(a, _, _)| held_now.contains(a));
                for a in held_now {
                    match down.iter_mut().find(|(d, _, _)| *d == a) {
                        None => {
                            down.push((a, now, now));
                            let _ = app.emit(
                                "pad",
                                PadEvent {
                                    action: a,
                                    repeat: false,
                                },
                            );
                        }
                        Some((_, since, last)) => {
                            if repeats(a)
                                && now >= *since + REPEAT_DELAY
                                && now >= *last + REPEAT_RATE
                            {
                                *last = now;
                                let _ = app.emit(
                                    "pad",
                                    PadEvent {
                                        action: a,
                                        repeat: true,
                                    },
                                );
                            }
                        }
                    }
                }
                std::thread::sleep(POLL);
            }
        })
        .expect("spawn gamepad thread");
}
