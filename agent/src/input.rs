//! Owns the single `Enigo` instance on a dedicated OS thread and applies
//! input commands serially. enigo isn't `Send`, so the rest of the async
//! server talks to it only through an `mpsc` channel.

use crossbeam_channel::Receiver;

use enigo::{
    Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings,
};

/// A single low-level input action requested by a paired phone.
#[derive(Debug, Clone)]
pub enum InputCmd {
    /// Relative cursor move in pixels.
    Move { dx: i32, dy: i32 },
    /// A click (optionally a double-click) of the given button.
    Click { button: MouseButton, double: bool },
    /// Press and hold a button (drag start).
    Down { button: MouseButton },
    /// Release a held button (drag end).
    Up { button: MouseButton },
    /// Scroll wheel, in notches. Positive dy scrolls down, positive dx right.
    Scroll { dx: i32, dy: i32 },
    /// Inject a string at the current keyboard focus (unicode-safe).
    Text(String),
    /// Tap a named special key (enter, backspace, tab, ...).
    Key(SpecialKey),
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

/// Run the input loop. Blocks until the channel is closed. Intended to own a
/// dedicated thread because `Enigo` is not `Send`.
pub fn run(rx: Receiver<InputCmd>) {
    let mut enigo = match Enigo::new(&Settings::default()) {
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

    while let Ok(cmd) = rx.recv() {
        if let Err(e) = apply(&mut enigo, cmd) {
            eprintln!("[pointflow] input error: {e}");
        }
    }
}

fn apply(enigo: &mut Enigo, cmd: InputCmd) -> Result<(), enigo::InputError> {
    match cmd {
        InputCmd::Move { dx, dy } => enigo.move_mouse(dx, dy, Coordinate::Rel),
        InputCmd::Click { button, double } => {
            enigo.button(button.into(), Direction::Click)?;
            if double {
                enigo.button(button.into(), Direction::Click)?;
            }
            Ok(())
        }
        InputCmd::Down { button } => enigo.button(button.into(), Direction::Press),
        InputCmd::Up { button } => enigo.button(button.into(), Direction::Release),
        InputCmd::Scroll { dx, dy } => {
            if dy != 0 {
                enigo.scroll(dy, Axis::Vertical)?;
            }
            if dx != 0 {
                enigo.scroll(dx, Axis::Horizontal)?;
            }
            Ok(())
        }
        InputCmd::Text(s) => enigo.text(&s),
        InputCmd::Key(k) => enigo.key(k.into(), Direction::Click),
    }
}
