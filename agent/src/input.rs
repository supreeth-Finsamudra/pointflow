//! Owns the single `Enigo` instance on a dedicated OS thread and applies
//! input commands serially. enigo isn't `Send`, so the rest of the async
//! server talks to it only through an `mpsc` channel.
//!
//! Cursor movement and scrolling carry sub-pixel deltas; we keep fractional
//! accumulators so slow, precise motion isn't lost to integer rounding. Scroll
//! is emitted as native pixel-unit `CGEvent`s (enigo only does coarse lines).

use crossbeam_channel::Receiver;

use core_graphics::event::{CGEvent, CGEventTapLocation, ScrollEventUnit};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};

/// A single low-level input action requested by a paired phone.
#[derive(Debug, Clone)]
pub enum InputCmd {
    /// Relative cursor move in (sub-)pixels.
    Move { dx: f64, dy: f64 },
    /// A click (optionally a double-click) of the given button.
    Click { button: MouseButton, double: bool },
    /// Press and hold a button (drag start).
    Down { button: MouseButton },
    /// Release a held button (drag end).
    Up { button: MouseButton },
    /// Scroll in (sub-)pixels. Positive dy follows the macOS wheel convention.
    Scroll { dx: f64, dy: f64 },
    /// Inject a string at the current keyboard focus (unicode-safe).
    Text(String),
    /// Tap a named special key (enter, backspace, tab, ...).
    Key(SpecialKey),
    /// Hold modifiers, tap a key, release — e.g. Ctrl+Left, ⌘C.
    Chord { mods: Vec<Modifier>, key: ChordKey },
}

#[derive(Debug, Clone, Copy)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl From<MouseButton> for Button {
    fn from(b: MouseButton) -> Self {
        match b {
            MouseButton::Left => Button::Left,
            MouseButton::Right => Button::Right,
            MouseButton::Middle => Button::Middle,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SpecialKey {
    Enter,
    Backspace,
    Tab,
    Escape,
    Space,
    Up,
    Down,
    Left,
    Right,
    Delete,
}

impl From<SpecialKey> for Key {
    fn from(k: SpecialKey) -> Self {
        match k {
            SpecialKey::Enter => Key::Return,
            SpecialKey::Backspace => Key::Backspace,
            SpecialKey::Tab => Key::Tab,
            SpecialKey::Escape => Key::Escape,
            SpecialKey::Space => Key::Space,
            SpecialKey::Up => Key::UpArrow,
            SpecialKey::Down => Key::DownArrow,
            SpecialKey::Left => Key::LeftArrow,
            SpecialKey::Right => Key::RightArrow,
            SpecialKey::Delete => Key::Delete,
        }
    }
}

impl SpecialKey {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "enter" | "return" => SpecialKey::Enter,
            "backspace" | "back" => SpecialKey::Backspace,
            "tab" => SpecialKey::Tab,
            "escape" | "esc" => SpecialKey::Escape,
            "space" => SpecialKey::Space,
            "up" => SpecialKey::Up,
            "down" => SpecialKey::Down,
            "left" => SpecialKey::Left,
            "right" => SpecialKey::Right,
            "delete" | "del" => SpecialKey::Delete,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Modifier {
    Cmd,
    Ctrl,
    Alt,
    Shift,
}

impl Modifier {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "cmd" | "meta" | "command" | "super" | "win" => Modifier::Cmd,
            "ctrl" | "control" => Modifier::Ctrl,
            "alt" | "option" | "opt" => Modifier::Alt,
            "shift" => Modifier::Shift,
            _ => return None,
        })
    }
}

impl From<Modifier> for Key {
    fn from(m: Modifier) -> Self {
        match m {
            Modifier::Cmd => Key::Meta,
            Modifier::Ctrl => Key::Control,
            Modifier::Alt => Key::Alt,
            Modifier::Shift => Key::Shift,
        }
    }
}

/// The key a chord taps: either a named special key or a single character.
#[derive(Debug, Clone, Copy)]
pub enum ChordKey {
    Special(SpecialKey),
    Char(char),
}

impl ChordKey {
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(k) = SpecialKey::parse(s) {
            return Some(ChordKey::Special(k));
        }
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(ChordKey::Char(c)),
            _ => None,
        }
    }

    fn to_key(self) -> Key {
        match self {
            ChordKey::Special(k) => k.into(),
            ChordKey::Char(c) => Key::Unicode(c),
        }
    }
}

/// Holds the input engine plus sub-pixel remainders for move and scroll.
struct Engine {
    enigo: Enigo,
    move_x: f64,
    move_y: f64,
    scroll_x: f64,
    scroll_y: f64,
}

/// Run the input loop. Blocks until the channel is closed. Intended to own a
/// dedicated thread because `Enigo` is not `Send`.
pub fn run(rx: Receiver<InputCmd>) {
    let enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "[pointflow] FATAL: could not initialize input engine: {e}\n\
                 Grant Accessibility permission to this app in \
                 System Settings → Privacy & Security → Accessibility, then restart."
            );
            return;
        }
    };
    let mut engine = Engine {
        enigo,
        move_x: 0.0,
        move_y: 0.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };

    while let Ok(cmd) = rx.recv() {
        if let Err(e) = engine.apply(cmd) {
            eprintln!("[pointflow] input error: {e}");
        }
    }
}

fn es(e: enigo::InputError) -> String {
    e.to_string()
}

impl Engine {
    fn apply(&mut self, cmd: InputCmd) -> Result<(), String> {
        match cmd {
            InputCmd::Move { dx, dy } => self.move_rel(dx, dy),
            InputCmd::Click { button, double } => {
                self.enigo.button(button.into(), Direction::Click).map_err(es)?;
                if double {
                    self.enigo.button(button.into(), Direction::Click).map_err(es)?;
                }
                Ok(())
            }
            InputCmd::Down { button } => {
                self.enigo.button(button.into(), Direction::Press).map_err(es)
            }
            InputCmd::Up { button } => {
                self.enigo.button(button.into(), Direction::Release).map_err(es)
            }
            InputCmd::Scroll { dx, dy } => self.scroll_px(dx, dy),
            InputCmd::Text(s) => self.enigo.text(&s).map_err(es),
            InputCmd::Key(k) => self.enigo.key(k.into(), Direction::Click).map_err(es),
            InputCmd::Chord { mods, key } => self.chord(&mods, key),
        }
    }

    /// Move relatively, carrying sub-pixel remainder so slow drags stay precise.
    fn move_rel(&mut self, dx: f64, dy: f64) -> Result<(), String> {
        self.move_x += dx;
        self.move_y += dy;
        let ix = self.move_x.trunc() as i32;
        let iy = self.move_y.trunc() as i32;
        if ix != 0 || iy != 0 {
            self.move_x -= ix as f64;
            self.move_y -= iy as f64;
            self.enigo
                .move_mouse(ix, iy, Coordinate::Rel)
                .map_err(es)?;
        }
        Ok(())
    }

    /// Emit a native pixel-unit scroll event, carrying sub-pixel remainder.
    fn scroll_px(&mut self, dx: f64, dy: f64) -> Result<(), String> {
        self.scroll_x += dx;
        self.scroll_y += dy;
        let ix = self.scroll_x.trunc() as i32;
        let iy = self.scroll_y.trunc() as i32;
        if ix == 0 && iy == 0 {
            return Ok(());
        }
        self.scroll_x -= ix as f64;
        self.scroll_y -= iy as f64;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| "could not create scroll event source".to_string())?;
        // wheel1 = vertical, wheel2 = horizontal.
        let event = CGEvent::new_scroll_event(source, ScrollEventUnit::PIXEL, 2, iy, ix, 0)
            .map_err(|_| "could not create scroll event".to_string())?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn chord(&mut self, mods: &[Modifier], key: ChordKey) -> Result<(), String> {
        let keys: Vec<Key> = mods.iter().map(|m| Key::from(*m)).collect();
        for k in &keys {
            self.enigo.key(*k, Direction::Press).map_err(es)?;
        }
        let result = self.enigo.key(key.to_key(), Direction::Click).map_err(es);
        // Always release modifiers, even if the main key failed.
        for k in keys.iter().rev() {
            let _ = self.enigo.key(*k, Direction::Release);
        }
        result
    }
}
