/*
 * @file config.rs
 * @author doraemon-hub-art (1660219734@qq.com)
 * @brief Layer 2: the configuration file, read once at startup
 * @date 2026-09-27
 *
 * @copyright Copyright (c) 2026
 */

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{info, warn};

use crate::error::{Error, Result};

// Per-file logger id, same scheme as in main.rs.
const LOG_ID: &str = "[CoordinatorConfig]";

/// The user's own configuration, in the XDG configuration directory.
const USER_CONFIG: &str = "more-effective-intrans/config.toml";

/// The configuration that comes with the program, installed system wide.
const SYSTEM_CONFIG: &str = "/etc/more-effective-intrans/config.toml";

/// Everything the configuration file can say.
///
/// Unknown keys are ignored rather than refused: a file written for a newer version should not
/// stop an older one from starting over a key it will never look at.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub translation: Translation,
}

/// The translation settings: which source is used, and what each source needs.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Translation {
    /// The source the tray menu marks as the picked one, by the name of its section.
    pub selected: String,
    pub baidu: Baidu,
}

/// What the Baidu provider needs. Empty credentials mean "not configured", which is what turns
/// translation off rather than making every word fail.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Baidu {
    pub appid: String,
    pub secret: String,
    /// Empty means the provider's own public endpoint.
    pub endpoint: String,
}

impl Default for Translation {
    fn default() -> Self {
        Self {
            selected: DEFAULT_SOURCE.to_owned(),
            baidu: Baidu::default(),
        }
    }
}

/// The source used when the configuration file does not say, and the only one there is.
pub const DEFAULT_SOURCE: &str = "baidu";

/// The configuration template that ships inside the binary.
///
/// It is what a complete configuration file looks like, so it is what a user gets when the
/// program has to write one for them.
pub const TEMPLATE: &str = include_str!("../../config.default.toml");

/// The path of the user's own configuration file, whether or not it is there.
pub fn user_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| dir.join(USER_CONFIG))
}

/// Makes sure the user's configuration file exists, and returns its path.
///
/// A missing file is written from [`TEMPLATE`], since an editor opening a documentation example
/// beats one complaining about a path that is not there. A file that is already there is left
/// exactly as it is — this never overwrites a configuration.
pub fn ensure_user_file() -> Result<PathBuf> {
    let path = user_path()
        .ok_or_else(|| Error::Config("neither XDG_CONFIG_HOME nor HOME is set".to_owned()))?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| Error::Config(format!("{}: {error}", dir.display())))?;
    }

    if !path.exists() {
        std::fs::write(&path, TEMPLATE)
            .map_err(|error| Error::Config(format!("{}: {error}", path.display())))?;
        info!(target: LOG_ID, "Wrote a fresh configuration file to {}.", path.display());
    }

    Ok(path)
}

impl Config {
    /// Reads the configuration.
    ///
    /// The user file wins over the system file, and with neither of them there the built-in
    /// defaults are used — that is not an error, it just means no source is configured yet.
    /// A file that is there but cannot be read or parsed *is* an error: carrying on would look
    /// like "the credentials stopped working" instead of pointing at the typo.
    pub fn load() -> Result<Self> {
        for path in candidates() {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(Error::Config(format!("{}: {error}", path.display())));
                }
            };

            let config = Self::parse(&text, &path)?;
            info!(target: LOG_ID, "Read the configuration from {}.", path.display());
            return Ok(config);
        }

        info!(target: LOG_ID, "No configuration file found, using the defaults.");
        Ok(Self::default())
    }

    /// Parses one configuration text.
    ///
    /// Kept apart from reading it so the shape of the file can be tested without touching the
    /// file system.
    fn parse(text: &str, path: &Path) -> Result<Self> {
        toml::from_str(text).map_err(|error| Error::Config(format!("{}: {error}", path.display())))
    }
}

/// The files to try, most specific first.
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    match user_path() {
        Some(path) => paths.push(path),
        None => warn!(target: LOG_ID, "Neither XDG_CONFIG_HOME nor HOME is set."),
    }
    paths.push(PathBuf::from(SYSTEM_CONFIG));

    paths
}

/// The XDG configuration directory: `$XDG_CONFIG_HOME`, or `~/.config` when it is unset.
fn user_config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }

    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "config.default.toml";

    #[test]
    fn a_full_file_is_read_as_written() {
        let text = r#"
            [translation]
            selected = "baidu"

            [translation.baidu]
            appid = "the appid"
            secret = "the secret"
            endpoint = "https://example.invalid/"
        "#;

        let config = Config::parse(text, Path::new(PATH)).expect("it must parse");

        assert_eq!(config.translation.selected, "baidu");
        assert_eq!(config.translation.baidu.appid, "the appid");
        assert_eq!(config.translation.baidu.secret, "the secret");
        assert_eq!(
            config.translation.baidu.endpoint,
            "https://example.invalid/"
        );
    }

    #[test]
    fn an_empty_file_leaves_everything_at_its_default() {
        let config = Config::parse("", Path::new(PATH)).expect("an empty file must parse");

        assert_eq!(config.translation.selected, DEFAULT_SOURCE);
        assert_eq!(config.translation.baidu.appid, "");
        assert_eq!(config.translation.baidu.secret, "");
        assert_eq!(config.translation.baidu.endpoint, "");
    }

    #[test]
    fn a_file_may_say_no_more_than_it_needs_to() {
        let text = r#"
            [translation.baidu]
            appid = "the appid"
        "#;

        let config = Config::parse(text, Path::new(PATH)).expect("it must parse");

        // A missing section still counts as "the default source", so an unconfigured provider
        // is reported as a missing credential rather than as a broken file.
        assert_eq!(config.translation.selected, DEFAULT_SOURCE);
        assert_eq!(config.translation.baidu.appid, "the appid");
        assert_eq!(config.translation.baidu.secret, "");
    }

    #[test]
    fn the_empty_settings_table_the_template_ships_with_is_accepted() {
        let text = "[translation]\nselected = \"baidu\"\n\n[app]\n";

        let config = Config::parse(text, Path::new(PATH)).expect("the template must parse");

        assert_eq!(config.translation.selected, "baidu");
    }

    #[test]
    fn the_template_that_ships_inside_the_binary_parses() {
        // The template is what a user's file is written from, so it has to match the structs
        // above: this is what catches the two drifting apart.
        let config = Config::parse(TEMPLATE, Path::new("config.default.toml"))
            .expect("the shipped template must parse");

        assert_eq!(config.translation.selected, DEFAULT_SOURCE);
    }

    #[test]
    fn a_file_that_makes_no_sense_is_an_error() {
        let error = Config::parse("this is not toml", Path::new(PATH)).expect_err("it must fail");

        let message = error.to_string();
        assert!(
            message.contains(PATH),
            "the message must name the file: {message}"
        );
    }
}
