/*
 * @file error.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Unified error enum shared by every layer
 * @date 2026-09-19
 *
 * @copyright Copyright (c) 2026
 */

use thiserror::Error;

/// Every failure the program can report, in one place: layers map their own error
/// vocabulary onto these variants so callers never have to know which crate failed.
#[derive(Debug, Error)]
pub enum Error {
    /// A credential or setting that has to come from the environment is not set. Keys live
    /// there on purpose, never in a config file.
    #[error("environment variable {0} is not set")]
    MissingEnv(&'static str),

    /// Anything that went wrong on the wire: DNS, TLS, connect, timeout, reading the body.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// A provider answered, but with its own error code instead of a translation.
    #[error("translation provider answered {code}: {message}")]
    Provider { code: String, message: String },

    /// A provider answered in a shape we do not understand — usually a changed contract.
    #[error("unexpected response from the translation provider: {0}")]
    UnexpectedResponse(String),
}

/// Shorthand used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
