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
    /// A credential or setting that has to come from the environment is not set. The
    /// configuration file is the place for credentials; a variable is the fallback for a
    /// session that has them exported.
    #[error("environment variable {0} is not set")]
    MissingEnv(&'static str),

    /// The configuration file is there but cannot be read or parsed. The path is part of the
    /// message, since with two files in the search path it is the only thing that says which one.
    #[error("configuration error: {0}")]
    Config(String),

    /// Anything that went wrong on the wire: DNS, TLS, connect, timeout, reading the body.
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    /// A provider answered, but with its own error code instead of a translation.
    #[error("translation provider answered {code}: {message}")]
    Provider { code: String, message: String },

    /// A provider answered in a shape we do not understand — usually a changed contract.
    #[error("unexpected response from the translation provider: {0}")]
    UnexpectedResponse(String),

    /// The X server could not be reached at all: no `DISPLAY`, no socket, refused connection.
    #[error("X11 connection failed: {0}")]
    X11Connection(#[from] x11rb::errors::ConnectError),

    /// The X server was reached, but refused the request or broke the connection meanwhile.
    #[error("X11 request failed: {0}")]
    X11Request(#[from] x11rb::errors::ReplyOrIdError),

    /// Simulated input could not be set up — usually because there is no X11 display.
    #[error("input simulation is unavailable: {0}")]
    Injector(#[from] enigo::NewConError),

    /// Simulated input was refused. Nothing else to try: the text simply did not get typed.
    #[error("could not type the text: {0}")]
    Inject(#[from] enigo::InputError),
}

// The two finer X11 errors are folded into [`Error::X11Request`] by hand: a request fails with
// the first one while it is being sent, or with the second one once its reply is checked, and
// neither converts into the other on the way through `?`.
impl From<x11rb::errors::ConnectionError> for Error {
    fn from(error: x11rb::errors::ConnectionError) -> Self {
        Self::X11Request(error.into())
    }
}

impl From<x11rb::errors::ReplyError> for Error {
    fn from(error: x11rb::errors::ReplyError) -> Self {
        Self::X11Request(error.into())
    }
}

/// Shorthand used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
