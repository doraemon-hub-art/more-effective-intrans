/*
 * @file ui_bridge.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief The only place allowed to read or write Slint properties
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::AppWindow;

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

/// Replaces the candidate row with `words`; passing an empty slice hides the row again
/// (the window height follows the row, see app_window.slint).
pub fn set_candidates(ui: &AppWindow, words: &[String]) {
    let items: Vec<SharedString> = words
        .iter()
        .map(|word| SharedString::from(word.as_str()))
        .collect();
    ui.set_candidates(ModelRc::new(VecModel::from(items)));
}
