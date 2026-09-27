/*
 * @file tray.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Tray icon lifecycle and context menu events
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use slint::ComponentHandle;
use tracing::{info, warn};

use crate::coordinator::config;
use crate::ingress::ui_bridge;
use crate::{AppWindow, TrayIcon};

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[IngressTray]";

/// Creates the tray icon and wires its menu up.
///
/// `source` is the translation source the menu is to mark as the picked one; it comes from the
/// configuration file.
///
/// The returned handle has to stay alive for as long as the icon is wanted: dropping it
/// removes the icon, and with the panel hidden that would leave nothing keeping the event
/// loop alive. `None` means the platform gave us no tray, in which case the caller keeps
/// the panel itself visible.
pub fn start(ui: &AppWindow, source: &str) -> Option<TrayIcon> {
    let tray = match TrayIcon::new() {
        Ok(tray) => tray,
        Err(err) => {
            warn!(target: LOG_ID, "No tray icon available ({err}).");
            return None;
        }
    };

    tray.set_source(source.into());

    // The callbacks run on the Slint event loop thread, and the Weak handle is the way
    // back to the panel without the tray owning it.
    let weak = ui.as_weak();
    tray.on_toggle_window(move || {
        let Some(ui) = weak.upgrade() else { return };
        match ui_bridge::toggle_visible(&ui) {
            Ok(true) => info!(target: LOG_ID, "Panel shown."),
            Ok(false) => info!(target: LOG_ID, "Panel hidden."),
            Err(err) => warn!(target: LOG_ID, "Could not toggle the panel ({err})."),
        }
    });

    // Picking a source only moves the mark in the menu for now: reading the configuration of the
    // picked source is the config layer's job, and writing the choice back to the file is a step
    // of its own.
    let weak_tray = tray.as_weak();
    tray.on_select_source(move |source| {
        info!(target: LOG_ID, "Translation source {source} picked.");
        if let Some(tray) = weak_tray.upgrade() {
            tray.set_source(source);
        }
    });

    // Opening the configuration means handing it to whatever the desktop uses for text files,
    // which is xdg-open's job. The call does not wait: the editor outlives us either way.
    tray.on_open_config(|| {
        let path = match config::ensure_user_file() {
            Ok(path) => path,
            Err(error) => {
                warn!(target: LOG_ID, "Could not prepare the configuration file: {error}.");
                return;
            }
        };

        match std::process::Command::new("xdg-open").arg(&path).spawn() {
            Ok(_) => info!(target: LOG_ID, "Opened {}.", path.display()),
            Err(error) => warn!(target: LOG_ID, "Could not open {}: {error}.", path.display()),
        }
    });

    tray.on_quit_app(|| {
        info!(target: LOG_ID, "Quit picked from the tray menu.");
        let _ = slint::quit_event_loop();
    });

    if let Err(err) = tray.show() {
        warn!(target: LOG_ID, "Could not put the tray icon up ({err}).");
        return None;
    }
    info!(target: LOG_ID, "Tray icon is up.");

    Some(tray)
}
