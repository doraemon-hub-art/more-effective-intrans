/*
 * @file hotkey.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Global hotkey listener, the primary event source of the application
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tracing::{info, warn};

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[IngressHotkey]";

// Alt+; as the trigger. The design notes mention a bare ";" but a global grab on a
// printable key without a modifier would swallow that key in every application, so it
// keeps a modifier for now; it becomes a config.toml entry once the config layer exists.
const TRIGGER_MODS: Modifiers = Modifiers::ALT;
const TRIGGER_KEY: Code = Code::Semicolon;

/// Registers the trigger hotkey and listens for it on a background thread.
///
/// The returned manager has to stay alive for as long as the hotkey is wanted: dropping
/// it unregisters the hotkey. `None` means this platform cannot provide global hotkeys
/// (no X11 display, for instance) and the caller simply continues without them.
///
/// `on_trigger` runs on the listener thread, which is not the one running the Slint event
/// loop; a Slint component may only be touched from its own thread, so the callback has to
/// hand its work over there itself (see `ui_bridge::toggle_visible_in_event_loop`).
pub fn start(on_trigger: impl Fn() + Send + 'static) -> Option<GlobalHotKeyManager> {
    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(err) => {
            warn!(target: LOG_ID, "Global hotkeys unavailable, continuing without them ({err}).");
            return None;
        }
    };

    let hotkey = HotKey::new(Some(TRIGGER_MODS), TRIGGER_KEY);
    if let Err(err) = manager.register(hotkey) {
        warn!(target: LOG_ID, "Could not register {} ({err}).", hotkey.into_string());
        return None;
    }
    info!(target: LOG_ID, "Registered global hotkey {}.", hotkey.into_string());

    // global-hotkey pumps X11 events on its own thread; this thread only waits on the
    // channel it fills, which keeps the Slint event loop free.
    std::thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.recv() {
            if event.state == HotKeyState::Pressed {
                info!(target: LOG_ID, "Hotkey pressed (id {}).", event.id);
                on_trigger();
            }
        }
    });

    Some(manager)
}
