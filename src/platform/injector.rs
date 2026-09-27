/*
 * @file injector.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief TextInjector trait and its default implementation: simulated key input
 * @date 2026-09-26
 *
 * @copyright Copyright (c) 2026
 */

use enigo::{Enigo, Keyboard, Settings};
use tracing::{debug, info};

use crate::error::Result;

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[PlatformInjector]";

/// What puts the English text where the user is typing.
///
/// `&mut self` rather than `&self`: an implementation owns a live connection to the display
/// server and may keep keys pressed between calls, so one injector has one owner instead of
/// being shared. Nothing here takes a window: input goes to whatever holds the keyboard focus
/// at the moment of the call, which is why the focus has to be handed back first (see
/// `platform::focus`).
pub trait TextInjector {
    /// Types `text` as if it came from the keyboard, into whichever window has the focus.
    fn type_text(&mut self, text: &str) -> Result<()>;
}

/// The default implementation: key events simulated through XTEST, which is what Enigo uses on
/// Linux/X11. No application-specific adaptation — the keys are indistinguishable from a real
/// keyboard's, so every toolkit accepts them.
pub struct KeyboardInjector {
    enigo: Enigo,
}

impl KeyboardInjector {
    /// Opens whatever the simulation needs. Without an X11 display this fails, and the caller
    /// is expected to carry on with a program that cannot type.
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default())?;
        info!(target: LOG_ID, "Keyboard injection is ready.");
        Ok(Self { enigo })
    }
}

impl TextInjector for KeyboardInjector {
    fn type_text(&mut self, text: &str) -> Result<()> {
        debug!(target: LOG_ID, "Typing {} character(s).", text.chars().count());
        self.enigo.text(text)?;
        Ok(())
    }
}
