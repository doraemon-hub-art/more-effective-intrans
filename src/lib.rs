/*
 * @file lib.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Library entry: expose the layer modules and assemble them
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::io::IsTerminal;

pub mod coordinator;
pub mod error;
pub mod ingress;
pub mod pipeline;
pub mod platform;

// Pull in the Rust code that slint-build generated from ui/app_window.slint.
// It lives in the library (not in main.rs) so every layer can reach the window handle.
slint::include_modules!();

/// Initialise the global logger.
///
/// The level is taken from the `RUST_LOG` environment variable (for example
/// `RUST_LOG=debug cargo r`), and falls back to `info` when it is unset.
/// Every layer then just calls `tracing::info!` / `debug!` / `warn!` / `error!`
/// and the lines carry the module path, so it is obvious which file logged what.
pub fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // Colour only when writing to a terminal: redirected logs stay clean text.
    let ansi = std::io::stdout().is_terminal();

    // Local wall-clock time (RFC 3339 with a UTC offset) instead of the default UTC,
    // which needs the "local-time" feature of tracing-subscriber.
    let timer = tracing_subscriber::fmt::time::LocalTime::rfc_3339();

    // try_init instead of init: a second call (from a test, say) must not panic.
    let _ = tracing_subscriber::fmt()
        .with_ansi(ansi)
        .with_timer(timer)
        .with_env_filter(filter)
        .try_init();
}
