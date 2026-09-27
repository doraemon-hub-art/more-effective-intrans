/*
 * @file main.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Process entry: load configuration, start the Tokio runtime and run the library
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::sync::{Arc, Mutex};

use more_effective_intrans::coordinator::config::Config;
use more_effective_intrans::ingress::{hotkey, tray, ui_bridge};
use more_effective_intrans::pipeline::dict;
use more_effective_intrans::pipeline::providers::traditional::BaiduTranslator;
use more_effective_intrans::pipeline::translator::Translator;
use more_effective_intrans::platform::focus::{self, Focus, WindowId};
use more_effective_intrans::platform::injector::{KeyboardInjector, TextInjector};
use more_effective_intrans::{AppWindow, init_logging};
use slint::ComponentHandle;
use tracing::{info, warn};

// Per-file logger id: it replaces the long module path in every log line, so a line
// immediately tells you which file it came from. Convention: UpperCamelCase version of
// the file's path under src/, wrapped in brackets.
const LOG_ID: &str = "[Main]";

// The panel's window title: the X server knows the panel's window by nothing else, so this is how
// it is found again when it has to be put in front (see `panel_is_frontmost`). Kept in sync with
// ui/app_window.slint, where the title is set.
const PANEL_TITLE: &str = "more-effective-intrans";

/// Whether our own panel window is the topmost one on the desktop.
///
/// Every failure answers "no" on purpose: the hotkey then shows the panel or brings it forward,
/// which is the harmless direction — answering "yes" wrongly would hide a panel the user is
/// reaching for.
fn panel_is_frontmost(focus: Option<&Arc<Focus>>) -> bool {
    let Some(focus) = focus else {
        return false;
    };
    let Ok(Some(panel)) = focus.find_window_by_title(PANEL_TITLE) else {
        return false;
    };

    focus.is_frontmost(panel).unwrap_or(false)
}

fn main() -> Result<(), slint::PlatformError> {
    init_logging();
    info!(target: LOG_ID, version = env!("CARGO_PKG_VERSION"), "Starting up.");

    // One runtime for the whole process. Every request runs on it rather than on the UI
    // thread, so the panel keeps reacting while a translation is in flight.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("intrans-net")
        .build()
        .expect("the network runtime must start");
    let network = runtime.handle().clone();

    // The configuration file says which translation source is used and what that source needs.
    // A file that is there but unreadable is reported and stepped over: a typo belongs in the
    // log, it is not worth refusing to start over.
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            warn!(target: LOG_ID, "Using the default configuration: {error}.");
            Config::default()
        }
    };
    info!(target: LOG_ID, "Translation source: {}.", config.translation.selected);

    // A source without credentials is simply off. The flow then stops after the lookup instead
    // of failing on every word, and the log says so once at startup.
    let baidu = &config.translation.baidu;
    let translator: Option<Arc<BaiduTranslator>> =
        match BaiduTranslator::from_settings(&baidu.appid, &baidu.secret, &baidu.endpoint) {
            Ok(translator) => Some(Arc::new(translator)),
            Err(error) => {
                warn!(target: LOG_ID, "Translation is off: {error}.");
                None
            }
        };

    // Reading the window focus and simulating keys are the two halves of "give the text back to
    // where the user was typing". Either can be missing on a machine without X11, and the flow
    // then keeps working up to the log line, exactly as it does without a translator.
    let focus: Option<Arc<Focus>> = match Focus::connect() {
        Ok(focus) => Some(Arc::new(focus)),
        Err(error) => {
            warn!(target: LOG_ID, "Window focus is off: {error}.");
            None
        }
    };

    let injector: Option<Arc<Mutex<KeyboardInjector>>> = match KeyboardInjector::new() {
        Ok(injector) => Some(Arc::new(Mutex::new(injector))),
        Err(error) => {
            warn!(target: LOG_ID, "Typing is off: {error}.");
            None
        }
    };

    // The window that had the keyboard when the hotkey was pressed. The panel takes that focus
    // a moment later, so this is the only record of where the English text has to end up.
    let target: Arc<Mutex<Option<WindowId>>> = Arc::new(Mutex::new(None));

    // Everything one translated word needs. All handles inside are `Arc`s, so each request
    // carries its own copy rather than borrowing from the event loop.
    let delivery = Delivery {
        translator: translator.clone(),
        focus: focus.clone(),
        injector: injector.clone(),
        target: Arc::clone(&target),
    };

    let dictionary = dict::Dict::load();

    // Build the window described in ui/app_window.slint. It is not shown yet: from here on
    // the tray icon is what puts it on screen, once it is asked for.
    let ui = AppWindow::new()?;
    info!(target: LOG_ID, "Window created.");

    // Bound to a name (rather than dropped) because dropping the manager unregisters
    // the hotkey again. Everything in here runs on the listener thread, and the panel itself is
    // only ever touched through the event loop hop in ui_bridge.
    let _hotkeys = hotkey::start({
        let weak = ui.as_weak();
        let focus = focus.clone();
        let target = Arc::clone(&target);
        let runtime = network.clone();

        move || {
            // Read on this thread: the stacking order is the window manager's business, and the
            // round trips would only hold the UI thread up.
            let frontmost = panel_is_frontmost(focus.as_ref());

            // Runs on the UI thread, but touches no Slint property: it is in this hop only
            // because the panel is about to take the keyboard focus away.
            let capture = {
                let focus = focus.clone();
                let target = Arc::clone(&target);

                move || {
                    let Some(focus) = focus.as_ref() else { return };
                    match focus.current() {
                        Ok(Some(window)) => {
                            *target.lock().unwrap() = Some(window);
                            info!(target: LOG_ID, "The keyboard was in 0x{window:x}.");
                        }
                        Ok(None) => {
                            warn!(target: LOG_ID, "Nothing had the keyboard focus; the text will have nowhere to go.")
                        }
                        Err(error) => {
                            warn!(target: LOG_ID, "Could not read the focused window ({error}).")
                        }
                    }
                }
            };

            // Runs on the UI thread as well, but the work it starts does not: the panel may not be
            // mapped yet right after being shown, and finding it plus the retries take a moment.
            let after_change = {
                let focus = focus.clone();
                let runtime = runtime.clone();

                move |action: ui_bridge::PanelAction| {
                    if action == ui_bridge::PanelAction::Hide {
                        return;
                    }

                    let Some(focus) = focus.clone() else { return };

                    runtime.spawn_blocking(move || match focus.find_window_by_title(PANEL_TITLE) {
                        Ok(Some(panel)) => {
                            if let Err(error) = focus.bring_to_front(panel) {
                                warn!(target: LOG_ID, "Could not put the panel in front ({error}).");
                            }
                        }
                        Ok(None) => warn!(target: LOG_ID, "The panel window is not on screen yet."),
                        Err(error) => {
                            warn!(target: LOG_ID, "Could not look for the panel window ({error}).")
                        }
                    });
                }
            };

            if let Err(error) = ui_bridge::hotkey_panel(&weak, frontmost, capture, after_change) {
                warn!(target: LOG_ID, "The hotkey could not reach the panel ({error}).");
            }
        }
    });

    // The closures need their way back to the window, and a Weak handle keeps the component
    // from owning its own callbacks.
    let weak = ui.as_weak();
    let submit_network = network.clone();
    let submit_delivery = delivery.clone();
    ui_bridge::on_submit(&ui, move |input| {
        let candidates = dictionary.lookup(&input);
        match candidates.as_slice() {
            [] => info!(target: LOG_ID, "Lookup {input:?} matched nothing."),
            [only] => {
                info!(target: LOG_ID, "Lookup {input:?} -> {}, a single match.", only.word);
                spawn_translation(&submit_network, submit_delivery.clone(), only.word.clone());
            }
            several => {
                let words: Vec<String> = several.iter().map(|c| c.word.clone()).collect();
                info!(target: LOG_ID, "Lookup {input:?} -> {}, waiting for a pick.", words.join(", "));
                if let Some(ui) = weak.upgrade() {
                    ui_bridge::set_candidates(&ui, &words);
                }
            }
        }
    });

    let weak = ui.as_weak();
    let pick_delivery = delivery.clone();
    ui_bridge::on_pick(&ui, move |word| {
        spawn_translation(&network, pick_delivery.clone(), word);
        if let Some(ui) = weak.upgrade() {
            ui_bridge::set_candidates(&ui, &[]);
        }
    });

    // Same reason as the hotkey manager above: dropping the handle takes the icon away and
    // with it the only thing keeping the event loop alive, since the panel stays hidden.
    let _tray = tray::start(&ui, &config.translation.selected);
    if _tray.is_none() {
        warn!(target: LOG_ID, "Starting with the panel visible instead of resident.");
        return ui.run();
    }

    info!(target: LOG_ID, "Entering the event loop.");
    let result = slint::run_event_loop();
    info!(target: LOG_ID, "Event loop finished, shutting down.");

    result
}

/// Everything one translated word needs to be written back where it belongs.
///
/// Cloned per request — the handles inside are all `Arc`s. The two `Option`s are the layers that
/// can be missing at run time: no provider without credentials, no focus control and no input
/// simulation without an X11 display.
#[derive(Clone)]
struct Delivery {
    translator: Option<Arc<BaiduTranslator>>,
    focus: Option<Arc<Focus>>,
    injector: Option<Arc<Mutex<KeyboardInjector>>>,
    /// The window captured when the hotkey was pressed.
    target: Arc<Mutex<Option<WindowId>>>,
}

impl Delivery {
    /// Gives the keyboard back to the window captured at the hotkey and types `text` there.
    ///
    /// Keys cannot be addressed to a window — they go to whatever holds the focus when they are
    /// sent — so the focus goes back first, and the pause after that covers the window manager
    /// taking its time over the request (see [`focus::ACTIVATION_PAUSE`]). Nothing here touches
    /// the UI thread: this runs on the network runtime.
    async fn deliver(&self, text: &str) {
        let (Some(focus), Some(injector)) = (self.focus.as_ref(), self.injector.as_ref()) else {
            warn!(target: LOG_ID, "Not typing {text:?}: no focus control or no input simulation.");
            return;
        };

        let target = *self.target.lock().unwrap();
        let Some(target) = target else {
            warn!(target: LOG_ID, "Not typing {text:?}: no window was captured at the hotkey.");
            return;
        };

        if let Err(error) = focus.activate(target) {
            // Still worth typing: the focus may well be in the right window already.
            warn!(target: LOG_ID, "Could not give the focus back to 0x{target:x} ({error}).");
        }

        tokio::time::sleep(focus::ACTIVATION_PAUSE).await;

        match injector.lock().unwrap().type_text(text) {
            Ok(()) => info!(target: LOG_ID, "Typed {text:?} into 0x{target:x}."),
            Err(error) => warn!(target: LOG_ID, "Could not type {text:?} ({error})."),
        }
    }
}

/// Hands one Chinese word to the translator and writes the English text that comes back into the
/// window the hotkey was pressed in.
///
/// The request is spawned onto the background runtime, so this returns straight away and the UI
/// thread never waits for the network, nor for the focus change that follows it.
fn spawn_translation(runtime: &tokio::runtime::Handle, delivery: Delivery, word: String) {
    let Some(translator) = delivery.translator.clone() else {
        warn!(target: LOG_ID, "Not translating {word}: no provider is configured.");
        return;
    };

    runtime.spawn(async move {
        match translator.translate(&word).await {
            Ok(english) => {
                // The translation lands in whatever the user is writing, so it is folded to
                // lowercase: a provider capitalises the first word of what it returns, and a
                // word typed in the middle of a line should not arrive capitalised.
                let english = english.to_lowercase();
                info!(target: LOG_ID, "Translation: {word} -> {english}.");
                delivery.deliver(&english).await;
            }
            Err(error) => warn!(target: LOG_ID, "Translation of {word} failed: {error}."),
        }
    });
}
