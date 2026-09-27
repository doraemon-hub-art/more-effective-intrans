/*
 * @file lib.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Library entry: expose the layer modules and assemble them
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use std::io::IsTerminal;
use std::path::PathBuf;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub mod coordinator;
pub mod error;
pub mod ingress;
pub mod pipeline;
pub mod platform;

// Pull in the Rust code that slint-build generated from ui/app_window.slint.
// It lives in the library (not in main.rs) so every layer can reach the window handle.
slint::include_modules!();

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[Lib]";

/// The log of the run going on, under the XDG state directory.
const LOG_FILE: &str = "more-effective-intrans/last-run.log";

/// The log file of this run, open and truncated.
struct LogFile {
    path: PathBuf,
    file: std::fs::File,
}

/// Initialise the global logger.
///
/// Two destinations carry the same lines: the terminal, with colour when it is one, and the log
/// file of this run, never with colour since it is read in an editor rather than in a terminal.
/// The level is taken from the `RUST_LOG` environment variable (for example
/// `RUST_LOG=debug cargo r`), and falls back to `info` when it is unset.
/// Every layer then just calls `tracing::info!` / `debug!` / `warn!` / `error!`
/// and the lines carry the module path, so it is obvious which file logged what.
pub fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // Colour only when writing to a terminal: redirected logs stay clean text.
    let ansi = std::io::stdout().is_terminal();
    let to_terminal = || {
        tracing_subscriber::fmt::layer()
            .with_ansi(ansi)
            .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
            .with_writer(std::io::stdout)
    };

    // Local wall-clock time (RFC 3339 with a UTC offset) instead of the default UTC,
    // which needs the "local-time" feature of tracing-subscriber.
    match open_log_file() {
        Ok(log) => {
            let to_file = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
                .with_writer(log.file);

            // try_init instead of init: a second call (from a test, say) must not panic.
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(to_terminal())
                .with(to_file)
                .try_init();

            tracing::info!(target: LOG_ID, "This run logs to {}.", log.path.display());
        }
        Err(reason) => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(to_terminal())
                .try_init();

            tracing::warn!(target: LOG_ID, "Logging to the terminal only: {reason}");
        }
    }
}

/// Opens the log file of this run, empty, creating the directory it lives in.
///
/// Creating the file rather than appending to it is the point of the whole thing: the file holds
/// the latest run and nothing older.
fn open_log_file() -> Result<LogFile, String> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or_else(|| "neither XDG_STATE_HOME nor HOME is set".to_owned())?;

    let path = base.join(LOG_FILE);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    }

    let file = std::fs::File::create(&path)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;

    Ok(LogFile { path, file })
}
