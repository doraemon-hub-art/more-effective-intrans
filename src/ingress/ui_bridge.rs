/*
 * @file ui_bridge.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief The only place allowed to read or write Slint properties
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tracing::{info, warn};

use crate::AppWindow;

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[IngressUiBridge]";

/// Registers the handler that runs when the user submits the input field with Enter.
///
/// The payload is the text as typed. Clearing the field happens inside the .slint file, so
/// the handler needs no handle back to the window — that would otherwise tie the component
/// to its own callback in a reference cycle.
pub fn on_submit(ui: &AppWindow, handler: impl Fn(String) + 'static) {
    ui.on_submitted(move |text: SharedString| handler(text.to_string()));
}

/// Registers the handler that runs when the user picks a candidate, by digit key or click.
pub fn on_pick(ui: &AppWindow, handler: impl Fn(String) + 'static) {
    ui.on_picked(move |word: SharedString| handler(word.to_string()));
}

/// Shows the panel if it is hidden and hides it if it is shown.
///
/// Reports whether the panel ended up visible, so the caller can log the outcome: with the
/// panel gone, the tray icon is all that is left keeping the process alive.
pub fn toggle_visible(ui: &AppWindow) -> Result<bool, slint::PlatformError> {
    if ui.window().is_visible() {
        ui.hide()?;
        Ok(false)
    } else {
        ui.show()?;
        Ok(true)
    }
}

/// What the hotkey did to the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAction {
    /// It was not on screen: it is now.
    Show,
    /// It was on screen with another window on top of it: it was put in front.
    Raise,
    /// It was on screen and in front already: it is gone.
    Hide,
}

/// Applies the hotkey rule to the panel and reports which of the three cases it was.
///
/// Called from a thread other than the one running the event loop — the hotkey listener — which
/// must not touch the component: `upgrade_in_event_loop` is the hop onto the owning thread that
/// makes reading and writing the panel legal, and it is also where the panel's own visibility can
/// be read. An error means there is no event loop left to hop to, which happens while shutting
/// down.
///
/// `panel_is_frontmost` comes from the X11 side instead, because only the window manager knows the
/// stacking order; whether the panel is on top has to be read there (see `Focus::is_frontmost`).
/// `before_show` runs on the UI thread just before the panel goes on screen, and only when it does
/// go on screen: showing it moves the keyboard focus, so whatever held that focus — the window the
/// text has to go back to later — can only be read in there. `after_change` is then called with
/// what was applied, which is where the caller follows up with the X11 work that needs the window
/// to exist on screen: putting it in front of the others and giving it the keyboard.
pub fn hotkey_panel(
    ui: &slint::Weak<AppWindow>,
    panel_is_frontmost: bool,
    before_show: impl FnOnce() + Send + 'static,
    after_change: impl FnOnce(PanelAction) + Send + 'static,
) -> Result<(), slint::EventLoopError> {
    ui.upgrade_in_event_loop(move |ui| {
        // Ready for the next word: whatever the last round left behind is only in the way.
        set_candidates(&ui, &[]);
        ui.set_input_text(SharedString::default());

        let action = if !ui.window().is_visible() {
            before_show();
            if let Err(err) = ui.show() {
                warn!(target: LOG_ID, "Could not show the panel ({err}).");
                return;
            }
            info!(target: LOG_ID, "Panel shown.");
            PanelAction::Show
        } else if panel_is_frontmost {
            if let Err(err) = ui.hide() {
                warn!(target: LOG_ID, "Could not hide the panel ({err}).");
                return;
            }
            info!(target: LOG_ID, "Panel hidden.");
            PanelAction::Hide
        } else {
            info!(target: LOG_ID, "Panel was behind another window, bringing it forward.");
            PanelAction::Raise
        };

        after_change(action);
    })
}

/// Replaces the candidate row with `words`; passing an empty slice hides the row again
/// (the window height follows the row, see app_window.slint).
pub fn set_candidates(ui: &AppWindow, words: &[String]) {
    let items: Vec<SharedString> = words
        .iter()
        .map(|word| SharedString::from(word.as_str()))
        .collect();
    ui.set_candidates(ModelRc::new(VecModel::from(items)));
}
