//! Wire protocol: JSON messages the phone sends over the WebSocket.

use serde::Deserialize;

use crate::input::{InputCmd, MouseButton, SpecialKey};

/// A message from the phone. The first message on a connection MUST be `Auth`.
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum ClientMsg {
    /// Pairing handshake — must match the agent's session token.
    Auth { token: String },
    /// Relative cursor move.
    Move { dx: f64, dy: f64 },
    /// Click (or double-click) a button. Defaults to left.
    Click {
        #[serde(default)]
        button: ButtonName,
        #[serde(default)]
        double: bool,
    },
    /// Press and hold a button (drag start).
    Down {
        #[serde(default)]
        button: ButtonName,
    },
    /// Release a held button (drag end).
    Up {
        #[serde(default)]
        button: ButtonName,
    },
    /// Scroll wheel in notches.
    Scroll { dx: f64, dy: f64 },
    /// Inject text at the current focus.
    Text { s: String },
    /// Tap a named special key.
    Key { k: String },
    /// Keep-alive; no effect.
    Ping,
}

#[derive(Debug, Default, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ButtonName {
    #[default]
    Left,
    Right,
    Middle,
}

impl From<ButtonName> for MouseButton {
    fn from(b: ButtonName) -> Self {
        match b {
            ButtonName::Left => MouseButton::Left,
            ButtonName::Right => MouseButton::Right,
            ButtonName::Middle => MouseButton::Middle,
        }
    }
}

impl ClientMsg {
    /// Convert an authenticated control message into an `InputCmd`.
    /// Returns `None` for `Auth`/`Ping` (no input action).
    pub fn into_cmd(self) -> Option<InputCmd> {
        match self {
            ClientMsg::Auth { .. } | ClientMsg::Ping => None,
            ClientMsg::Move { dx, dy } => Some(InputCmd::Move {
                dx: dx.round() as i32,
                dy: dy.round() as i32,
            }),
            ClientMsg::Click { button, double } => Some(InputCmd::Click {
                button: button.into(),
                double,
            }),
            ClientMsg::Down { button } => Some(InputCmd::Down {
                button: button.into(),
            }),
            ClientMsg::Up { button } => Some(InputCmd::Up {
                button: button.into(),
            }),
            ClientMsg::Scroll { dx, dy } => Some(InputCmd::Scroll {
                dx: dx.round() as i32,
                dy: dy.round() as i32,
            }),
            ClientMsg::Text { s } => Some(InputCmd::Text(s)),
            ClientMsg::Key { k } => SpecialKey::parse(&k).map(InputCmd::Key),
        }
    }
}
