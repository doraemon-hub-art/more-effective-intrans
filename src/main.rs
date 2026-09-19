/*
 * @file main.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Process entry: load configuration, start the Tokio runtime and run the library
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::sync::Arc;

use more_effective_intrans::ingress::{hotkey, ui_bridge};
use more_effective_intrans::pipeline::dict;
use more_effective_intrans::pipeline::providers::traditional::BaiduTranslator;
use more_effective_intrans::pipeline::translator::Translator;
use more_effective_intrans::{AppWindow, init_logging};
use slint::ComponentHandle;
use tracing::{info, warn};

// Per-file logger id: it replaces the long module path in every log line, so a line
// immediately tells you which file it came from. Convention: UpperCamelCase version of
// the file's path under src/, wrapped in brackets.
const LOG_ID: &str = "[Main]";

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

    // Credentials come from the environment. Without them the flow stops after the lookup
    // instead of failing on every word, and the log says so once at startup.
    let translator: Option<Arc<BaiduTranslator>> = match BaiduTranslator::from_env() {
        Ok(translator) => Some(Arc::new(translator)),
        Err(error) => {
            warn!(target: LOG_ID, "Translation is off: {error}.");
            None
        }
    };

    let dictionary = dict::Dict::load();

    // Build the window described in ui/app_window.slint, then hand control to the Slint
    // event loop. run() blocks here and returns once the window is closed.
    let ui = AppWindow::new()?;
    info!(target: LOG_ID, "Window created.");

    // Bound to a name (rather than dropped) because dropping the manager unregisters
    // the hotkey again.
    let _hotkeys = hotkey::start();

    // The closures need their way back to the window, and a Weak handle keeps the component
    // from owning its own callbacks.
    let weak = ui.as_weak();
    let submit_network = network.clone();
    let submit_translator = translator.clone();
    ui_bridge::on_submit(&ui, move |input| {
        let candidates = dictionary.lookup(&input);
        match candidates.as_slice() {
            [] => info!(target: LOG_ID, "Lookup {input:?} matched nothing."),
            [only] => {
                info!(target: LOG_ID, "Lookup {input:?} -> {}, a single match.", only.word);
                spawn_translation(
                    &submit_network,
                    submit_translator.clone(),
                    only.word.clone(),
                );
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
    ui_bridge::on_pick(&ui, move |word| {
        spawn_translation(&network, translator.clone(), word);
        if let Some(ui) = weak.upgrade() {
            ui_bridge::set_candidates(&ui, &[]);
        }
    });

    info!(target: LOG_ID, "Entering the event loop.");
    let result = ui.run();
    info!(target: LOG_ID, "Event loop finished, shutting down.");

    result
}

/// Hands one Chinese word to the translator and logs the English text that comes back.
///
/// The request is spawned onto the background runtime, so this returns straight away and
/// the UI thread never waits for the network. The result shows up in the log when it is
/// ready — which is also why the delivery does not touch Slint yet.
fn spawn_translation(
    runtime: &tokio::runtime::Handle,
    translator: Option<Arc<BaiduTranslator>>,
    word: String,
) {
    let Some(translator) = translator else {
        warn!(target: LOG_ID, "Not translating {word}: no provider is configured.");
        return;
    };

    runtime.spawn(async move {
        match translator.translate(&word).await {
            Ok(english) => info!(target: LOG_ID, "Translation: {word} -> {english}."),
            Err(error) => warn!(target: LOG_ID, "Translation of {word} failed: {error}."),
        }
    });
}
